//! Raw input shared by every inbound adapter, and the helpers parsers use to collect every
//! field error (api.md AC-8).
//!
//! serde stops at the first error, so adapters never deserialize a field into its final type.
//! Each field is a [`Field<T>`], unknown fields land in [`UnknownFields`], and the resource's
//! parser (`<res>/commands.rs`) turns them into domain values, collecting every failure in
//! [`FieldErrors`].

use uuid::Uuid;

use crate::error::{AppError, FieldCode, FieldError};

/// One raw input field. Deserializing it never fails.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Field<T> {
    /// The field is absent.
    #[default]
    Missing,
    /// The field is `null`.
    Null,
    /// The field has the wrong JSON type, or a number that does not fit `T`.
    Invalid,
    Present(T),
}

impl<T> Field<T> {
    /// The value of a required field. Records `required` (missing or `null`) or
    /// `invalid_type`, and returns `None`, otherwise.
    pub fn required(self, name: &str, errors: &mut FieldErrors) -> Option<T> {
        match self {
            Field::Present(value) => Some(value),
            Field::Missing | Field::Null => {
                errors.push(FieldError::of(name, FieldCode::Required));
                None
            }
            Field::Invalid => {
                errors.push(FieldError::of(name, FieldCode::InvalidType));
                None
            }
        }
    }

    /// The value of an optional field: `Some(None)` when missing or `null`. Records
    /// `invalid_type`, and returns `None`, for a wrong type.
    pub fn optional(self, name: &str, errors: &mut FieldErrors) -> Option<Option<T>> {
        match self {
            Field::Present(value) => Some(Some(value)),
            Field::Missing | Field::Null => Some(None),
            Field::Invalid => {
                errors.push(FieldError::of(name, FieldCode::InvalidType));
                None
            }
        }
    }
}

/// Names of fields the input does not define, in order. The input is an allowlist, so each
/// one is an `unknown_field` error (security.md SEC-4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnknownFields(pub Vec<String>);

/// Whether a text field may span lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lines {
    /// Rejects every control character (Unicode category Cc).
    Single,
    /// Allows `\n`, `\r`, and `\t`, and rejects every other control character.
    Multi,
}

/// `false` when the text holds U+0000 (PostgreSQL text cannot store it) or a control
/// character the line rule forbids.
pub fn text_is_allowed(text: &str, lines: Lines) -> bool {
    text.chars().all(|c| match lines {
        Lines::Single => !c.is_control(),
        Lines::Multi => !c.is_control() || matches!(c, '\n' | '\r' | '\t'),
    })
}

/// Parses a producer-supplied id. Only a UUIDv7 passes: ids are time-ordered, and the list's
/// newest-first order depends on it (kafka.md). Text
/// that is not a UUID, and any other version (including the nil and max UUIDs), gives
/// `invalid_format`. A path id that is only looked up needs `Uuid::parse_str`, not this.
pub fn uuid_v7(field: &str, text: &str) -> Result<Uuid, FieldError> {
    match Uuid::parse_str(text) {
        Ok(id) if id.get_version_num() == 7 => Ok(id),
        _ => Err(FieldError::new(
            field,
            FieldCode::InvalidFormat,
            "must be a UUIDv7",
        )),
    }
}

/// Every field error found while parsing one input.
#[derive(Debug, Default)]
pub struct FieldErrors(Vec<FieldError>);

impl FieldErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, error: FieldError) {
        self.0.push(error);
    }

    /// Records `unknown_field` for each unknown field.
    pub fn unknown(&mut self, unknown: &UnknownFields) {
        for name in &unknown.0 {
            self.push(FieldError::of(name.as_str(), FieldCode::UnknownField));
        }
    }

    /// Records `invalid_format` when the text breaks the control-character rule, and
    /// returns whether it is allowed.
    pub fn text(&mut self, name: &str, text: &str, lines: Lines) -> bool {
        let allowed = text_is_allowed(text, lines);
        if !allowed {
            self.push(FieldError::new(
                name,
                FieldCode::InvalidFormat,
                "contains a control character that is not allowed",
            ));
        }
        allowed
    }

    /// The value when `result` is `Ok`; otherwise records the error and returns `None`.
    pub fn check<T>(&mut self, result: Result<T, FieldError>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.push(error);
                None
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `AppError::Validation` with every collected error. Parsers call it when any field
    /// failed, so the list is never empty.
    pub fn into_error(self) -> AppError {
        AppError::Validation(self.0)
    }
}

#[cfg(feature = "serde")]
mod serde_impls {
    use std::fmt;

    use serde::de::{DeserializeOwned, IgnoredAny, MapAccess, Visitor};
    use serde::{Deserialize, Deserializer};

    use super::{Field, UnknownFields};

