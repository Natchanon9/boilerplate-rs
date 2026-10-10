//! The three routers, each served on its own port (runtime.md AC-5).

use std::collections::BTreeMap;
use std::sync::Arc;

use application::health::HealthCheck;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tower_http::trace::TraceLayer;

use crate::auth::{JwtVerifier, authenticate};
use crate::error::problem_details;
use crate::x_cache::{XCache, x_cache};

/// Every `/api/v1` route, behind JWT auth (security.md SEC-1). Resources merge their
/// routes in here during wiring, each with its `RequireScope` (security.md SEC-2).
/// Every error response, including unmatched paths, is problem+json.
///
/// Auth is a `route_layer`, so it runs only on matched paths: an unknown path is `404`
/// even without a token. The trace layer does not record headers (tower-http
/// `DefaultMakeSpan` has `include_headers: false`), so the `Authorization` header is never
/// logged (security.md SEC-9).
///
/// `x_cache_header` is `Some` only with `CACHE_DEBUG_HEADER=true` (security.md SEC-6). It
/// runs inside auth, so only authenticated requests on matched routes can carry `X-Cache`,
/// and never a 401 or 403 (cache.md C6).
pub fn protected(api_v1: Router, verifier: JwtVerifier, x_cache_header: Option<XCache>) -> Router {
    let router = Router::new().nest("/api/v1", api_v1);
    // axum panics on a `route_layer` over no routes; with none there is nothing to protect.
    let router = if router.has_routes() {
        // A later `route_layer` wraps the earlier ones, so auth runs first.
        let router = match x_cache_header {
            Some(mode) => router.route_layer(middleware::from_fn_with_state(mode, x_cache)),
            None => router,
        };
        router.route_layer(middleware::from_fn_with_state(verifier, authenticate))
    } else {
        router
    };
    router
        .layer(middleware::from_fn(problem_details))
        .layer(TraceLayer::new_for_http())
}

/// Routes that need no auth. Empty in the MVP. Errors are problem+json, as on protected.
pub fn public() -> Router {
    Router::new()
        .layer(middleware::from_fn(problem_details))
        .layer(TraceLayer::new_for_http())
}

/// `/healthz` and `/readyz`, with no auth. Never reachable from outside the deployment.
/// No problem+json here: `/readyz` answers 503 with its own body for probes.
pub fn hidden(checks: Vec<Arc<dyn HealthCheck>>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(Arc::new(checks))
        .layer(TraceLayer::new_for_http())
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    checks: BTreeMap<&'static str, &'static str>,
}

/// Liveness: the process is up. Checks no dependency.
async fn healthz() -> Json<Health> {
    Json(Health {
        status: "ok",
        checks: BTreeMap::new(),
    })
}

/// Readiness: `200` when every dependency answers, otherwise `503`
/// ([RFC 9110 15.6.4](https://www.rfc-editor.org/rfc/rfc9110#name-503-service-unavailable)).
/// The body names each check but never includes the error, which is logged instead.
async fn readyz(State(checks): State<Arc<Vec<Arc<dyn HealthCheck>>>>) -> Response {
    let mut results = BTreeMap::new();
    let mut ready = true;
    for check in checks.iter() {
        let state = match check.check().await {
            Ok(()) => "ok",
            Err(err) => {
                tracing::warn!(check = check.name(), error = %err, "readiness check failed");
                ready = false;
                "unavailable"
            }
        };
        results.insert(check.name(), state);
    }
    let (status, label) = if ready {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
    };
    (
        status,
        Json(Health {
            status: label,
            checks: results,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use application::health::HealthError;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    struct Fixed(&'static str, bool);

    #[async_trait]
    impl HealthCheck for Fixed {
        fn name(&self) -> &'static str {
            self.0
        }

        async fn check(&self) -> Result<(), HealthError> {
            if self.1 {
                Ok(())
            } else {
                Err(HealthError("connection refused to 10.0.0.5".to_owned()))
            }
        }
    }

    async fn get(router: Router, path: &str) -> (StatusCode, String) {
        let response = router
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn healthz_is_ok_without_checking_dependencies() {
        let router = hidden(vec![Arc::new(Fixed("database", false))]);
        assert_eq!(
            get(router, "/healthz").await,
            (StatusCode::OK, r#"{"status":"ok"}"#.to_owned())
        );
    }

    #[tokio::test]
    async fn readyz_is_ok_when_every_check_passes() {
        let router = hidden(vec![
            Arc::new(Fixed("database", true)),
            Arc::new(Fixed("cache", true)),
        ]);
        assert_eq!(
            get(router, "/readyz").await,
            (
                StatusCode::OK,
                r#"{"status":"ok","checks":{"cache":"ok","database":"ok"}}"#.to_owned()
            )
        );
    }

    #[tokio::test]
    async fn readyz_is_503_without_leaking_the_error() {
        let router = hidden(vec![
            Arc::new(Fixed("database", true)),
            Arc::new(Fixed("cache", false)),
        ]);
        let (status, body) = get(router, "/readyz").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            r#"{"status":"unavailable","checks":{"cache":"unavailable","database":"ok"}}"#
        );
    }

    #[tokio::test]
    async fn health_routes_are_not_on_the_protected_router() {
        let verifier = crate::auth::JwtVerifier::hs256(&[0; 32], "iss", "aud");
        let (status, body) = get(protected(Router::new(), verifier, None), "/healthz").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains(r#""type":"about:blank""#), "{body}");
        assert!(body.contains(r#""instance":"/healthz""#), "{body}");
    }
}
