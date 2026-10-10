//! The strict command envelope (kafka.md).
//!
//! All six fields are required. The parser reads the JSON into a `serde_json::Value` first,
//! so each failure has its own cause and nothing about the payload reaches an error message.

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::reason::{DlqReason, Rejection};

/// The only `schema_version` the consumer accepts (own proposal).
pub const SCHEMA_VERSION: u64 = 1;

/// The canonical text form of a UUID: 8-4-4-4-12 hex digits with hyphens.
const UUID_TEXT_LEN: usize = 36;

/// A command envelope that passed every check.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub message_id: Uuid,
    pub correlation_id: Uuid,
    /// `<res>.<op>`, such as `widget.create`.
    pub message_type: String,
    pub occurred_at: OffsetDateTime,
    /// A JSON object whose `id` is a string equal to the message key.
    pub payload: Value,
}

/// Parses the message value and checks the key. A failure is the [`Rejection`] to dead-letter:
/// `malformed_json` when the value is not JSON, `invalid_envelope` for anything else wrong
/// with the envelope or the key.
pub fn parse(key: Option<&[u8]>, value: Option<&[u8]>) -> Result<Envelope, Rejection> {
    let Some(value) = value else {
        return Err(Rejection::new(
            DlqReason::MalformedJson,
            "the message has no value",
            None,
        ));
    };
    let json: Value = serde_json::from_slice(value).map_err(|err| {
        Rejection::new(
            DlqReason::MalformedJson,
            format!("the value is not valid JSON: {err}"),
            None,
        )
    })?;
    let Value::Object(mut object) = json else {
        return Err(invalid("the value is not a JSON object", None));
    };

    let message_id = uuid_field(object.get("message_id"), "message_id", None)?;
    // From here on every rejection carries the message id, for the log.
    let id = Some(message_id);
    let correlation_id = uuid_field(object.get("correlation_id"), "correlation_id", id)?;
    let message_type = match object.get("type") {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        _ => return Err(invalid("`type` is not a non-empty string", id)),
    };
    match object.get("schema_version").and_then(Value::as_u64) {
        Some(SCHEMA_VERSION) => {}
        _ => {
            return Err(invalid(
                format!("`schema_version` is not {SCHEMA_VERSION}"),
                id,
            ));
        }
    }
    let occurred_at = match object.get("occurred_at") {
        Some(Value::String(text)) => OffsetDateTime::parse(text, &Rfc3339)
            .map_err(|_| invalid("`occurred_at` is not an RFC 3339 date-time", id))?,
        _ => return Err(invalid("`occurred_at` is not a string", id)),
    };
    let payload = match object.remove("payload") {
        Some(payload @ Value::Object(_)) => payload,
        _ => return Err(invalid("`payload` is not a JSON object", id)),
    };

    let Some(key) = key else {
        return Err(invalid("the message has no key", id));
    };
    match payload.get("id") {
        Some(Value::String(payload_id)) if payload_id.as_bytes() == key => {}
        Some(Value::String(_)) => return Err(invalid("the key differs from `payload.id`", id)),
        _ => return Err(invalid("`payload.id` is not a string", id)),
    }

    Ok(Envelope {
        message_id,
        correlation_id,
        message_type,
        occurred_at,
        payload,
    })
}

fn invalid(cause: impl Into<String>, message_id: Option<Uuid>) -> Rejection {
    Rejection::new(DlqReason::InvalidEnvelope, cause, message_id)
}

