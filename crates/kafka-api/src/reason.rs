//! The DLQ `error.reason` codes and their `error.detail` text (kafka.md AC-3).

use application::error::AppError;
use serde_json::json;
use uuid::Uuid;

/// Header that carries the [`DlqReason`] code.
pub const REASON_HEADER: &str = "error.reason";
/// Header that carries the detail: field errors for `validation`, a fixed sentence otherwise.
pub const DETAIL_HEADER: &str = "error.detail";

/// Why a message went to the DLQ. The set is closed: consumers of the DLQ match on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DlqReason {
    MalformedJson,
    InvalidEnvelope,
    UnknownType,
    Validation,
    NotFound,
    Conflict,
    Unavailable,
    Internal,
}

impl DlqReason {
    /// The value of the `error.reason` header.
    pub fn as_str(self) -> &'static str {
        match self {
            DlqReason::MalformedJson => "malformed_json",
            DlqReason::InvalidEnvelope => "invalid_envelope",
            DlqReason::UnknownType => "unknown_type",
            DlqReason::Validation => "validation",
            DlqReason::NotFound => "not_found",
            DlqReason::Conflict => "conflict",
            DlqReason::Unavailable => "unavailable",
            DlqReason::Internal => "internal",
        }
    }

    /// The fixed `error.detail` sentence. It never names an internal cause.
    /// `validation` has no fixed sentence: its detail is the field errors.
    fn fixed_detail(self) -> &'static str {
        match self {
            DlqReason::MalformedJson => "the message value is not valid JSON",
            DlqReason::InvalidEnvelope => "the envelope is invalid",
            DlqReason::UnknownType => "no handler accepts this type on this topic",
            DlqReason::Validation => "the payload failed validation",
            DlqReason::NotFound => "the entity was not found",
            DlqReason::Conflict => "the command conflicts with the current state",
            DlqReason::Unavailable => "a dependency was unavailable",
            DlqReason::Internal => "an internal error occurred",
        }
    }
}

impl From<&AppError> for DlqReason {
    fn from(error: &AppError) -> Self {
        match error {
            AppError::Validation(_) => DlqReason::Validation,
            AppError::NotFound => DlqReason::NotFound,
            AppError::Conflict(_) => DlqReason::Conflict,
            AppError::Unavailable(_) => DlqReason::Unavailable,
            AppError::Internal(_) | AppError::Unauthorized | AppError::Forbidden => {
                DlqReason::Internal
            }
        }
    }
}

/// A message the consumer will not process. It becomes a DLQ message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub reason: DlqReason,
    /// The `error.detail` header value.
    pub detail: String,
    /// What went wrong, for the log only. Never put it in a header, and never put a payload
    /// value in it.
    pub cause: String,
    /// The envelope's `message_id`, when it was read before the rejection.
    pub message_id: Option<Uuid>,
}

impl Rejection {
    /// A rejection whose detail is the reason's fixed sentence.
    pub fn new(reason: DlqReason, cause: impl Into<String>, message_id: Option<Uuid>) -> Self {
        Self {
            reason,
            detail: reason.fixed_detail().to_owned(),
            cause: cause.into(),
            message_id,
        }
    }

    /// The rejection for a use case failure.
    /// A validation failure carries its field errors as the detail.
    pub fn from_app_error(error: &AppError, message_id: Uuid) -> Self {
        let reason = DlqReason::from(error);
        let mut rejection = Self::new(reason, error.to_string(), Some(message_id));
        if let AppError::Validation(errors) = error {
            let fields: Vec<_> = errors
                .iter()
                .map(|e| json!({"field": e.field, "code": e.code.as_str(), "message": e.message}))
                .collect();
            rejection.detail = serde_json::Value::Array(fields).to_string();
        }
        rejection
    }
}

#[cfg(test)]
mod tests {
    use application::error::{FieldCode, FieldError};

    use super::*;

    #[test]
    fn app_errors_map_to_reason_codes() {
        let cases = [
            (AppError::Validation(vec![]), "validation"),
            (AppError::NotFound, "not_found"),
            (AppError::Conflict("sku taken".into()), "conflict"),
            (AppError::Unavailable("db down".into()), "unavailable"),
            (AppError::Internal("bad row".into()), "internal"),
            (AppError::Unauthorized, "internal"),
            (AppError::Forbidden, "internal"),
        ];
        for (error, code) in cases {
            assert_eq!(DlqReason::from(&error).as_str(), code, "{error}");
        }
    }

    #[test]
    fn reason_codes_are_the_documented_set() {
        let all = [
            DlqReason::MalformedJson,
            DlqReason::InvalidEnvelope,
            DlqReason::UnknownType,
            DlqReason::Validation,
            DlqReason::NotFound,
            DlqReason::Conflict,
            DlqReason::Unavailable,
            DlqReason::Internal,
        ];
        let codes: Vec<_> = all.iter().map(|r| r.as_str()).collect();
        assert_eq!(
            codes,
            [
                "malformed_json",
                "invalid_envelope",
                "unknown_type",
                "validation",
                "not_found",
                "conflict",
                "unavailable",
                "internal"
            ]
        );
    }

    #[test]
    fn validation_detail_is_the_field_errors_as_json() {
        let error = AppError::Validation(vec![
            FieldError::of("name", FieldCode::Required),
            FieldError::new("price", FieldCode::OutOfRange, "must be at most 10"),
        ]);
        let rejection = Rejection::from_app_error(&error, Uuid::nil());
        let detail: serde_json::Value = serde_json::from_str(&rejection.detail).unwrap();
        assert_eq!(
            detail,
            json!([
                {"field": "name", "code": "required", "message": "is required"},
                {"field": "price", "code": "out_of_range", "message": "must be at most 10"},
            ])
        );
    }

    #[test]
    fn other_details_are_fixed_sentences_without_the_cause() {
        for error in [
            AppError::Conflict("sku ABC-1 is taken".into()),
            AppError::Unavailable("connection refused to 10.0.0.5".into()),
            AppError::Internal("column price: invalid digit".into()),
        ] {
            let rejection = Rejection::from_app_error(&error, Uuid::nil());
            assert_eq!(rejection.detail, rejection.reason.fixed_detail());
            assert!(rejection.cause.contains(&error.to_string()));
            for secret in ["ABC-1", "10.0.0.5", "invalid digit"] {
                assert!(!rejection.detail.contains(secret), "{}", rejection.detail);
            }
        }
    }
}
