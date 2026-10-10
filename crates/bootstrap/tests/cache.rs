//! Cache-aside against a real Redis container: `get_or_load`, `invalidate`, fail-open, and
//! the `X-Cache` header as wired by bootstrap (cache.md AC-1 to AC-5, C4 to C6).
//! The per-resource decorator is covered by each resource's own tests.

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use application::cache_status::{self, CacheStatus};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use axum::{Json, Router};
use bootstrap::config::{AuthConfig, CacheConfig};
use bootstrap::wiring;
use http_api::auth::RequireScope;
use http_api::x_cache::X_CACHE;
use infrastructure::redis::cache_aside::{self, RedisCache};
use jsonwebtoken::{Algorithm, EncodingKey, Header, get_current_timestamp};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::json;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;

/// Every await on the system under test is bounded (layers.md section 7).
const STEP: Duration = Duration::from_secs(30);

/// The default TTL (cache.md AC-5).
const TTL: Duration = Duration::from_secs(300);

const SECRET: &str = "0123456789abcdef0123456789abcdef-test-only";

/// Stands in for a resource row: the cached value is the row's JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Row {
    id: u32,
    name: String,
}

fn row(id: u32) -> Row {
    Row {
        id,
        name: format!("row {id}"),
    }
}

async fn redis() -> (ContainerAsync<Redis>, RedisCache) {
    let container = Redis::default().with_tag("8.10.2").start().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let config = CacheConfig {
        redis_url: SecretString::from(format!("redis://127.0.0.1:{port}")),
        ttl: TTL,
        app_name: "rs-boilerplate".to_owned(),
        app_env: "test".to_owned(),
    };
    (container, wiring::redis_cache(&config).unwrap())
}

/// One raw Redis command, to inspect or plant values behind the helper's back.
async fn raw<T: redis::FromRedisValue>(cache: &RedisCache, cmd: &mut redis::Cmd) -> T {
    let mut conn = cache.connection();
    tokio::time::timeout(STEP, cmd.query_async::<T>(&mut conn))
        .await
        .unwrap()
        .unwrap()
}

async fn exists(cache: &RedisCache, key: &str) -> bool {
    raw::<i64>(cache, redis::cmd("EXISTS").arg(key)).await == 1
}

/// One `get_or_load` inside a cache-status scope, as a request would run it. Returns the
/// value and the recorded outcome, and counts loader calls.
async fn read(
    cache: &RedisCache,
    key: &str,
    db: Option<Row>,
    loads: &AtomicUsize,
) -> (Option<Row>, Option<CacheStatus>) {
    let load = cache_aside::get_or_load(cache, key, TTL, || async {
        loads.fetch_add(1, Ordering::SeqCst);
        Ok::<_, Infallible>(db)
    });
    let (result, status) = tokio::time::timeout(STEP, cache_status::scope(None, load))
        .await
        .unwrap();
    let Ok(value) = result;
    (value, status)
}

#[tokio::test]
async fn cache_ac_1_first_read_is_a_miss_then_a_hit() {
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 1);
    let loads = AtomicUsize::new(0);

    let first = read(&cache, &key, Some(row(1)), &loads).await;
    assert_eq!(first, (Some(row(1)), Some(CacheStatus::Miss)));
    let second = read(&cache, &key, Some(row(1)), &loads).await;
    assert_eq!(second, (Some(row(1)), Some(CacheStatus::Hit)));
    assert_eq!(loads.load(Ordering::SeqCst), 1, "a HIT must not load");
}

#[tokio::test]
async fn cache_ac_5_value_is_json_under_the_conventional_key_with_the_ttl() {
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 7);
    assert_eq!(key, "rs-boilerplate:test:v1:probe:7");

    read(&cache, &key, Some(row(7)), &AtomicUsize::new(0)).await;
    let stored: String = raw(&cache, redis::cmd("GET").arg(&key)).await;
    assert_eq!(stored, r#"{"id":7,"name":"row 7"}"#);
    let ttl: i64 = raw(&cache, redis::cmd("TTL").arg(&key)).await;
    assert!(ttl > 0 && ttl <= 300, "TTL {ttl}");
}

