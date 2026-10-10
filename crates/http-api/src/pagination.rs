//! List query parameters and the paged response (api.md AC-4).
//!
//! `limit` and `cursor` are the only query parameters. Every problem is collected into one
//! `422`: `invalid_type` for a non-integer `limit`, `out_of_range` outside 1 to 100,
//! `invalid_format` for a bad cursor, and `unknown_field` for any other parameter.

use application::error::{FieldCode, FieldError};
use application::input::FieldErrors;
use application::pagination::{Page, PageRequest};
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;
use uuid::Uuid;

use crate::error::ApiError;

/// The opaque `next_cursor`: the last `id` of a page, base64url-encoded without padding.
/// Unsigned, because it is not secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor(pub Uuid);

impl Cursor {
    pub fn encode(self) -> String {
        URL_SAFE_NO_PAD.encode(self.0.as_bytes())
    }

    pub fn decode(text: &str) -> Option<Self> {
        let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
        Uuid::from_slice(&bytes).ok().map(Cursor)
    }
}

/// The validated `limit` and `cursor` of a list request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListQuery(pub PageRequest);

impl<S: Send + Sync> FromRequestParts<S> for ListQuery {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(pairs) = Query::<Vec<(String, String)>>::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                ApiError::validation(vec![FieldError::of("query", FieldCode::InvalidFormat)])
            })?;
        parse(&pairs).map(ListQuery).map_err(ApiError::from)
    }
}

fn parse(pairs: &[(String, String)]) -> Result<PageRequest, application::error::AppError> {
    let mut errors = FieldErrors::new();
    let mut limit = Some(None);
    let mut cursor = None;
    for (name, value) in pairs {
        match name.as_str() {
            "limit" => match value.parse::<i64>() {
                // Any integer is the right type; PageRequest reports one outside 1 to 100.
                Ok(n) => {
                    limit = Some(Some(u32::try_from(n).unwrap_or(if n < 0 {
                        0
                    } else {
                        u32::MAX
                    })))
                }
                Err(_) => {
                    limit = None;
                    errors.push(FieldError::new(
                        "limit",
                        FieldCode::InvalidType,
                        "must be an integer",
                    ));
                }
            },
            "cursor" => match Cursor::decode(value) {
                Some(Cursor(id)) => cursor = Some(id),
                None => errors.push(FieldError::of("cursor", FieldCode::InvalidFormat)),
            },
            other => errors.push(FieldError::of(other, FieldCode::UnknownField)),
        }
    }
    let request = limit.and_then(|limit| errors.check(PageRequest::new(limit, cursor)));
    match request {
        Some(request) if errors.is_empty() => Ok(request),
        _ => Err(errors.into_error()),
    }
}

/// `{ "data": [...], "next_cursor": "..." | null }`.
#[derive(Debug, Serialize)]
pub struct PageResponse<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> PageResponse<T> {
    /// Maps each item of a use-case page to its response DTO.
    pub fn from_page<U>(page: Page<U>, to_dto: impl FnMut(U) -> T) -> Self {
        Self {
            next_cursor: page.next.map(|id| Cursor(id).encode()),
            data: page.items.into_iter().map(to_dto).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use application::error::AppError;

    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn codes(result: Result<PageRequest, AppError>) -> Vec<(String, FieldCode)> {
        match result {
            Err(AppError::Validation(errors)) => {
                errors.into_iter().map(|e| (e.field, e.code)).collect()
            }
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn cursor_round_trips_and_is_url_safe() {
        let id = Uuid::from_u128(0xfbff_ffff_ffff_7fff_bfff_ffff_ffff_fffe);
        let text = Cursor(id).encode();
        assert_eq!(text.len(), 22);
        assert!(
            text.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(Cursor::decode(&text), Some(Cursor(id)));
        assert_eq!(Cursor::decode("not a cursor"), None);
        assert_eq!(Cursor::decode("AAAA"), None);
    }

    #[test]
    fn defaults_without_parameters() {
        let request = parse(&[]).unwrap();
        assert_eq!((request.limit(), request.after()), (10, None));
    }

    #[test]
    fn valid_limit_and_cursor() {
        let id = Uuid::from_u128(42);
        let request = parse(&pairs(&[("limit", "25"), ("cursor", &Cursor(id).encode())])).unwrap();
        assert_eq!((request.limit(), request.after()), (25, Some(id)));
    }

    #[test]
    fn limit_rules() {
        assert_eq!(
            codes(parse(&pairs(&[("limit", "101")]))),
            [("limit".into(), FieldCode::OutOfRange)]
        );
        assert_eq!(
            codes(parse(&pairs(&[("limit", "0")]))),
            [("limit".into(), FieldCode::OutOfRange)]
        );
        assert_eq!(
            codes(parse(&pairs(&[("limit", "-1")]))),
            [("limit".into(), FieldCode::OutOfRange)]
        );
        assert_eq!(
            codes(parse(&pairs(&[("limit", "1.5")]))),
            [("limit".into(), FieldCode::InvalidType)]
        );
        assert_eq!(
            codes(parse(&pairs(&[("limit", "ten")]))),
            [("limit".into(), FieldCode::InvalidType)]
        );
    }

    #[test]
    fn every_problem_is_collected() {
        let got = codes(parse(&pairs(&[
            ("limit", "x"),
            ("cursor", "!!"),
            ("sort", "price"),
        ])));
        assert_eq!(
            got,
            [
                ("limit".into(), FieldCode::InvalidType),
                ("cursor".into(), FieldCode::InvalidFormat),
                ("sort".into(), FieldCode::UnknownField),
            ]
        );
    }

    #[test]
    fn page_response_encodes_next_cursor() {
        let id = Uuid::from_u128(7);
        let page = Page {
            items: vec![1, 2],
            next: Some(id),
        };
        let response = PageResponse::from_page(page, |n| n * 10);
        assert_eq!(response.data, [10, 20]);
        assert_eq!(response.next_cursor, Some(Cursor(id).encode()));
        let last = PageResponse::from_page(
            Page::<i32> {
                items: vec![],
                next: None,
            },
            |n| n,
        );
        assert_eq!(
            serde_json::to_string(&last).unwrap(),
            r#"{"data":[],"next_cursor":null}"#
        );
    }
}
