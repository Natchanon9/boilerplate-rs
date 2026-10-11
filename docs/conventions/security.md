# Security baseline

**Date**: 2026-10-09 (cut to MVP on 2026-10-10)
**Status**: Accepted
**Kind**: capability
**Built in**: [assignment feature 4](../assignment/scope.md)

## Summary

This is the security standard every resource inherits automatically. It is mapped to the [OWASP API Security Top 10 (2023)](https://owasp.org/API-Security/editions/2023/en/0x11-t10/) and enforced by code or tooling wherever possible. `/flow-review` cites these rule IDs in its findings.

The service is an **OAuth 2.0 resource server**: it validates JWT access tokens but never issues them in production. Sign-up, login, and user management belong to an identity provider (IdP), not to this boilerplate. The MVP validates **HS256** tokens signed with a shared secret. Asymmetric keys and JWKS are designed but deferred (see [Deferred](#deferred)).

## Authentication (MVP)

- **Algorithm:** HS256 only, pinned in the validator. Any other `alg`, including `none`, is rejected, which blocks algorithm-confusion attacks ([RFC 8725 3.1](https://www.rfc-editor.org/rfc/rfc8725#section-3.1)).
- **Secret:** `AUTH_JWT_SECRET` from the environment, wrapped in `secrecy::SecretString`. It must be at least 32 bytes ([RFC 7518 3.2](https://www.rfc-editor.org/rfc/rfc7518#section-3.2): an HS256 key must be at least as large as the hash output, 256 bits). The app refuses to start with a shorter secret. `.env` is gitignored, and `.env.example` holds only a placeholder.
- **Claims** checked (decided 2026-10-11):
  - `sub` and `exp` are required. `exp` is enforced.
  - `iss` must equal `AUTH_ISSUER`, and `aud` must contain `AUTH_AUDIENCE`. Both are required.
  - `nbf` is enforced when the token has one: a token not yet valid is `401` `invalid_token`. It is not required ([RFC 7519 4.1.5](https://www.rfc-editor.org/rfc/rfc7519#section-4.1.5): the token "MUST NOT be accepted for processing" before `nbf`).
  - `iat` is not checked and not required.
  - Clock leeway is the library default of 60 s, applied to `exp` and `nbf` ([api.md](api.md), MVP limits).
  - The scope claim is `scope` only; `scp` is not read (see Deferred). The 32-byte secret rule is checked when the config is resolved, not by the JWT verifier.
- **Scopes:** read from `scope`, a space-delimited string ([RFC 9068](https://www.rfc-editor.org/rfc/rfc9068)). On success, a `Principal { sub, scopes }` is placed in the request extensions, and `RequireScope("widgets:write")` enforces authorization per route. The standard scope names are `<res_plural>:read` and `<res_plural>:write`, with the plural from the resource's HTTP path (`widgets:read` for `/api/v1/widgets`).
- **Responses** (decided 2026-10-10), per [RFC 6750 3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1). Every body is problem+json ([api.md](api.md), error responses):
  - No `Authorization` header, or a scheme other than `Bearer`: `401` with `WWW-Authenticate: Bearer` and no error code. Only these two cases get the bare challenge. RFC 6750 3.1: when "the request lacks any authentication information (e.g., the client [...] attempted using an unsupported authentication method), the resource server SHOULD NOT include an error code or other error information".
  - `Bearer` with an empty or whitespace-only token, a malformed token, a bad signature, an expired token, any `alg` other than HS256 (including `none`), a wrong `iss` or `aud`, a missing `sub` or `exp`, or an `nbf` still in the future beyond the leeway: `401` with `WWW-Authenticate: Bearer error="invalid_token"`.
  - A valid token without the route's scope: `403` with `WWW-Authenticate: Bearer error="insufficient_scope", scope="<required scope>"`.
- **Unknown paths:** auth runs only on routes the protected router matches (axum `route_layer`), so an unknown path returns the problem+json `404` even without a token (decided 2026-10-10).
- **Dev tokens:** `cargo xtask token --sub alice --scope "widgets:read widgets:write" --ttl 1h` signs with the same `AUTH_JWT_SECRET` and prints a token (decided 2026-10-10):
  - `--sub` and `--scope` (space-delimited) are required. `--ttl` defaults to `1h` (own proposal: one demo session, as in the example above) and accepts `<n>s`, `<n>m`, or `<n>h`.
  - It reads `AUTH_JWT_SECRET`, `AUTH_ISSUER`, and `AUTH_AUDIENCE` from the environment or `.env` (the environment wins, as for the app), and refuses a secret shorter than 32 bytes, like the app.
  - It signs HS256 with the claims `sub`, `scope`, `iss`, `aud`, `iat`, and `exp`, and prints only the token to stdout. Errors name the flag or env var and never print the secret (SEC-9).
- **Libraries:** `jsonwebtoken` only. Our own code is the extractor and `RequireScope`.

## Requirements (rule IDs)

| ID | OWASP | Control | Enforced by |
|----|-------|---------|-------------|
| SEC-1 | API2 Broken Authentication | JWT validation as above, on every route of the protected router. The hidden router (`/healthz`, `/readyz`) has no auth and must not be routed outside the deployment ([runtime.md](runtime.md), Hidden router exposure) | Middleware + tests |
| SEC-2 | API5 BFLA | Every route declares its required scope: `<res_plural>:read` or `<res_plural>:write`, the plural of its HTTP path (`widgets:read` for `/api/v1/widgets`) | `RequireScope` + 403 tests |
| SEC-3 | API1 BOLA | An owned resource's spec defines an ownership rule, and its queries filter by owner (`sub`) | Resource spec + tests |
| SEC-4 | API3 BOPLA / mass assignment | Request DTOs are allowlists: every unknown field is rejected as `unknown_field`, and all of them are reported ([api.md](api.md), input error rules). Response DTOs list their fields explicitly. Entities and rows are never serialized directly | Review + tests |
| SEC-5 | API4 Resource consumption | Body limit and page size cap ([api.md](api.md), MVP limits). Rate limit and request timeout are deferred | Middleware |
| SEC-6 | API8 Misconfiguration | No internal detail in errors. `X-Cache` only when `CACHE_DEBUG_HEADER=true`, and the app refuses to start with `CACHE_DEBUG_HEADER=true` and `APP_ENV=production` (`production` is the reserved value, own proposal; [cache.md](cache.md) C6). CORS and security headers are deferred | Config + tests |
| SEC-7 | API10 Unsafe consumption | Kafka messages pass the same validation as HTTP input | The shared application parser ([api.md](api.md), input error rules) |
| SEC-8 | Injection | SQL only through `sqlx::query!` / `query_as!` or bound parameters. Never `format!` | Grep check in `make verify` |
| SEC-9 | Secrets (`info env` redacts them, [runtime.md](runtime.md)) | Secrets come from env or files, wrapped in `secrecy::SecretString`. `.env.example` holds no real values. The `authorization` header is never logged | Types + tracing redaction |
| SEC-10 | Supply chain | `cargo deny check` (advisories, licenses, bans, sources) | `make verify` + CI |
| SEC-11 | Memory / panic safety | `#![forbid(unsafe_code)]`. No `unwrap`, `expect`, or `panic!` outside tests (clippy `unwrap_used`, `expect_used`, `panic`) | `[workspace.lints]` |
| SEC-13 | Logging | Structured logs. No PII or tokens. Request ids are deferred | Review |

SEC-12 is deferred.

## Build plan
1. `[workspace.lints]` (SEC-11) and `deny.toml` (SEC-10)
2. `xtask token` (SEC-1)
3. JWT extractor and `RequireScope`, with 401 and 403 tests, including the expired-token, wrong-`alg`, and `alg: none` cases (SEC-1, SEC-2)
4. Secret types + redaction, and the startup check on the secret length (SEC-9)
5. SQL grep check in `make verify` (SEC-8)

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## Deferred

Designed and decided, then cut on 2026-10-10 to keep the MVP small. The original text is kept.

### Asymmetric keys, JWKS, and dev key pairs (S1 as decided on 2026-10-09)
**Deferred:** JWKS fetching and caching, key generation, and `kid` handling are more code than a demo needs. **MVP uses:** HS256 with a shared secret from the environment. Switching later changes the key source and the algorithm allowlist, not the extractor's interface.

Original text (Authentication design, decided 2026-10-09, S1):

#### Principles
- Validate access tokens per **[RFC 9068](https://www.rfc-editor.org/rfc/rfc9068)** (JWT profile for OAuth 2.0 access tokens) and **[RFC 8725](https://www.rfc-editor.org/rfc/rfc8725)** (JWT best current practices).
- Use **asymmetric keys only** (ES256 by default, RS256 also allowed), with an allowlist of algorithms. This blocks `alg: none` and the HS/RS algorithm-confusion attack.
- Use the **same validation path in every environment.** Only the key source changes.

#### Key source, chosen by config
| `AUTH_JWKS_URL` / `AUTH_JWKS_FILE` | Used for | Token issued by |
|------------------------------------|----------|-----------------|
| `AUTH_JWKS_URL=https://idp/.well-known/jwks.json` | staging and prod | Any OIDC IdP (Keycloak, Auth0, Entra ID, Cognito, Zitadel, Rauthy) |
| `AUTH_JWKS_FILE=.secrets/dev/jwks.json` | local, CI, demo | `cargo xtask token` |

- **JWKS URL:** keys are cached in memory with a TTL. An unknown `kid` triggers one refresh, rate-limited to stop a key-flooding DoS.
- **Dev keys:** `make setup` (`cargo xtask dev-keys`) generates an ES256 key pair into `.secrets/dev/`, which is gitignored. No private key is ever committed, not even a dev one.
- **Dev tokens:** `cargo xtask token --sub alice --scope "widgets:read widgets:write" --ttl 1h` signs with the dev private key and prints a token.
- **Nothing else runs.** No extra container, no extra service. Pointing at a real IdP later is a config change, not a code change.

#### Validation rules
- Require `kid`, `iss` = `AUTH_ISSUER`, `aud` contains `AUTH_AUDIENCE`, and the `exp`, `nbf`, and `iat` claims (leeway: [docs/conventions/api.md](api.md) N6).
- Require `sub`.
- Scopes come from `scope` (a space-delimited string, per [RFC 9068](https://www.rfc-editor.org/rfc/rfc9068)), falling back to `scp` (an array, for IdPs that use it).
- On success, a `Principal { sub, scopes }` is placed in the request extensions. `RequireScope("widgets:write")` enforces authorization per route.
- Responses are `401` for a missing or invalid token (with `WWW-Authenticate: Bearer error="invalid_token"`) and `403` for a missing scope (`insufficient_scope`), per [RFC 6750 3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1).

#### Libraries
- `jsonwebtoken` for decoding and validation, using its `jwk::JwkSet` and `DecodingKey::from_jwk`.
- `reqwest` to fetch the JWKS (URL mode only).
- About 150 lines of our own code: the extractor, the JWKS cache, and `RequireScope`. No auth framework is needed, because a resource server does not manage users or sessions.


### Rate limit, request timeout, CORS, and security headers (parts of SEC-5 and SEC-6)
**Deferred:** a rate limit needs a business number (open question P4, now in [api.md](api.md) Deferred), and a timeout needs a measured baseline. The API is service-to-service, so CORS does not apply yet. **MVP uses:** the body limit and the page size cap.

Original rows:

| ID | OWASP | Control | Enforced by |
|----|-------|---------|-------------|
| SEC-5 | API4 Resource consumption | Rate limit keyed by token `sub` (`tower_governor`), body limit, timeout, page size cap (values: [docs/conventions/api.md](api.md) N4, N7, N8) | Middleware |
| SEC-6 | API8 Misconfiguration | CORS allowlist, security headers, `/docs` and `X-Cache` off in prod, no internal detail in errors | Config + tests |
| SEC-1 | API2 Broken Authentication | JWT validation as above, on every route except `/healthz`, `/readyz`, and `/metrics` (those three sit behind network policy) | Middleware + tests |
| SEC-13 | Logging | Structured logs with a request-id. No PII or tokens | Review |

### Hardened container image (SEC-12)
**Deferred:** distroless and non-root are hardening steps that do not change the demo. **MVP uses:** a plain multi-stage Dockerfile.

| ID | OWASP | Control | Enforced by |
|----|-------|---------|-------------|
| SEC-12 | Container | Multi-stage build, distroless image, non-root user | Dockerfile |

### Mock OIDC provider (S2)

| # | Question | Decision (at the time) |
|---|----------|------------------------|
| S2 | Add an optional mock OIDC server to compose (for example, `navikt/mock-oauth2-server`) to demo a real token endpoint? | Deferred. It is nice to have, but it costs time and adds nothing to what the service itself does |

Original build plan:
1. `[workspace.lints]` (SEC-11) and `deny.toml` (SEC-10)
2. `xtask dev-keys` and `xtask token` (SEC-1)
3. JWT extractor, JWKS file and URL sources, `RequireScope`, with 401/403 tests including the alg-confusion and expired-token cases (SEC-1, SEC-2)
4. Rate limit, CORS, security headers (SEC-5, SEC-6)
5. Secret types + redaction (SEC-9)
6. SQL grep check in `make verify` (SEC-8)
7. Dockerfile (SEC-12)
