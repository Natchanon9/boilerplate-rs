//! Health and readiness on the hidden router, against real Postgres and Redis containers
//! (stack-architecture.md AC-6, runtime.md AC-5, database.md AC-5).

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bootstrap::config::{AuthConfig, CacheConfig, DatabaseConfig, DbPlatform};
use bootstrap::wiring;
use http_body_util::BodyExt;
use secrecy::SecretString;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;

/// Every await on the system under test is bounded (layers.md section 7).
const STEP: Duration = Duration::from_secs(30);

async fn postgres() -> (ContainerAsync<Postgres>, DatabaseConfig) {
    let container = Postgres::default().with_tag("18.6").start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let config = DatabaseConfig {
        platform: DbPlatform::Postgres,
        url: SecretString::from(url),
    };
    (container, config)
}

async fn redis() -> (ContainerAsync<Redis>, CacheConfig) {
    let container = Redis::default().with_tag("8.10.2").start().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let config = CacheConfig {
        redis_url: SecretString::from(format!("redis://127.0.0.1:{port}")),
        ttl: Duration::from_secs(300),
        app_name: "rs-boilerplate".to_owned(),
        app_env: "test".to_owned(),
    };
    (container, config)
}

fn auth() -> AuthConfig {
    AuthConfig {
        jwt_secret: SecretString::from("0123456789abcdef0123456789abcdef-test-only"),
        issuer: "test-issuer".to_owned(),
        audience: "test-audience".to_owned(),
    }
}

async fn get(router: &Router, path: &str) -> (StatusCode, String) {
    let request = Request::get(path).body(Body::empty()).unwrap();
    let response = tokio::time::timeout(STEP, router.clone().oneshot(request))
        .await
        .unwrap()
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn stack_ac_6_healthz_and_readyz_with_cache_enabled() {
    let (_pg, db) = postgres().await;
    let (redis_container, cache) = redis().await;
    let infra = wiring::Infra::connect(&db, Some(&cache)).unwrap();
    let app = wiring::http_app(&infra, &auth(), false);

    assert_eq!(
        get(&app.hidden, "/healthz").await,
        (StatusCode::OK, r#"{"status":"ok"}"#.to_owned())
    );
    assert_eq!(
        get(&app.hidden, "/readyz").await,
        (
            StatusCode::OK,
            r#"{"status":"ok","checks":{"cache":"ok","database":"ok"}}"#.to_owned()
        )
    );

    // Redis down: not ready, but still alive. Each probe is timed: the 1st sees the dropped
    // connection, later ones make one reconnect attempt each. The bound is the cache call
    // bound (cache.md C9): one 1 s connection attempt plus one 500 ms command, with more
    // than 3x headroom, well under the 9.5 s that 6 reconnect retries took.
    redis_container.stop_with_timeout(Some(0)).await.unwrap();
    for probe in 1..=3 {
        let started = Instant::now();
        let (status, body) = get(&app.hidden, "/readyz").await;
        let elapsed = started.elapsed();
        eprintln!("/readyz {probe} after the Redis stop: {elapsed:?}");
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            r#"{"status":"unavailable","checks":{"cache":"unavailable","database":"ok"}}"#
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "/readyz {probe} took {elapsed:?}"
        );
    }
    assert_eq!(get(&app.hidden, "/healthz").await.0, StatusCode::OK);
}

#[tokio::test]
async fn database_ac_5_readyz_fails_fast_while_the_database_is_down() {
    let (pg, db) = postgres().await;
    let infra = wiring::Infra::connect(&db, None).unwrap();
    let app = wiring::http_app(&infra, &auth(), false);

    assert_eq!(get(&app.hidden, "/readyz").await.0, StatusCode::OK);

    // Postgres down: each probe must fail fast. The design bound is the 1 s pool acquire
    // timeout; 5 s leaves headroom and still catches the 30 s sqlx default.
    pg.stop_with_timeout(Some(0)).await.unwrap();
    for probe in 1..=3 {
        let started = Instant::now();
        let (status, _body) = get(&app.hidden, "/readyz").await;
        let elapsed = started.elapsed();
        eprintln!("/readyz {probe} after the Postgres stop: {elapsed:?}");
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            elapsed < Duration::from_secs(5),
            "/readyz {probe} took {elapsed:?}"
        );
    }
}

#[tokio::test]
async fn stack_ac_6_readyz_skips_redis_when_cache_is_disabled() {
    let (_pg, db) = postgres().await;
    let infra = wiring::Infra::connect(&db, None).unwrap();
    let app = wiring::http_app(&infra, &auth(), false);

    assert_eq!(
        get(&app.hidden, "/readyz").await,
        (
            StatusCode::OK,
            r#"{"status":"ok","checks":{"database":"ok"}}"#.to_owned()
        )
    );
}

#[tokio::test]
async fn runtime_ac_5_health_routes_live_only_on_the_hidden_router() {
    let (_pg, db) = postgres().await;
    let infra = wiring::Infra::connect(&db, None).unwrap();
    let app = wiring::http_app(&infra, &auth(), false);

    for router in [&app.protected, &app.public] {
        assert_eq!(get(router, "/healthz").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get(router, "/readyz").await.0, StatusCode::NOT_FOUND);
    }
}
