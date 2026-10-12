//! Helpers shared by the `track_*.rs` integration tests: containers, tokens, and one-call
//! HTTP requests against the wired protected router. Docker must run.

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions. Each test file
// uses a different part of this module.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use bootstrap::config::{AuthConfig, CacheConfig, DatabaseConfig, DbPlatform};
use bootstrap::wiring::{self, Infra};
use http_body_util::BodyExt;
use infrastructure::postgres;
use jsonwebtoken::{Algorithm, EncodingKey, Header, get_current_timestamp};
use secrecy::SecretString;
use serde_json::{Value, json};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;

/// Every await on the system under test is bounded (layers.md section 7).
pub const STEP: Duration = Duration::from_secs(30);

pub const SECRET: &str = "0123456789abcdef0123456789abcdef-test-only";
const ISSUER: &str = "test-issuer";
const AUDIENCE: &str = "test-audience";

pub fn auth() -> AuthConfig {
    AuthConfig {
        jwt_secret: SecretString::from(SECRET),
        issuer: ISSUER.to_owned(),
        audience: AUDIENCE.to_owned(),
    }
}

/// A token for `alice` holding `scope` (space-separated scopes).
pub fn token(scope: &str) -> String {
    let now = get_current_timestamp();
    let claims = json!({
        "sub": "alice", "scope": scope, "iss": ISSUER, "aud": AUDIENCE,
        "iat": now, "exp": now + 3600,
    });
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// Read and write access to tracks.
pub fn full_token() -> String {
    token("tracks:read tracks:write")
}

/// The workspace root, where `seeds/` lives.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub struct Database {
    pub container: ContainerAsync<Postgres>,
    pub config: DatabaseConfig,
    pub url: String,
}

/// A fresh Postgres container. Nothing is migrated yet.
pub async fn postgres_container() -> Database {
    let container = Postgres::default().with_tag("18.6").start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let config = DatabaseConfig {
        platform: DbPlatform::Postgres,
        url: SecretString::from(url.clone()),
    };
    Database {
        container,
        config,
        url,
    }
}

pub async fn redis_container() -> (ContainerAsync<Redis>, CacheConfig) {
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

/// Applies every migration through the shared pool.
pub async fn migrate(infra: &Infra) {
    tokio::time::timeout(STEP, postgres::migrate(&infra.pool))
        .await
        .unwrap()
        .unwrap();
}

/// Loads `seeds/postgres/` through the shared pool.
pub async fn seed(infra: &Infra) {
    let dir = workspace_root().join("seeds/postgres");
    tokio::time::timeout(STEP, postgres::seed(&infra.pool, &dir))
        .await
        .unwrap()
        .unwrap();
}

/// The protected router wired as `serve` builds it, with the `X-Cache` header on.
pub fn router(infra: &Infra) -> Router {
    wiring::http_app(infra, &auth(), true).protected
}

/// A migrated database and the wired router, without the cache feature.
pub struct App {
    pub db: Database,
    pub infra: Infra,
    pub router: Router,
}

pub async fn app() -> App {
    let db = postgres_container().await;
    let infra = Infra::connect(&db.config, None).unwrap();
    migrate(&infra).await;
    let router = router(&infra);
    App { db, infra, router }
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|v| v.to_str().unwrap())
    }

    /// `(field, code)` of every entry in a problem+json `errors` list.
    pub fn field_errors(&self) -> Vec<(String, String)> {
        self.body["errors"]
            .as_array()
            .unwrap_or_else(|| panic!("no errors list in {}", self.body))
            .iter()
            .map(|e| {
                (
                    e["field"].as_str().unwrap().to_owned(),
                    e["code"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
}

/// Sends one request with an optional bearer token and JSON body.
pub async fn call(
    router: &Router,
    method: Method,
    path: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(bearer) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    }
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = tokio::time::timeout(STEP, router.clone().oneshot(request))
        .await
        .unwrap()
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = tokio::time::timeout(STEP, response.into_body().collect())
        .await
        .unwrap()
        .unwrap()
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        headers,
        body,
    }
}

/// The valid create or update body from the spec's happy path.
pub fn valid_body() -> Value {
    json!({"title": "Clair de lune", "artist": "Debussy", "duration_ms": 300_000})
}

/// POST a valid track with a full token and return the created track's JSON.
pub async fn create_track(router: &Router, body: Value) -> Value {
    let reply = call(
        router,
        Method::POST,
        "/api/v1/tracks",
        Some(&full_token()),
        Some(body),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    reply.body
}

pub fn track_path(id: &str) -> String {
    format!("/api/v1/tracks/{id}")
}
