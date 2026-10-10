//! Decoding a command payload into a handler's payload struct.

use application::error::{AppError, FieldCode, FieldError};
use serde::de::DeserializeOwned;

/// Decodes `payload` into `T`: a `#[serde(default)]` struct of `application::input::Field<T>`
/// fields plus `#[serde(flatten)] unknown: UnknownFields` (kafka.md).
///
/// Such a struct never fails on a bad field (the application parser reports it), so this only
/// fails when the payload is not an object. The envelope already guarantees an object, so the
/// error is a safety net, reported as `payload` is `invalid_type`.
pub fn decode<T: DeserializeOwned>(payload: serde_json::Value) -> Result<T, AppError> {
    serde_json::from_value(payload)
        .map_err(|_| AppError::Validation(vec![FieldError::of("payload", FieldCode::InvalidType)]))
}

#[cfg(test)]
mod tests {
    use application::input::{Field, UnknownFields};
    use serde::Deserialize;
    use serde_json::json;

    use super::*;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Dto {
        id: Field<String>,
        name: Field<String>,
        #[serde(flatten)]
        unknown: UnknownFields,
    }

    #[test]
    fn decodes_present_wrong_type_and_unknown_fields_without_failing() {
        let dto: Dto = decode(json!({"id": "a", "name": 5, "extra": true})).unwrap();
        assert_eq!(dto.id, Field::Present("a".to_owned()));
        assert_eq!(dto.name, Field::Invalid);
        assert_eq!(dto.unknown, UnknownFields(vec!["extra".to_owned()]));
    }

    #[test]
    fn a_payload_that_is_not_an_object_is_a_validation_error() {
        let AppError::Validation(errors) = decode::<Dto>(json!([1])).unwrap_err() else {
            panic!("expected validation");
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "payload");
        assert_eq!(errors[0].code, FieldCode::InvalidType);
    }
}
