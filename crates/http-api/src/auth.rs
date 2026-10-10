//! Bearer-token authentication and per-route scopes (security.md SEC-1, SEC-2).
//!
//! The protected router runs [`authenticate`] on every route it matches: it validates the
//! HS256 access token with a [`JwtVerifier`] and puts a [`Principal`] in the request
//! extensions. Each route then declares its scope with [`RequireScope`]:
//!
//! ```text
//! Router::new().route("/widgets", get(list).route_layer(RequireScope("widgets:read")))
//! ```
//!
//! Failures follow [RFC 6750 3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1), with
//! problem+json bodies: `401` and `WWW-Authenticate: Bearer` without credentials, `401` with
//! `error="invalid_token"` for any bad token, and `403` with `error="insufficient_scope"`.
//! The token and the `Authorization` header are never logged (security.md SEC-9).

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tower::{Layer, Service};

use crate::error::ApiError;

/// The authenticated caller: the token's `sub` and its scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub sub: String,
    pub scopes: BTreeSet<String>,
}

impl Principal {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
    }
}

/// Handlers take `principal: Principal` to read the caller, for example to filter by owner
/// (security.md SEC-3). Without one, the route is not behind [`authenticate`], so it
/// answers `401`.
impl<S: Send + Sync> FromRequestParts<S> for Principal {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Principal>()
            .cloned()
            .ok_or_else(|| challenge(StatusCode::UNAUTHORIZED, HeaderValue::from_static("Bearer")))
    }
}

/// Validates HS256 access tokens.
///
/// - `alg` is pinned to HS256, so `none` and every other algorithm are rejected
///   ([RFC 8725 3.1](https://www.rfc-editor.org/rfc/rfc8725#section-3.1)).
/// - `sub`, `exp`, `iss`, and `aud` are required; `iss` must equal the issuer and `aud` must
///   contain the audience.
/// - `nbf` is rejected while it is in the future (beyond the leeway) when present; a token
///   without `nbf` is accepted. `iat` is not checked.
/// - Clock leeway is `jsonwebtoken`'s default of 60 s (`Validation::new` sets `leeway: 60`
///   in jsonwebtoken 11.1.0, src/validation.rs), listed with the MVP limits (api.md).
/// - Scopes come from `scope`, a space-delimited string
///   ([RFC 9068 2.2.3](https://www.rfc-editor.org/rfc/rfc9068#section-2.2.3)).
#[derive(Clone)]
pub struct JwtVerifier(Arc<Inner>);

struct Inner {
    key: DecodingKey,
    validation: Validation,
}

/// The registered claims are checked by `Validation`; only these two are read.
#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    scope: Option<String>,
}

impl JwtVerifier {
    /// `secret` is `AUTH_JWT_SECRET`; its length is checked when the config is resolved.
    pub fn hs256(secret: &[u8], issuer: &str, audience: &str) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["sub", "exp", "iss", "aud"]);
        // `nbf` is checked when the token has one, and never required
        // ([RFC 7519 4.1.5](https://www.rfc-editor.org/rfc/rfc7519#section-4.1.5)).
        validation.validate_nbf = true;
        validation.set_issuer(&[issuer]);
        validation.set_audience(&[audience]);
        Self(Arc::new(Inner {
            key: DecodingKey::from_secret(secret),
            validation,
        }))
    }

    /// The caller behind `token`, or why the token is invalid.
    pub fn verify(&self, token: &str) -> Result<Principal, jsonwebtoken::errors::Error> {
        let data = jsonwebtoken::decode::<Claims>(token, &self.0.key, &self.0.validation)?;
        let scopes = data
            .claims
            .scope
            .as_deref()
            .unwrap_or_default()
            .split(' ')
            .filter(|scope| !scope.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Principal {
            sub: data.claims.sub,
            scopes,
        })
    }
}

/// Never prints the key.
impl fmt::Debug for JwtVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtVerifier")
            .field("algorithms", &self.0.validation.algorithms)
            .field("iss", &self.0.validation.iss)
            .field("aud", &self.0.validation.aud)
            .finish_non_exhaustive()
    }
}

