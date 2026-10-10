//! Every error response is RFC 9457 `application/problem+json`
//! (api.md AC-7).
//!
//! Handlers and extractors return [`ApiError`]. The [`problem_details`] middleware, installed
//! on the protected and public routers, adds `instance` (the request path) and turns any
//! other error response, such as the router's own 404 and 405, into a problem as well.

use std::borrow::Cow;

use application::error::{AppError, FieldError};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// The media type of every error body ([RFC 9457 3](https://www.rfc-editor.org/rfc/rfc9457#name-the-problem-details-json-ob)).
pub const PROBLEM_JSON: &str = "application/problem+json";

/// An error on its way to the client.
#[derive(Debug, Clone)]
pub struct ApiError {
    status: StatusCode,
    detail: Cow<'static, str>,
    errors: Vec<FieldError>,
}

impl ApiError {
    pub fn new(status: StatusCode, detail: impl Into<Cow<'static, str>>) -> Self {
        Self {
            status,
            detail: detail.into(),
            errors: Vec::new(),
        }
    }

    /// The generic problem for a status, with a fixed sentence as `detail`.
    pub fn from_status(status: StatusCode) -> Self {
        Self::new(status, default_detail(status))
    }

    /// `422` with one entry per failing field.
    pub fn validation(errors: Vec<FieldError>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            detail: Cow::Borrowed("The request has invalid fields."),
            errors,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn field_errors(&self) -> &[FieldError] {
        &self.errors
    }

    fn body(&self, instance: Option<String>) -> Problem {
        Problem {
            kind: "about:blank",
            title: self.status.canonical_reason().unwrap_or("Error"),
            status: self.status.as_u16(),
            detail: self.detail.to_string(),
            instance,
            errors: self
                .errors
                .iter()
                .map(|e| ProblemField {
                    field: e.field.clone(),
                    code: e.code.as_str(),
                    message: e.message.clone(),
                })
                .collect(),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Validation(errors) => Self::validation(errors),
            AppError::NotFound => Self::from_status(StatusCode::NOT_FOUND),
            // The use case writes the conflict message for clients, so it is safe to show.
            AppError::Conflict(detail) => Self::new(StatusCode::CONFLICT, detail),
            AppError::Unauthorized => Self::from_status(StatusCode::UNAUTHORIZED),
            AppError::Forbidden => Self::from_status(StatusCode::FORBIDDEN),
            // The cause stays in the log, never in the response (security.md SEC-6).
            AppError::Unavailable(cause) => {
                tracing::warn!(%cause, "dependency unavailable");
                Self::from_status(StatusCode::SERVICE_UNAVAILABLE)
            }
            AppError::Internal(cause) => {
                tracing::error!(%cause, "internal error");
                Self::from_status(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

/// Fixed `detail` sentences, so no internal detail ever reaches a client (own proposal).
fn default_detail(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "The request body is not valid JSON.",
        StatusCode::UNAUTHORIZED => "A valid bearer token is required.",
        StatusCode::FORBIDDEN => "The token does not grant the required scope.",
        StatusCode::NOT_FOUND => "The resource does not exist.",
        StatusCode::METHOD_NOT_ALLOWED => "The method is not allowed for this resource.",
        StatusCode::CONFLICT => "The request conflicts with the current state of the resource.",
        StatusCode::PAYLOAD_TOO_LARGE => "The request body is larger than the limit.",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => {
            "The request body must be JSON (Content-Type: application/json)."
        }
        StatusCode::UNPROCESSABLE_ENTITY => "The request has invalid fields.",
        StatusCode::SERVICE_UNAVAILABLE => "A dependency is unavailable. Try again later.",
        s if s.is_server_error() => "An unexpected error occurred.",
        _ => "The request failed.",
    }
}

#[derive(Debug, Serialize)]
struct Problem {
    #[serde(rename = "type")]
    kind: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<ProblemField>,
}

#[derive(Debug, Serialize)]
struct ProblemField {
    field: String,
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = render(self.status, &self.body(None), Response::new(Body::empty()));
        // Kept so `problem_details` can render it again with `instance`.
        response.extensions_mut().insert(self);
        response
    }
}

/// Middleware: adds `instance` to every problem, and turns any other 4xx or 5xx response
/// into a problem, keeping its headers (for example `Allow` on 405).
pub async fn problem_details(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    if let Some(error) = response.extensions_mut().remove::<ApiError>() {
        return render(error.status, &error.body(Some(path)), response);
    }
    let status = response.status();
    if (status.is_client_error() || status.is_server_error()) && !is_problem(&response) {
        let error = ApiError::from_status(status);
        return render(status, &error.body(Some(path)), response);
    }
    response
}

fn is_problem(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes() == PROBLEM_JSON.as_bytes())
}

/// Replaces the body of `template` with the problem, keeping its other headers.
fn render(status: StatusCode, problem: &Problem, template: Response) -> Response {
    let (mut parts, _) = template.into_parts();
    let body = match serde_json::to_vec(problem) {
        Ok(body) => body,
        Err(err) => {
            tracing::error!(error = %err, "cannot serialize a problem");
            Vec::new()
        }
    };
    parts.status = status;
    parts.headers.remove(header::CONTENT_LENGTH);
    parts
        .headers
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
    Response::from_parts(parts, Body::from(body))
}

#[cfg(test)]
mod tests {
    use application::error::FieldCode;
    use axum::Router;
    use axum::routing::get;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

    async fn send(router: Router, request: Request) -> (StatusCode, Option<String>, Value) {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_owned());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, content_type, body)
    }

    fn get_request(path: &str) -> Request {
        Request::get(path).body(Body::empty()).unwrap()
    }

    fn failing(error: AppError) -> Router {
        let error = std::sync::Mutex::new(Some(error));
        let error = std::sync::Arc::new(error);
        Router::new()
            .route(
                "/thing",
                get(move || {
                    let error = error.clone();
                    async move { ApiError::from(error.lock().unwrap().take().unwrap()) }
                }),
            )
            .layer(axum::middleware::from_fn(problem_details))
    }

    #[tokio::test]
    async fn every_app_error_maps_to_its_status() {
        let cases = [
            (AppError::NotFound, 404),
            (AppError::Conflict("SKU taken".into()), 409),
            (AppError::Validation(vec![]), 422),
            (AppError::Unauthorized, 401),
            (AppError::Forbidden, 403),
            (AppError::Unavailable("pool timed out".into()), 503),
            (AppError::Internal("row decode failed".into()), 500),
        ];
        for (error, status) in cases {
            let (got, content_type, body) = send(failing(error), get_request("/thing")).await;
            assert_eq!(got.as_u16(), status);
            assert_eq!(content_type.as_deref(), Some(PROBLEM_JSON));
            assert_eq!(body["type"], "about:blank");
            assert_eq!(body["status"], status);
            assert_eq!(body["title"], got.canonical_reason().unwrap());
            assert_eq!(body["instance"], "/thing");
            assert!(body["detail"].is_string());
        }
    }

    #[tokio::test]
    async fn internal_causes_never_reach_the_client() {
        for error in [
            AppError::Unavailable("pool timed out at 10.0.0.5".into()),
            AppError::Internal("row decode failed: secret".into()),
        ] {
            let (_, _, body) = send(failing(error), get_request("/thing")).await;
            let text = body.to_string();
            assert!(
                !text.contains("10.0.0.5") && !text.contains("secret"),
                "{text}"
            );
        }
    }

    #[tokio::test]
    async fn conflict_shows_the_use_case_message() {
        let (_, _, body) = send(
            failing(AppError::Conflict("SKU taken".into())),
            get_request("/thing"),
        )
        .await;
        assert_eq!(body["detail"], "SKU taken");
    }

    #[tokio::test]
    async fn validation_lists_every_field() {
        let errors = vec![
            FieldError::of("name", FieldCode::Required),
            FieldError::new("price", FieldCode::OutOfRange, "must be at most 10"),
        ];
        let (_, _, body) = send(failing(AppError::Validation(errors)), get_request("/thing")).await;
        assert_eq!(
            body["errors"],
            json!([
                {"field": "name", "code": "required", "message": "is required"},
                {"field": "price", "code": "out_of_range", "message": "must be at most 10"}
            ])
        );
    }

    #[tokio::test]
    async fn router_404_and_405_become_problems() {
        let router = failing(AppError::NotFound);
        let (status, content_type, body) = send(router.clone(), get_request("/nowhere")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(content_type.as_deref(), Some(PROBLEM_JSON));
        assert_eq!(body["instance"], "/nowhere");

        let response = router
            .oneshot(Request::delete("/thing").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(response.headers().contains_key(header::ALLOW));
        assert_eq!(response.headers()[header::CONTENT_TYPE], PROBLEM_JSON);
    }
}
