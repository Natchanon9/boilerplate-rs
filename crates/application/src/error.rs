//! Errors every use case returns. Each inbound adapter maps them on its own terms
//! (stack-architecture.md).

use std::fmt;

/// The result of a use case.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    /// A unique business key is already taken.
    #[error("conflict: {0}")]
    Conflict(String),
    /// Every failing input field, collected by the input parser.
    #[error("validation failed: {} field error(s)", .0.len())]
    Validation(Vec<FieldError>),
    /// A dependency is down. Retrying may help.
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    /// Anything unexpected, such as a row that fails to decode. Retrying will not help.
    #[error("internal error: {0}")]
    Internal(String),
}

/// One failing input field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub code: FieldCode,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, code: FieldCode, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            code,
            message: message.into(),
        }
    }

    /// An error with the code's default message.
    pub fn of(field: impl Into<String>, code: FieldCode) -> Self {
        Self::new(field, code, code.default_message())
    }
}

/// The error codes of a field error (api.md AC-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldCode {
    /// Missing or `null`.
    Required,
    UnknownField,
    /// Wrong JSON type, including a fractional or out-of-range number for an integer.
    InvalidType,
    InvalidFormat,
    TooShort,
    TooLong,
    OutOfRange,
    Immutable,
}

impl FieldCode {
    pub fn as_str(self) -> &'static str {
        match self {
            FieldCode::Required => "required",
            FieldCode::UnknownField => "unknown_field",
            FieldCode::InvalidType => "invalid_type",
            FieldCode::InvalidFormat => "invalid_format",
            FieldCode::TooShort => "too_short",
            FieldCode::TooLong => "too_long",
            FieldCode::OutOfRange => "out_of_range",
            FieldCode::Immutable => "immutable",
        }
    }

    pub fn default_message(self) -> &'static str {
        match self {
            FieldCode::Required => "is required",
            FieldCode::UnknownField => "is not a known field",
            FieldCode::InvalidType => "has the wrong type",
            FieldCode::InvalidFormat => "has an invalid format",
            FieldCode::TooShort => "is too short",
            FieldCode::TooLong => "is too long",
            FieldCode::OutOfRange => "is out of range",
            FieldCode::Immutable => "cannot be changed",
        }
    }
}

impl fmt::Display for FieldCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
