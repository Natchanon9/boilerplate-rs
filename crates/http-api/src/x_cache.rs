//! The `X-Cache` debug header (cache.md C6, security.md SEC-6).
//!
//! Installed on the protected router only when `CACHE_DEBUG_HEADER=true`. It opens the
//! `application::cache_status` scope around each `GET`, and the cache decorator records
//! `HIT`, `MISS`, or `ERROR` inside it.
//!
//! Which responses carry the header:
//! - Only `GET`s that reach a matched route with a valid token: the layer is a
//!   `route_layer` inside JWT auth, so an unknown path or a `401` never carries it.
//! - Cache enabled: only when a cache read recorded an outcome (get by id). Lists and
//!   writes record nothing, so they carry no header.
//! - Cache disabled: there is no decorator, so the scope starts at `BYPASS` and every such
//!   `GET` carries it (cache.md AC-3).
//! - Never on a `401` or `403`, in any mode: an auth rejection is not a cache result, and
//!   `RequireScope` answers `403` inside this layer.

use application::cache_status::{self, CacheStatus};
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

pub const X_CACHE: HeaderName = HeaderName::from_static("x-cache");

/// The `x_cache` middleware's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XCache {
    /// `false` unless `ENABLE_FEATURES` contains `cache`: every `GET` is a `BYPASS`.
    pub cache_enabled: bool,
}

/// Sets `X-Cache` from the outcome recorded while the request ran.
pub async fn x_cache(State(mode): State<XCache>, request: Request, next: Next) -> Response {
    if request.method() != Method::GET {
        return next.run(request).await;
    }
    let initial = (!mode.cache_enabled).then_some(CacheStatus::Bypass);
    let (mut response, status) = cache_status::scope(initial, next.run(request)).await;
    let auth_rejected = matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    );
    if let (Some(status), false) = (status, auth_rejected) {
        response
            .headers_mut()
            .insert(X_CACHE, HeaderValue::from_static(status.as_str()));
    }
    response
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::middleware;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    /// `/read` records the given outcome, as the cache decorator does; `/list` records nothing.
    fn app(cache_enabled: bool, outcome: CacheStatus) -> Router {
        Router::new()
            .route(
                "/read",
                get(move || async move {
                    cache_status::record(outcome);
                    "value"
                })
                .post(move || async move {
                    cache_status::record(outcome);
                    StatusCode::CREATED
                }),
            )
            .route("/list", get(|| async { "[]" }))
            .route(
                "/forbidden",
                get(move || async move {
                    cache_status::record(outcome);
                    StatusCode::FORBIDDEN
                }),
            )
            .route("/unauthorized", get(|| async { StatusCode::UNAUTHORIZED }))
            .route_layer(middleware::from_fn_with_state(
                XCache { cache_enabled },
                x_cache,
            ))
    }

    async fn header(router: Router, method: Method, path: &str) -> Option<String> {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        response
            .headers()
            .get(X_CACHE)
            .map(|value| value.to_str().unwrap().to_owned())
    }

    #[tokio::test]
    async fn get_carries_the_recorded_outcome() {
        for outcome in [CacheStatus::Hit, CacheStatus::Miss, CacheStatus::Error] {
            let value = header(app(true, outcome), Method::GET, "/read").await;
            assert_eq!(value.as_deref(), Some(outcome.as_str()));
        }
    }

    #[tokio::test]
    async fn get_without_a_cache_read_carries_no_header() {
        assert_eq!(
            header(app(true, CacheStatus::Hit), Method::GET, "/list").await,
            None
        );
    }

    #[tokio::test]
    async fn writes_carry_no_header() {
        assert_eq!(
            header(app(true, CacheStatus::Miss), Method::POST, "/read").await,
            None
        );
    }

    #[tokio::test]
    async fn every_get_is_bypass_when_the_cache_is_disabled() {
        for path in ["/read", "/list"] {
            // Without a decorator nothing records an outcome; `Bypass` stands in for that.
            let value = header(app(false, CacheStatus::Bypass), Method::GET, path).await;
            assert_eq!(value.as_deref(), Some("BYPASS"), "{path}");
        }
    }

    #[tokio::test]
    async fn auth_rejections_carry_no_header_in_any_mode() {
        for cache_enabled in [true, false] {
            for path in ["/forbidden", "/unauthorized"] {
                let router = app(cache_enabled, CacheStatus::Hit);
                assert_eq!(header(router, Method::GET, path).await, None, "{path}");
            }
        }
    }

    #[tokio::test]
    async fn unmatched_paths_carry_no_header() {
        assert_eq!(
            header(app(false, CacheStatus::Hit), Method::GET, "/nope").await,
            None
        );
    }
}
