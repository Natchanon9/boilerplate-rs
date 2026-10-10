//! Request extractors shared by every resource (api.md).

use application::error::{FieldCode, FieldError};
use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::error::ApiError;

/// A JSON request body deserialized into a DTO whose fields are
/// `application::input::Field<T>`, so every field error is collected later by the
/// application parser.
///
/// - `415` without a JSON `Content-Type`
/// - `413` above the body limit, axum's `DefaultBodyLimit` of 2 MiB (api.md)
/// - `400` when the body is not syntactically valid JSON
/// - `422` `{field: "body", code: "invalid_type"}` when it is JSON but not an object
#[derive(Debug, Clone)]
pub struct JsonBody<T>(pub T);

impl<S, T> FromRequest<S> for JsonBody<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        if !is_json(request.headers()) {
            return Err(ApiError::from_status(StatusCode::UNSUPPORTED_MEDIA_TYPE));
        }
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|rejection| {
                if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    ApiError::from_status(StatusCode::PAYLOAD_TOO_LARGE)
                } else {
                    ApiError::from_status(StatusCode::BAD_REQUEST)
                }
            })?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::from_status(StatusCode::BAD_REQUEST))?;
        if !value.is_object() {
            return Err(not_an_object());
        }
        // Every DTO field is a Field<T>, so this fails only for a DTO written against the
        // convention; report it as a type error rather than a server error.
        serde_json::from_value(value)
            .map(JsonBody)
            .map_err(|_| not_an_object())
    }
}

fn not_an_object() -> ApiError {
    ApiError::validation(vec![FieldError::new(
        "body",
        FieldCode::InvalidType,
        "must be a JSON object",
    )])
}

/// `application/json`, or any `application/*+json` type, with optional parameters.
fn is_json(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let essence = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json"
        || (essence.starts_with("application/") && essence.ends_with("+json"))
}

/// The `{id}` path segment as a UUID. Anything else is `404`, so the id format is not
/// revealed (api.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdPath(pub Uuid);

impl<S: Send + Sync> FromRequestParts<S> for IdPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND))?;
        Uuid::parse_str(&raw)
            .map(IdPath)
            .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND))
    }
}

#[cfg(test)]
mod tests {
    use application::input::{Field, UnknownFields};
    use axum::Router;
    use axum::body::Body;
    use axum::routing::{get, post};
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::error::problem_details;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Dto {
        name: Field<String>,
        price: Field<i64>,
        #[serde(flatten)]
        unknown: UnknownFields,
    }

    async fn echo(JsonBody(dto): JsonBody<Dto>) -> String {
        format!("{:?}|{:?}|{:?}", dto.name, dto.price, dto.unknown.0)
    }

    async fn id(IdPath(id): IdPath) -> String {
        id.to_string()
    }

    fn router() -> Router {
        Router::new()
            .route("/things", post(echo))
            .route("/things/{id}", get(id))
            .layer(axum::middleware::from_fn(problem_details))
    }

    async fn send(request: Request) -> (StatusCode, String) {
        let response = router().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    fn post_json(content_type: Option<&str>, body: impl Into<Body>) -> Request {
        let mut builder = Request::post("/things");
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        builder.body(body.into()).unwrap()
    }

    #[tokio::test]
    async fn captures_every_field_state_and_unknown_field() {
        let body = r#"{"name": 1, "price": null, "extra": true, "other": 2}"#;
        let (status, text) = send(post_json(Some("application/json"), body)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(text, r#"Invalid|Null|["extra", "other"]"#);
    }

    #[tokio::test]
    async fn accepts_json_media_type_variants() {
        for content_type in [
            "application/json; charset=utf-8",
            "application/problem+json",
            "Application/JSON",
        ] {
            let (status, _) = send(post_json(Some(content_type), "{}")).await;
            assert_eq!(status, StatusCode::OK, "{content_type}");
        }
    }

    #[tokio::test]
    async fn rejects_missing_or_non_json_content_type_with_415() {
        for content_type in [None, Some("text/plain")] {
            let (status, _) = send(post_json(content_type, "{}")).await;
            assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }
    }

    #[tokio::test]
    async fn syntax_error_is_400() {
        for body in ["{\"name\": ", "", "not json"] {
            let (status, text) = send(post_json(Some("application/json"), body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            let problem: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(problem["status"], 400);
        }
    }

    #[tokio::test]
    async fn non_object_json_is_422_on_body() {
        let (status, text) = send(post_json(Some("application/json"), "[1, 2]")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let problem: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            problem["errors"],
            json!([{"field": "body", "code": "invalid_type", "message": "must be a JSON object"}])
        );
    }

    #[tokio::test]
    async fn body_over_the_default_limit_is_413() {
        let big = format!("{{\"name\": \"{}\"}}", "a".repeat(2 * 1024 * 1024 + 1));
        let (status, text) = send(post_json(Some("application/json"), big)).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(text.contains("\"status\":413"));
    }

    #[tokio::test]
    async fn id_path_accepts_uuid_and_hides_bad_ids_as_404() {
        let id = "0192f1a2-0000-7000-8000-000000000001";
        let ok = Request::get(format!("/things/{id}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(ok).await, (StatusCode::OK, id.to_owned()));
        let bad = Request::get("/things/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(bad).await.0, StatusCode::NOT_FOUND);
    }
}