/// Middleware for the protected router: puts the [`Principal`] in the request extensions,
/// or answers `401`.
pub(crate) async fn authenticate(
    State(verifier): State<JwtVerifier>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(request.headers()) else {
        return challenge(StatusCode::UNAUTHORIZED, HeaderValue::from_static("Bearer"));
    };
    match verifier.verify(token) {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(err) => {
            // A fixed reason only: claim values may hold personal data (security.md SEC-13).
            tracing::debug!(
                reason = rejection_reason(err.kind()),
                "bearer token rejected"
            );
            challenge(
                StatusCode::UNAUTHORIZED,
                HeaderValue::from_static(r#"Bearer error="invalid_token""#),
            )
        }
    }
}

/// The credentials of `Authorization: Bearer <token>`. The scheme is case-insensitive
/// ([RFC 9110 11.1](https://www.rfc-editor.org/rfc/rfc9110#section-11.1)). `None` for no
/// header or another scheme. `Bearer` with an empty or whitespace-only token returns an
/// empty token, which fails verification as `invalid_token`.
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ').unwrap_or((value, ""));
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

fn rejection_reason(kind: &ErrorKind) -> &'static str {
    match kind {
        ErrorKind::ExpiredSignature => "expired",
        ErrorKind::InvalidSignature => "bad signature",
        ErrorKind::InvalidAlgorithm | ErrorKind::InvalidAlgorithmName => "algorithm not allowed",
        ErrorKind::InvalidIssuer => "wrong issuer",
        ErrorKind::InvalidAudience => "wrong audience",
        ErrorKind::MissingRequiredClaim(_) => "missing claim",
        _ => "malformed",
    }
}

/// A problem+json response with a `WWW-Authenticate` challenge (`problem_details` keeps it).
fn challenge(status: StatusCode, www_authenticate: HeaderValue) -> Response {
    let mut response = ApiError::from_status(status).into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, www_authenticate);
    response
}

/// Per-route authorization: the [`Principal`] must hold this scope, or the route answers
/// `403` with `WWW-Authenticate: Bearer error="insufficient_scope", scope="<scope>"`.
/// Add it with `route_layer` on the method router, so only matched methods check it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequireScope(pub &'static str);

impl<S> Layer<S> for RequireScope {
    type Service = RequireScopeService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireScopeService {
            inner,
            scope: self.0,
        }
    }
}

/// The service built by [`RequireScope`].
#[derive(Debug, Clone)]
pub struct RequireScopeService<S> {
    inner: S,
    scope: &'static str,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl<S> Service<Request> for RequireScopeService<S>
where
    S: Service<Request, Response = Response, Error = Infallible>,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = BoxFuture<Result<Response, Infallible>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let rejection = match request.extensions().get::<Principal>() {
            Some(principal) if principal.has_scope(self.scope) => None,
            Some(_) => Some(insufficient_scope(self.scope)),
            // Not behind `authenticate`: fail closed.
            None => Some(challenge(
                StatusCode::UNAUTHORIZED,
                HeaderValue::from_static("Bearer"),
            )),
        };
        match rejection {
            None => Box::pin(self.inner.call(request)),
            Some(response) => Box::pin(async move { Ok(response) }),
        }
    }
}