#[tokio::test]
async fn cache_undecodable_value_is_a_miss_that_overwrites_it() {
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 2);
    // An old shape (no `name`, as if `v1` was not bumped), and bytes that are not JSON.
    let stale: [&[u8]; 2] = [br#"{"id":2}"#, b"\xff\xfenot json"];
    for value in stale {
        let _: () = raw(&cache, redis::cmd("SET").arg(&key).arg(value)).await;
        let loads = AtomicUsize::new(0);

        let first = read(&cache, &key, Some(row(2)), &loads).await;
        assert_eq!(first, (Some(row(2)), Some(CacheStatus::Miss)));
        let stored: String = raw(&cache, redis::cmd("GET").arg(&key)).await;
        assert_eq!(stored, r#"{"id":2,"name":"row 2"}"#);
        let second = read(&cache, &key, Some(row(2)), &loads).await;
        assert_eq!(second, (Some(row(2)), Some(CacheStatus::Hit)));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn cache_loader_none_writes_nothing() {
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 3);
    let loads = AtomicUsize::new(0);

    assert_eq!(
        read(&cache, &key, None, &loads).await,
        (None, Some(CacheStatus::Miss))
    );
    assert!(!exists(&cache, &key).await, "no negative caching");
    // So the next read is a MISS again.
    read(&cache, &key, None, &loads).await;
    assert_eq!(loads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cache_loader_error_is_returned_and_nothing_is_cached() {
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 4);
    let load = cache_aside::get_or_load::<Row, _, _, _>(&cache, &key, TTL, || async {
        Err("database down")
    });
    let result = tokio::time::timeout(STEP, load).await.unwrap();
    assert_eq!(result, Err("database down"));
    assert!(!exists(&cache, &key).await);
}

#[tokio::test]
async fn cache_ac_2_invalidate_removes_the_key() {
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 5);
    let loads = AtomicUsize::new(0);
    read(&cache, &key, Some(row(5)), &loads).await;
    assert!(exists(&cache, &key).await);

    tokio::time::timeout(STEP, cache_aside::invalidate(&cache, &key))
        .await
        .unwrap();
    assert!(!exists(&cache, &key).await);
    let updated = Row {
        id: 5,
        name: "renamed".to_owned(),
    };
    assert_eq!(
        read(&cache, &key, Some(updated.clone()), &loads).await,
        (Some(updated), Some(CacheStatus::Miss))
    );
}

/// Upper bound for one cache call while Redis is down (cache.md C9). By design a call makes
/// one connection attempt (1 s connection timeout) and at most one command (500 ms response
/// timeout), 1.5 s in all; 5 s leaves more than 3x headroom for a slow CI runner, and still
/// fails on the 9.5 s reconnect cycle that 6 retries caused.
const OUTAGE_CALL_BOUND: Duration = Duration::from_secs(5);

#[tokio::test]
async fn cache_ac_4_redis_down_fails_open_with_error() {
    let (redis_container, cache) = redis().await;
    let key = cache.key("probe", 6);
    let loads = AtomicUsize::new(0);
    read(&cache, &key, Some(row(6)), &loads).await;

    redis_container.stop_with_timeout(Some(0)).await.unwrap();
    // The cached value is unreachable, so the loader answers, and a write still succeeds.
    // Every call is timed: the 1st sees the dropped connection, later ones reconnect.
    for call in 1..=3 {
        let started = Instant::now();
        let outcome = read(&cache, &key, Some(row(6)), &loads).await;
        let elapsed = started.elapsed();
        eprintln!("cache call {call} after the Redis stop: {elapsed:?}");
        assert_eq!(outcome, (Some(row(6)), Some(CacheStatus::Error)));
        assert!(elapsed < OUTAGE_CALL_BOUND, "call {call} took {elapsed:?}");
    }
    assert_eq!(loads.load(Ordering::SeqCst), 4);
    let started = Instant::now();
    tokio::time::timeout(STEP, cache_aside::invalidate(&cache, &key))
        .await
        .unwrap();
    assert!(
        started.elapsed() < OUTAGE_CALL_BOUND,
        "DEL took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn cache_read_outside_a_scope_records_nothing_and_still_works() {
    // The Kafka path has no X-Cache scope.
    let (_redis, cache) = redis().await;
    let key = cache.key("probe", 8);
    let load = cache_aside::get_or_load(&cache, &key, TTL, || async {
        Ok::<_, Infallible>(Some(row(8)))
    });
    let result = tokio::time::timeout(STEP, load).await.unwrap();
    assert_eq!(result, Ok(Some(row(8))));
}

// X-Cache through the wired protected router.

fn auth() -> AuthConfig {
    AuthConfig {
        jwt_secret: SecretString::from(SECRET),
        issuer: "test-issuer".to_owned(),
        audience: "test-audience".to_owned(),
    }
}

fn token() -> String {
    token_with_scope("probe:read")
}

fn token_with_scope(scope: &str) -> String {
    let now = get_current_timestamp();
    let claims = json!({
        "sub": "alice", "scope": scope, "iss": "test-issuer", "aud": "test-audience",
        "iat": now, "exp": now + 3600,
    });
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// Reads a probe through the cache, as a `Cached<Res>Repository` would. Id 404 is missing.
async fn get_probe(
    State(cache): State<Option<RedisCache>>,
    Path(id): Path<u32>,
) -> Result<Json<Row>, StatusCode> {
    let db = (id != 404).then(|| row(id));
    let value = match cache {
        Some(cache) => {
            let key = cache.key("probe", id);
            let Ok(value) =
                cache_aside::get_or_load(&cache, &key, TTL, || async { Ok::<_, Infallible>(db) })
                    .await;
            value
        }
        // The cache feature is off: no decorator, straight to the database.
        None => db,
    };
    value.map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// The protected router as wiring builds it, with a probe resource: get by id goes
/// through the cache, the list does not, and delete invalidates.
fn probe_app(cache: Option<RedisCache>, debug_header: bool) -> Router {
    let cache_enabled = cache.is_some();
    let api_v1 = Router::new()
        .route(
            "/probes",
            get(|| async { Json(Vec::<Row>::new()) }).route_layer(RequireScope("probe:read")),
        )
        .route(
            "/probes/{id}",
            get(get_probe)
                .delete(
                    |State(cache): State<Option<RedisCache>>, Path(id): Path<u32>| async move {
                        if let Some(cache) = cache {
                            cache_aside::invalidate(&cache, &cache.key("probe", id)).await;
                        }
                        StatusCode::NO_CONTENT
                    },
                )
                .route_layer(RequireScope("probe:read")),
        )
        .with_state(cache);
    wiring::protected_router(api_v1, &auth(), cache_enabled, debug_header)
}

/// Sends a request and returns the status and the `X-Cache` header.
async fn send(
    router: &Router,
    method: &str,
    path: &str,
    auth: bool,
) -> (StatusCode, Option<String>) {
    let mut request = Request::builder().method(method).uri(path);
    if auth {
        request = request.header(header::AUTHORIZATION, format!("Bearer {}", token()));
    }
    let request = request.body(Body::empty()).unwrap();
    let response = tokio::time::timeout(STEP, router.clone().oneshot(request))
        .await
        .unwrap()
        .unwrap();
    let x_cache = response
        .headers()
        .get(X_CACHE)
        .map(|value| value.to_str().unwrap().to_owned());
    (response.status(), x_cache)
}

fn with(status: StatusCode, x_cache: &str) -> (StatusCode, Option<String>) {
    (status, Some(x_cache.to_owned()))
}

#[tokio::test]
async fn cache_ac_1_x_cache_header_shows_miss_then_hit() {
    let (_redis, cache) = redis().await;
    let app = probe_app(Some(cache), true);

    assert_eq!(
        send(&app, "GET", "/api/v1/probes/1", true).await,
        with(StatusCode::OK, "MISS")
    );
    assert_eq!(
        send(&app, "GET", "/api/v1/probes/1", true).await,
        with(StatusCode::OK, "HIT")
    );
    // A missing row is still a cache read.
    assert_eq!(
        send(&app, "GET", "/api/v1/probes/404", true).await,
        with(StatusCode::NOT_FOUND, "MISS")
    );
}

#[tokio::test]
async fn cache_ac_2_x_cache_is_miss_after_a_write_and_writes_carry_no_header() {
    let (_redis, cache) = redis().await;
    let app = probe_app(Some(cache), true);
    send(&app, "GET", "/api/v1/probes/1", true).await;

    assert_eq!(
        send(&app, "DELETE", "/api/v1/probes/1", true).await,
        (StatusCode::NO_CONTENT, None)
    );
    assert_eq!(
        send(&app, "GET", "/api/v1/probes/1", true).await,
        with(StatusCode::OK, "MISS")
    );
}

#[tokio::test]
async fn cache_x_cache_header_only_on_cache_reads() {
    let (_redis, cache) = redis().await;
    let app = probe_app(Some(cache), true);

    assert_eq!(
        send(&app, "GET", "/api/v1/probes", true).await,
        (StatusCode::OK, None)
    );
    // No token, or no route: the middleware never runs.
    assert_eq!(
        send(&app, "GET", "/api/v1/probes/1", false).await,
        (StatusCode::UNAUTHORIZED, None)
    );
    assert_eq!(
        send(&app, "GET", "/api/v1/nope", true).await,
        (StatusCode::NOT_FOUND, None)
    );
}

#[tokio::test]
async fn cache_x_cache_header_is_absent_without_cache_debug_header() {
    let (_redis, cache) = redis().await;
    let app = probe_app(Some(cache), false);

    for _ in 0..2 {
        assert_eq!(
            send(&app, "GET", "/api/v1/probes/1", true).await,
            (StatusCode::OK, None)
        );
    }
}

#[tokio::test]
async fn cache_ac_3_x_cache_is_bypass_when_the_cache_is_disabled() {
    // No Redis at all: with the feature off nothing connects to it.
    let app = probe_app(None, true);

    for path in ["/api/v1/probes/1", "/api/v1/probes/1", "/api/v1/probes"] {
        assert_eq!(
            send(&app, "GET", path, true).await,
            with(StatusCode::OK, "BYPASS")
        );
    }
    assert_eq!(
        send(&app, "DELETE", "/api/v1/probes/1", true).await,
        (StatusCode::NO_CONTENT, None)
    );
}

#[tokio::test]
async fn cache_ac_4_x_cache_is_error_and_the_api_answers_200_when_redis_is_down() {
    let (redis_container, cache) = redis().await;
    let app = probe_app(Some(cache), true);
    send(&app, "GET", "/api/v1/probes/1", true).await;

    redis_container.stop_with_timeout(Some(0)).await.unwrap();
    assert_eq!(
        send(&app, "GET", "/api/v1/probes/1", true).await,
        with(StatusCode::OK, "ERROR")
    );
    // A write still succeeds; the failed DEL is only logged.
    assert_eq!(
        send(&app, "DELETE", "/api/v1/probes/1", true).await,
        (StatusCode::NO_CONTENT, None)
    );
}

#[tokio::test]
async fn cache_x_cache_header_is_never_on_401_or_403() {
    let (_redis, cache) = redis().await;
    for app in [probe_app(Some(cache), true), probe_app(None, true)] {
        for (bearer, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some(token_with_scope("other:read")), StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::get("/api/v1/probes/1");
            if let Some(bearer) = &bearer {
                request = request.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
            }
            let request = request.body(Body::empty()).unwrap();
            let response = tokio::time::timeout(STEP, app.clone().oneshot(request))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(response.status(), expected);
            assert_eq!(response.headers().get(X_CACHE), None, "{expected}");
        }
    }
}
