# Rust workspace with clean architecture

**Date**: 2026-10-09 (cut to MVP on 2026-10-10)
**Status**: Proposed
**Kind**: architecture
**Built in**: [assignment feature 2](../assignment/scope.md)

## Summary

We use Rust (axum, tokio, sqlx) with PostgreSQL, Redis, and Kafka. The code is a Cargo workspace with one crate per clean architecture layer. Inner crates cannot see outer frameworks, and the compiler enforces it, so neither a person nor an AI can put code in the wrong layer without the build failing.

## Context

The boilerplate has to be a template that AI tools follow strictly, and prompts alone do not stop an AI from drifting. The enforcement has to be mechanical. Business logic must also be shared by HTTP and Kafka ([docs/conventions/kafka.md](kafka.md)), and the cache must plug in and out without touching use cases ([docs/conventions/cache.md](cache.md)).

## Requirements

**Acceptance criteria**
- **AC-1**: `cargo build --workspace` passes on the stable toolchain pinned in `rust-toolchain.toml`.
- **AC-2**: `domain` and `application` have no dependency on `axum`, `sqlx`, `redis`, `rdkafka`, or `tower`. `cargo deny` bans enforce this inside `make verify`.
- **AC-3**: `make up` starts the stack with one command: postgres, the one-shot `migrate`, and the app always, plus redis with `ENABLE_FEATURES=cache` and kafka (KRaft), kafka-init, and kafka-ui with `ENABLE_FEATURES=kafka`. Those are the compose profiles `cache` and `kafka` ([Compose profiles](https://docs.docker.com/compose/how-tos/profiles/); [runtime.md](runtime.md), Compose topology). With `ENABLE_FEATURES=cache,kafka`, `make up` (or `docker compose up` with `COMPOSE_PROFILES=cache,kafka`) starts everything.
- **AC-4**: `make verify` runs fmt, clippy (`-D warnings`), tests, `cargo deny check`, and `cargo sqlx prepare --check`, and all of them pass.
- **AC-5**: Invalid or missing config stops the app at startup with an error that names the env var. The error never prints a secret value.
- **AC-6**: `GET /healthz` (liveness) and `GET /readyz` (DB, plus Redis when the cache is on) work on the hidden router ([runtime.md](runtime.md)).
- **AC-7**: A single binary provides the subcommands in [runtime.md](runtime.md): `serve`, `consume`, `all` (for dev), `db migrate`, `db rollback`, `db seed`, `info env`, and `info features`. The database engine is pluggable through `DB_PLATFORM` ([database.md](database.md)).
- **AC-8**: Every doc reference in a code comment resolves. `make ref-check`, part of `make verify`, fails when a comment cites a missing file or an ID the file no longer defines, and prints the comment's path and line.

## Options considered (summary)

| Topic | Chosen | Runner-up | Why |
|-------|--------|-----------|-----|
| Web framework | axum | actix-web | Native to the tokio/tower ecosystem, so every tower middleware is available |
| DB access | sqlx (`query!` macros) | diesel, sea-orm | Compile-time checked SQL, async native, no hidden queries |
| Redis client | `redis` + `ConnectionManager` | `fred`, `deadpool-redis` | Multiplexed, auto-reconnecting, and the most widely used |
| Kafka client | `rdkafka` (`cmake-build` feature) | `rskafka` | Mature: consumer groups, SASL, transactions |
| Port dispatch | `#[async_trait]` + `Arc<dyn Port>` | generics | The cache decorator has to be swapped at runtime, and dyn dispatch costs nothing measurable next to IO |
| Where ports live | `application` | `domain` | In clean architecture, the use case layer owns the interfaces it needs. The domain stays pure business rules |

## Decision

### Crate map and dependency rule

```
crates/
  domain/          Entities, value objects, domain errors, domain events      (no IO)
  application/     Use cases + ports (repository and publisher traits)         depends on: domain
  infrastructure/  Postgres repos, Redis cache decorator, Kafka producer       depends on: application, domain
  http-api/        Inbound adapter: axum routes, DTOs, problem+json           depends on: application, domain
  kafka-api/       Inbound adapter: consumer, envelope, DLQ                   depends on: application, domain
  bootstrap/       main, config, wiring (composition root), subcommands        depends on: all
xtask/             Dev tasks: check-skills, seed, token, perf helpers
migrations/        sqlx migrations
```

| Crate | May use | Must not use |
|-------|---------|--------------|
| domain | `thiserror`, `uuid`, `time` | serde, tokio, axum, sqlx, redis, rdkafka |
| application | domain, `async-trait`, `thiserror`, `uuid` (generates v7 ids for HTTP creates). `tokio` with the `rt` feature only, for the `tokio::task_local!` slot in `cache_status` that carries the cache outcome from the decorator in infrastructure to the `X-Cache` middleware in http-api; application is the only crate both can reach ([cache.md](cache.md)). Behind its `serde` feature only, `serde` and `serde_json`, so `input::Field<T>` can be deserialized; only the inbound adapters enable it ([api.md](api.md), input error rules) | axum, sqlx, redis, rdkafka, tower |
| infrastructure | application, domain, sqlx, redis, rdkafka, serde, serde_json (cache values, [cache.md](cache.md) C1) | axum, http-api, kafka-api |
| http-api | application, domain, axum, tower (the `Layer` and `Service` traits behind `RequireScope`), tower-http, serde, jsonwebtoken | sqlx, redis, rdkafka, infrastructure |
| kafka-api | application, domain, rdkafka, serde. Also `serde_json`, `uuid`, and `time` (the envelope fields), `async-trait` (the handler trait), `tokio` and `tokio-util` (the consume loop and its shutdown token), and `tracing` | axum, sqlx, redis, infrastructure, http-api |
| bootstrap | everything, plus `anyhow` | (the only crate allowed to use `anyhow`) |

### Stack

| Layer | Choice |
|-------|--------|
| Language | Rust stable (pinned in `rust-toolchain.toml`), edition 2024 |
| HTTP | axum + tower-http (trace, limit) |
| DB | PostgreSQL 18, sqlx. Local builds and CI compile online against Postgres (the sqlx default when `DATABASE_URL` is set). The offline query data in `.sqlx/` lets the Docker build compile without a DB. `cargo sqlx prepare --check` connects to a live DB ([sqlx-cli](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md)), so CI runs a Postgres service for AC-4 |
| Cache | Redis 8 (Valkey works as a drop-in replacement) |
| Messaging | Apache Kafka 4.x (KRaft), rdkafka |
| IDs | UUIDv7, generated in the app: by the application use case for HTTP creates, and by the producer for Kafka creates (the id is needed before the insert, for events). Timestamps come from the database ([api.md](api.md)) |
| Errors | `thiserror` in every library crate, `anyhow` only in bootstrap |
| Code comments | A file's `//!` header names the convention it implements once. An item cites a rule only where the code makes a choice a reader might "fix" wrongly (security, timeouts, retries, fail-open, redaction); comments never narrate the code. One citation form, at the end of the sentence: `(<file>.md)` or `(<file>.md <ID>)`, several as `(cache.md C6, security.md SEC-6)`. `<file>` is a file in `docs/conventions/`, written bare, or `layers.md` with `section <n>`. IDs are `AC-<n>`, `SEC-<n>`, or a decisions-log ID such as `C9`. `make ref-check` enforces it (AC-8) (own proposal, accepted by the engineer 2026-10-11) |
| Config | 12-factor env vars parsed into a typed `Config`, validated at startup, secrets wrapped with `secrecy` |
| Observability | `tracing` (JSON logs) |
| Tests | Unit tests use in-memory fakes of ports. Integration tests use `testcontainers` |
| Demo load | `oha`, for the single cache on/off comparison in the video ([cache.md](cache.md)) |
| Local env | docker compose and a Makefile |

Crate versions are the latest at scaffold time and are locked by `Cargo.lock`. The spec does not list versions, so it does not go stale.

### Use case style

Each resource has its own module in every layer (a vertical slice inside each layer), for example `application/src/product/`. Its use cases live in one struct, `<Res>UseCases`, which holds `Arc<dyn Port>` fields and has the methods `create`, `get`, `list`, `update`, and `delete`. See O1.

### Error flow

`DomainError` (for example, a broken invariant) maps to `AppError { NotFound, Conflict, Validation(Vec<FieldError>), Unavailable, Unauthorized, Forbidden, Internal }`. `Unavailable` is a dependency that is down (retrying may help); `Internal` is anything unexpected, such as a row that fails to decode or a constraint other than a unique key (retrying will not help). Validation is done by the application parser, which both adapters share ([api.md](api.md), input error rules). Each inbound adapter then maps that on its own terms: http-api to problem+json ([docs/conventions/api.md](api.md)), and kafka-api to an error reply or the DLQ ([docs/conventions/kafka.md](kafka.md)).

## Build plan
1. Workspace and empty crates, `rust-toolchain.toml`, lint config (`[workspace.lints]`) (AC-1)
2. `deny.toml` with bans that match the dependency rule (AC-2)
3. `docker-compose.yml` (with the Kafka topic init step), `.env.example`, Makefile (`up`, `down`, `verify`) (AC-3, AC-4)
4. bootstrap: `Config` + validation, the CLI, feature toggles, and tracing ([runtime.md](runtime.md)) (AC-5, AC-7)
5. The three routers, with health and readiness on hidden (AC-6)
6. CI (GitHub Actions) running `make verify` (AC-4)
7. `make ref-check` in `make verify` (AC-8)

## Consequences
- **Gain:** a layering mistake by an AI fails the build immediately, with no reliance on review.
- **Cost:** more crates than a typical small project. Adding a resource touches 4 to 6 crates, which is exactly what the workflow skills guide.
- `rdkafka` needs cmake, a C toolchain, and the libcurl headers (`libcurl4-openssl-dev`) to build, so the Docker build stage and CI have to provide them. The headers are needed only because librdkafka 2.12.1, bundled by rdkafka-sys 4.10, includes `<curl/curl.h>` in a cmake build even with curl switched off ([`rdkafka_conf.c`](https://github.com/confluentinc/librdkafka/blob/v2.12.1/src/rdkafka_conf.c#L59); master fixed it). The `libz` feature is off, so no zlib is linked.

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## Resolved questions (closed 2026-10-10 with the recommendation)

| # | Question | Decision |
|---|----------|----------|
| O1 | One use case struct per resource, or one struct per use case (`CreateProduct`, ...)? | One per resource. It is easier to read, and the AI imitates it reliably. Split it later if use cases grow complex |
| O2 | Run the Kafka consumer as a separate process, or together with HTTP? | One binary with separate subcommands, so each scales independently. `all` is for dev |
| O3 | Redis 8 or Valkey? | Redis 8, since it matches the brief. Note in the README that Valkey works as a drop-in |

## Deferred

Designed and decided, then cut on 2026-10-10 so the MVP ships by 14 Oct. The original text is kept.

### OpenAPI, metrics, request-id propagation, and query plans
**Deferred:** none of them is needed to demo the brief, and each adds code to every route or crate. **MVP uses:** JSON logs through `tracing`, request examples in the README, and one `oha` run for the video.

Original rows:

| Layer | Choice |
|-------|--------|
| HTTP | axum + tower-http (trace, request-id, timeout, limit, cors) |
| Observability | `tracing` (JSON), request-id propagated into Kafka headers, `/metrics` (Prometheus) |
| API docs | OpenAPI generated from code with `utoipa` |
| Perf | `oha` (HTTP load) and `EXPLAIN (ANALYZE, BUFFERS)` |

The original http-api row also allowed `utoipa`. The original Makefile also had a `perf` target.

### `AppError::VersionMismatch`
**Deferred** on 2026-10-10: the MVP has no `If-Match` and no version check, so nothing can raise it ([api.md](api.md), Deferred). **MVP uses:** last write wins. It returns together with optimistic locking.

Original text: `AppError { NotFound, Conflict, VersionMismatch, Validation(Vec<FieldError>), Unavailable, Unauthorized, Forbidden }`.