fn uuid_field(
    value: Option<&Value>,
    name: &str,
    message_id: Option<Uuid>,
) -> Result<Uuid, Rejection> {
    match value {
        Some(Value::String(text)) if text.len() == UUID_TEXT_LEN => Uuid::parse_str(text)
            .map_err(|_| invalid(format!("`{name}` is not a UUID"), message_id)),
        _ => Err(invalid(format!("`{name}` is not a UUID"), message_id)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ID: &str = "0199c0aa-0000-7000-8000-000000000001";

    fn valid() -> Value {
        json!({
            "message_id": "0199c0aa-1111-7000-8000-000000000002",
            "correlation_id": "0199c0aa-2222-7000-8000-000000000003",
            "type": "product.create",
            "schema_version": 1,
            "occurred_at": "2026-10-09T10:00:00Z",
            "payload": {"id": ID, "name": "Pen"},
        })
    }

    fn parse_json(value: &Value, key: Option<&str>) -> Result<Envelope, Rejection> {
        parse(key.map(str::as_bytes), Some(value.to_string().as_bytes()))
    }

    /// `valid()` with one field replaced, or removed when `replacement` is `None`.
    fn with(field: &str, replacement: Option<Value>) -> Value {
        let mut envelope = valid();
        let Some(object) = envelope.as_object_mut() else {
            panic!("the valid envelope is an object");
        };
        match replacement {
            Some(value) => object.insert(field.to_owned(), value),
            None => object.remove(field),
        };
        envelope
    }

    fn reason_of(value: &Value, key: Option<&str>) -> DlqReason {
        parse_json(value, key).unwrap_err().reason
    }

    #[test]
    fn a_valid_envelope_is_accepted() {
        let envelope = parse_json(&valid(), Some(ID)).unwrap();
        assert_eq!(envelope.message_type, "product.create");
        assert_eq!(envelope.payload["name"], "Pen");
        assert_eq!(
            envelope.message_id.to_string(),
            "0199c0aa-1111-7000-8000-000000000002"
        );
        assert_eq!(envelope.occurred_at.year(), 2026);
    }

    #[test]
    fn a_value_that_is_not_json_is_malformed_json() {
        for bytes in [&b"not json"[..], b"", b"{\"a\":", b"\xff\xfe"] {
            let rejection = parse(Some(ID.as_bytes()), Some(bytes)).unwrap_err();
            assert_eq!(rejection.reason, DlqReason::MalformedJson, "{bytes:?}");
            assert_eq!(rejection.message_id, None);
        }
    }

    #[test]
    fn a_message_without_a_value_is_malformed_json() {
        let rejection = parse(Some(ID.as_bytes()), None).unwrap_err();
        assert_eq!(rejection.reason, DlqReason::MalformedJson);
    }

    #[test]
    fn json_that_is_not_an_object_is_an_invalid_envelope() {
        for value in [json!([1]), json!("text"), json!(5), json!(null)] {
            assert_eq!(reason_of(&value, Some(ID)), DlqReason::InvalidEnvelope);
        }
    }

    #[test]
    fn each_missing_field_is_an_invalid_envelope() {
        for field in [
            "message_id",
            "correlation_id",
            "type",
            "schema_version",
            "occurred_at",
            "payload",
        ] {
            assert_eq!(
                reason_of(&with(field, None), Some(ID)),
                DlqReason::InvalidEnvelope,
                "missing {field}"
            );
        }
    }

    #[test]
    fn a_bad_field_is_an_invalid_envelope() {
        let cases = [
            ("message_id", json!("not-a-uuid")),
            ("message_id", json!(5)),
            (
                "message_id",
                json!("urn:uuid:0199c0aa-1111-7000-8000-000000000002"),
            ),
            (
                "correlation_id",
                json!("0199c0aa-2222-7000-8000-00000000000"),
            ),
            ("correlation_id", json!(null)),
            ("type", json!("")),
            ("type", json!(7)),
            ("schema_version", json!(2)),
            ("schema_version", json!(0)),
            ("schema_version", json!("1")),
            ("schema_version", json!(1.5)),
            ("schema_version", json!(null)),
            ("occurred_at", json!("yesterday")),
            ("occurred_at", json!("2026-10-09 10:00:00")),
            ("occurred_at", json!(1_760_000_000)),
            ("payload", json!([])),
            ("payload", json!("text")),
            ("payload", json!(null)),
        ];
        for (field, value) in cases {
            assert_eq!(
                reason_of(&with(field, Some(value.clone())), Some(ID)),
                DlqReason::InvalidEnvelope,
                "{field} = {value}"
            );
        }
    }

    #[test]
    fn a_missing_key_is_an_invalid_envelope() {
        assert_eq!(reason_of(&valid(), None), DlqReason::InvalidEnvelope);
    }

    #[test]
    fn a_key_that_differs_from_the_payload_id_is_an_invalid_envelope() {
        let other = "0199c0aa-0000-7000-8000-0000000000ff";
        assert_eq!(reason_of(&valid(), Some(other)), DlqReason::InvalidEnvelope);
        assert_eq!(reason_of(&valid(), Some("")), DlqReason::InvalidEnvelope);
    }

    #[test]
    fn a_payload_without_a_string_id_is_an_invalid_envelope() {
        for payload in [
            json!({"name": "Pen"}),
            json!({"id": 5}),
            json!({"id": null}),
        ] {
            assert_eq!(
                reason_of(&with("payload", Some(payload)), Some(ID)),
                DlqReason::InvalidEnvelope
            );
        }
    }

    #[test]
    fn a_rejection_after_the_message_id_carries_it_for_the_log() {
        let rejection = parse_json(&valid(), None).unwrap_err();
        assert_eq!(
            rejection.message_id.map(|id| id.to_string()),
            Some("0199c0aa-1111-7000-8000-000000000002".to_owned())
        );
    }

    #[test]
    fn causes_and_details_never_contain_payload_values() {
        let envelope = with("payload", Some(json!({"id": "x", "secret": "hunter2"})));
        let rejection = parse_json(&envelope, Some(ID)).unwrap_err();
        assert!(!rejection.cause.contains("hunter2"));
        assert!(!rejection.detail.contains("hunter2"));
    }
}