fn insufficient_scope(scope: &str) -> Response {
    let value = HeaderValue::from_str(&format!(
        r#"Bearer error="insufficient_scope", scope="{scope}""#
    ))
    // A scope with characters a header cannot hold still gets the error code.
    .unwrap_or_else(|_| HeaderValue::from_static(r#"Bearer error="insufficient_scope""#));
    challenge(StatusCode::FORBIDDEN, value)
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::routing::get;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use http_body_util::BodyExt;
    use jsonwebtoken::{EncodingKey, Header, get_current_timestamp};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::error::PROBLEM_JSON;
    use crate::routers::protected;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef-test-only";
    const ISSUER: &str = "test-issuer";
    const AUDIENCE: &str = "test-audience";

    fn verifier() -> JwtVerifier {
        JwtVerifier::hs256(SECRET, ISSUER, AUDIENCE)
    }

    fn claims() -> Value {
        let now = get_current_timestamp();
        json!({
            "sub": "alice",
            "scope": "things:read things:write",
            "iss": ISSUER,
            "aud": AUDIENCE,
            "iat": now,
            "exp": now + 3600,
        })
    }

    fn sign(alg: Algorithm, secret: &[u8], claims: &Value) -> String {
        jsonwebtoken::encode(&Header::new(alg), claims, &EncodingKey::from_secret(secret)).unwrap()
    }

    fn token(claims: &Value) -> String {
        sign(Algorithm::HS256, SECRET, claims)
    }

    fn without(claim: &str) -> Value {
        let mut claims = claims();
        claims.as_object_mut().unwrap().remove(claim);
        claims
    }

    fn with(claim: &str, value: Value) -> Value {
        let mut claims = claims();
        claims[claim] = value;
        claims
    }

    async fn whoami(principal: Principal) -> String {
        let scopes: Vec<&str> = principal.scopes.iter().map(String::as_str).collect();
        format!("{}|{}", principal.sub, scopes.join(","))
    }

    fn app() -> Router {
        let api = Router::new()
            .route(
                "/things",
                get(whoami).route_layer(RequireScope("things:read")),
            )
            .route(
                "/admin",
                get(whoami).route_layer(RequireScope("things:admin")),
            );
        protected(api, verifier(), None)
    }

    struct Reply {
        status: StatusCode,
        challenge: Option<String>,
        content_type: Option<String>,
        body: String,
    }

    async fn send(method: &str, path: &str, authorization: Option<&str>) -> Reply {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        let response = app()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let text = |name| {
            response
                .headers()
                .get(name)
                .map(|v: &HeaderValue| v.to_str().unwrap().to_owned())
        };
        let challenge = text(header::WWW_AUTHENTICATE);
        let content_type = text(header::CONTENT_TYPE);
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            challenge,
            content_type,
            body: String::from_utf8(bytes.to_vec()).unwrap(),
        }
    }

    async fn get_things(authorization: Option<&str>) -> Reply {
        send("GET", "/api/v1/things", authorization).await
    }

    async fn assert_invalid_token(token: &str) {
        let reply = get_things(Some(&format!("Bearer {token}"))).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
        assert_eq!(
            reply.challenge.as_deref(),
            Some(r#"Bearer error="invalid_token""#)
        );
        assert_eq!(reply.content_type.as_deref(), Some(PROBLEM_JSON));
        assert!(reply.body.contains(r#""status":401"#), "{}", reply.body);
        assert!(!reply.body.contains(token), "{}", reply.body);
    }

    #[tokio::test]
    async fn valid_token_puts_the_principal_in_the_request() {
        let reply = get_things(Some(&format!("Bearer {}", token(&claims())))).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body, "alice|things:read,things:write");
    }

    #[tokio::test]
    async fn bearer_scheme_is_case_insensitive() {
        let reply = get_things(Some(&format!("bearer {}", token(&claims())))).await;
        assert_eq!(reply.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_header_is_401_without_an_error_code() {
        let reply = get_things(None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.challenge.as_deref(), Some("Bearer"));
        assert_eq!(reply.content_type.as_deref(), Some(PROBLEM_JSON));
        assert!(reply.body.contains(r#""instance":"/api/v1/things""#));
    }

    #[tokio::test]
    async fn non_bearer_scheme_is_401_without_an_error_code() {
        for value in ["Basic YWxpY2U6cHc=", "Token abc", "Basic", "Bearerabc"] {
            let reply = get_things(Some(value)).await;
            assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{value}");
            assert_eq!(reply.challenge.as_deref(), Some("Bearer"), "{value}");
        }
    }

    #[tokio::test]
    async fn empty_bearer_token_is_invalid() {
        for value in ["Bearer", "Bearer ", "Bearer   ", "bearer \t"] {
            let reply = get_things(Some(value)).await;
            assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{value:?}");
            assert_eq!(
                reply.challenge.as_deref(),
                Some(r#"Bearer error="invalid_token""#),
                "{value:?}"
            );
        }
    }

    #[tokio::test]
    async fn garbage_token_is_invalid() {
        for garbage in ["not-a-jwt", "a.b.c", "eyJhbGciOiJIUzI1NiJ9.e30"] {
            assert_invalid_token(garbage).await;
        }
    }

    #[tokio::test]
    async fn bad_signature_is_invalid() {
        let other = b"another-secret-of-at-least-32-bytes!!";
        assert_invalid_token(&sign(Algorithm::HS256, other, &claims())).await;
    }

    #[tokio::test]
    async fn expired_beyond_the_leeway_is_invalid() {
        let now = get_current_timestamp();
        // 60 s leeway: 30 s ago is accepted, 120 s ago is not.
        let reply = get_things(Some(&format!(
            "Bearer {}",
            token(&with("exp", json!(now - 30)))
        )))
        .await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_invalid_token(&token(&with("exp", json!(now - 120)))).await;
    }

    #[tokio::test]
    async fn not_before_in_the_future_is_invalid() {
        let now = get_current_timestamp();
        // 60 s leeway: 30 s ahead is accepted, 120 s ahead is not. No `nbf` is accepted too.
        let reply = get_things(Some(&format!(
            "Bearer {}",
            token(&with("nbf", json!(now + 30)))
        )))
        .await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_invalid_token(&token(&with("nbf", json!(now + 120)))).await;
        let reply = get_things(Some(&format!("Bearer {}", token(&claims())))).await;
        assert_eq!(reply.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn alg_none_is_invalid() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims().to_string());
        assert_invalid_token(&format!("{header}.{payload}.")).await;
    }

    #[tokio::test]
    async fn other_algorithms_are_invalid() {
        for alg in [Algorithm::HS384, Algorithm::HS512] {
            assert_invalid_token(&sign(alg, SECRET, &claims())).await;
        }
    }

    #[tokio::test]
    async fn wrong_issuer_is_invalid() {
        assert_invalid_token(&token(&with("iss", json!("someone-else")))).await;
        assert_invalid_token(&token(&without("iss"))).await;
    }

    #[tokio::test]
    async fn wrong_audience_is_invalid() {
        assert_invalid_token(&token(&with("aud", json!("someone-else")))).await;
        assert_invalid_token(&token(&without("aud"))).await;
        // `aud` may be an array that contains the audience.
        let reply = get_things(Some(&format!(
            "Bearer {}",
            token(&with("aud", json!(["other", AUDIENCE])))
        )))
        .await;
        assert_eq!(reply.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_sub_is_invalid() {
        assert_invalid_token(&token(&without("sub"))).await;
    }

    #[tokio::test]
    async fn missing_exp_is_invalid() {
        assert_invalid_token(&token(&without("exp"))).await;
    }

    #[tokio::test]
    async fn missing_scope_is_403_with_the_required_scope() {
        let reply = send(
            "GET",
            "/api/v1/admin",
            Some(&format!("Bearer {}", token(&claims()))),
        )
        .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN);
        assert_eq!(
            reply.challenge.as_deref(),
            Some(r#"Bearer error="insufficient_scope", scope="things:admin""#)
        );
        assert_eq!(reply.content_type.as_deref(), Some(PROBLEM_JSON));
        assert!(reply.body.contains(r#""status":403"#), "{}", reply.body);
    }

    #[tokio::test]
    async fn token_without_a_scope_claim_has_no_scopes() {
        let reply = get_things(Some(&format!("Bearer {}", token(&without("scope"))))).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unknown_path_is_404_before_auth() {
        for path in ["/api/v1/nowhere", "/nowhere"] {
            let reply = send("GET", path, None).await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(reply.challenge, None);
            assert_eq!(reply.content_type.as_deref(), Some(PROBLEM_JSON));
        }
    }

    #[tokio::test]
    async fn wrong_method_is_401_without_a_token_and_405_with_one() {
        // Auth wraps the whole matched path, including axum's 405 fallback, so the method
        // list is not revealed without a token. RequireScope wraps matched methods only.
        let reply = send("DELETE", "/api/v1/admin", None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        let bearer = format!("Bearer {}", token(&claims()));
        let reply = send("DELETE", "/api/v1/admin", Some(&bearer)).await;
        assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(reply.content_type.as_deref(), Some(PROBLEM_JSON));
    }

    #[tokio::test]
    async fn require_scope_without_authenticate_fails_closed() {
        let router = Router::new().route(
            "/open",
            get(|| async { "reached" }).route_layer(RequireScope("things:read")),
        );
        let response = router
            .oneshot(Request::get("/open").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let text = format!("{:?}", verifier());
        assert!(!text.contains("0123456789abcdef"), "{text}");
        assert!(text.contains(ISSUER), "{text}");
    }
}
