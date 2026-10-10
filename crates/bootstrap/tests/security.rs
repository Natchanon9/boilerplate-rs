//! JWT auth as wired by bootstrap: the verifier built from `AuthConfig`, the protected
//! router, and `RequireScope` (security.md SEC-1, SEC-2).
//!
//! No container: the database pool is lazy and these routes never touch it.

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Request, StatusCode, header};
use axum::routing::get;
use bootstrap::config::{AuthConfig, DatabaseConfig, DbPlatform};
use bootstrap::wiring;
use http_api::auth::{Principal, RequireScope};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, get_current_timestamp};
use secrecy::SecretString;
use serde_json::{Value, json};
use tower::ServiceExt;

/// Every await on the system under test is bounded (layers.md section 7).
const STEP: Duration = Duration::from_secs(30);

const SECRET: &str = "0123456789abcdef0123456789abcdef-test-only";
const ISSUER: &str = "test-issuer";
const AUDIENCE: &str = "test-audience";

fn auth() -> AuthConfig {
    AuthConfig {
        jwt_secret: SecretString::from(SECRET),
        issuer: ISSUER.to_owned(),
        audience: AUDIENCE.to_owned(),
    }
}

/// A pool that never connects: nothing here reaches the database.
fn lazy_db() -> DatabaseConfig {
    DatabaseConfig {
        platform: DbPlatform::Postgres,
        url: SecretString::from("postgres://app:app@127.0.0.1:1/app"),
    }
}

fn token(secret: &str, claims: &Value) -> String {
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

fn claims(scope: &str) -> Value {
    let now = get_current_timestamp();
    json!({"sub": "alice", "scope": scope, "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600})
}

/// The protected router with one probe route, as a resource would add it in wiring.
fn probe_app() -> Router {
    let api_v1 = Router::new().route(
        "/probe",
        get(|principal: Principal| async move { principal.sub })
            .route_layer(RequireScope("probe:read")),
    );
    http_api::routers::protected(api_v1, wiring::jwt_verifier(&auth()), None)
}

async fn send(
    router: &Router,
    path: &str,
    bearer: Option<&str>,
) -> (StatusCode, Option<HeaderValue>, String) {
    let mut request = Request::get(path);
    if let Some(token) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request.body(Body::empty()).unwrap();
    let response = tokio::time::timeout(STEP, router.clone().oneshot(request))
        .await
        .unwrap()
        .unwrap();
    let status = response.status();
    let challenge = response.headers().get(header::WWW_AUTHENTICATE).cloned();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, challenge, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn security_sec_1_wired_verifier_accepts_a_token_from_the_configured_secret() {
    let (status, _, body) = send(
        &probe_app(),
        "/api/v1/probe",
        Some(&token(SECRET, &claims("probe:read"))),
    )
    .await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "alice"));
}

#[tokio::test]
async fn security_sec_1_missing_token_is_401_without_an_error_code() {
    let (status, challenge, body) = send(&probe_app(), "/api/v1/probe", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(challenge.unwrap(), "Bearer");
    assert!(body.contains(r#""status":401"#), "{body}");
}

#[tokio::test]
async fn security_sec_1_wired_verifier_rejects_another_secret_issuer_or_audience() {
    let other_secret = token(
        "another-secret-of-at-least-32-bytes!!",
        &claims("probe:read"),
    );
    let mut other_issuer = claims("probe:read");
    other_issuer["iss"] = json!("someone-else");
    let mut other_audience = claims("probe:read");
    other_audience["aud"] = json!("someone-else");
    for bad in [
        other_secret,
        token(SECRET, &other_issuer),
        token(SECRET, &other_audience),
    ] {
        let (status, challenge, _) = send(&probe_app(), "/api/v1/probe", Some(&bad)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(challenge.unwrap(), r#"Bearer error="invalid_token""#);
    }
}

#[tokio::test]
async fn security_sec_2_missing_scope_is_403_naming_the_scope() {
    let (status, challenge, body) = send(
        &probe_app(),
        "/api/v1/probe",
        Some(&token(SECRET, &claims("probe:write"))),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        challenge.unwrap(),
        r#"Bearer error="insufficient_scope", scope="probe:read""#
    );
    assert!(body.contains(r#""status":403"#), "{body}");
}

#[tokio::test]
async fn security_sec_1_http_app_answers_unknown_paths_with_404_before_auth() {
    let infra = wiring::Infra::connect(&lazy_db(), None).unwrap();
    let app = wiring::http_app(&infra, &auth(), false);
    for path in ["/api/v1/nowhere", "/nowhere"] {
        let (status, challenge, body) = send(&app.protected, path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(challenge.is_none(), "{path}");
        assert!(body.contains(r#""status":404"#), "{body}");
    }
}