    impl<'de, T: DeserializeOwned> Deserialize<'de> for Field<T> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = serde_json::Value::deserialize(deserializer)?;
            if value.is_null() {
                return Ok(Field::Null);
            }
            Ok(serde_json::from_value(value).map_or(Field::Invalid, Field::Present))
        }
    }

    impl<'de> Deserialize<'de> for UnknownFields {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Keys;

            impl<'de> Visitor<'de> for Keys {
                type Value = UnknownFields;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a map of unknown fields")
                }

                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                    let mut names = Vec::new();
                    while let Some((name, IgnoredAny)) = map.next_entry::<String, IgnoredAny>()? {
                        names.push(name);
                    }
                    Ok(UnknownFields(names))
                }
            }

            deserializer.deserialize_map(Keys)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_reports_missing_null_and_wrong_type() {
        let mut errors = FieldErrors::new();
        assert_eq!(Field::<i64>::Missing.required("a", &mut errors), None);
        assert_eq!(Field::<i64>::Null.required("b", &mut errors), None);
        assert_eq!(Field::<i64>::Invalid.required("c", &mut errors), None);
        assert_eq!(Field::Present(5).required("d", &mut errors), Some(5));
        let AppError::Validation(list) = errors.into_error() else {
            panic!("expected validation");
        };
        let codes: Vec<_> = list.iter().map(|e| (e.field.as_str(), e.code)).collect();
        assert_eq!(
            codes,
            [
                ("a", FieldCode::Required),
                ("b", FieldCode::Required),
                ("c", FieldCode::InvalidType)
            ]
        );
    }

    #[test]
    fn optional_treats_missing_and_null_as_none() {
        let mut errors = FieldErrors::new();
        assert_eq!(
            Field::<String>::Missing.optional("a", &mut errors),
            Some(None)
        );
        assert_eq!(Field::<String>::Null.optional("a", &mut errors), Some(None));
        assert_eq!(Field::<String>::Invalid.optional("a", &mut errors), None);
        assert!(!errors.is_empty());
    }

    #[test]
    fn text_rules_follow_the_line_kind() {
        assert!(text_is_allowed("สวัสดี ok", Lines::Single));
        assert!(!text_is_allowed("a\nb", Lines::Single));
        assert!(text_is_allowed("a\nb\r\n\tc", Lines::Multi));
        assert!(!text_is_allowed("a\u{0}b", Lines::Multi));
        assert!(!text_is_allowed("a\u{7f}b", Lines::Multi));
        assert!(!text_is_allowed("a\u{1b}b", Lines::Single));
    }

    #[test]
    fn uuid_v7_accepts_only_version_7() {
        let v7 = Uuid::now_v7();
        assert_eq!(uuid_v7("id", &v7.to_string()), Ok(v7));

        let rejected = [
            "550e8400-e29b-41d4-a716-446655440000", // v4
            "00000000-0000-0000-0000-000000000000", // nil
            "ffffffff-ffff-ffff-ffff-ffffffffffff", // max
            "not-a-uuid",
            "",
        ];
        for text in rejected {
            let error = uuid_v7("id", text).unwrap_err();
            assert_eq!(error.field, "id", "{text}");
            assert_eq!(error.code, FieldCode::InvalidFormat, "{text}");
        }
    }

    #[test]
    fn unknown_fields_are_each_reported() {
        let mut errors = FieldErrors::new();
        errors.unknown(&UnknownFields(vec!["x".into(), "y".into()]));
        let AppError::Validation(list) = errors.into_error() else {
            panic!("expected validation");
        };
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|e| e.code == FieldCode::UnknownField));
    }

    #[cfg(feature = "serde")]
    mod serde {
        use serde::Deserialize;

        use super::super::*;

        #[derive(Debug, Default, Deserialize)]
        #[serde(default)]
        struct Dto {
            name: Field<String>,
            price: Field<i64>,
            note: Field<String>,
            #[serde(flatten)]
            unknown: UnknownFields,
        }

        fn dto(json: &str) -> Dto {
            serde_json::from_str(json).unwrap()
        }

        #[test]
        fn every_field_state_is_captured_without_failing() {
            let d = dto(r#"{"name": 5, "price": 1.5, "note": null, "x": 1, "y": [2]}"#);
            assert_eq!(d.name, Field::Invalid);
            assert_eq!(d.price, Field::Invalid);
            assert_eq!(d.note, Field::Null);
            assert_eq!(d.unknown, UnknownFields(vec!["x".into(), "y".into()]));
        }

        #[test]
        fn missing_present_and_out_of_range_numbers() {
            let d = dto(r#"{"name": "a", "price": 99999999999999999999}"#);
            assert_eq!(d.name, Field::Present("a".to_owned()));
            assert_eq!(d.price, Field::Invalid);
            assert_eq!(d.note, Field::Missing);
            assert_eq!(d.unknown, UnknownFields::default());
        }
    }
}
