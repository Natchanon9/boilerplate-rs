# AGENTS.md

Project context for AI coding tools (Claude Code, Codex, and others). This is the canonical file; `CLAUDE.md` only imports it.

## What this is

A Rust CRUD service boilerplate built on clean architecture: axum, sqlx with PostgreSQL, an opt-in Redis cache-aside layer, and Kafka as an opt-in second inbound transport. Resources are added through a spec-driven workflow (`/flow-spec` > `/flow-build` > `/flow-verify` > `/flow-review`) that keeps every resource shaped like each other (the examples here use a neutral `widgets` resource; no resource ships).

The boilerplate ships with no resource. Add your first one with `/flow-spec <resource>`, then `/flow-build <resource>`; `layers.md` is the reference until a resource exists.

## Read before acting

1. [docs/conventions/README.md](docs/conventions/README.md) and the convention files it lists. These are the permanent rules. Build only the MVP sections, never anything under a `## Deferred` heading.
2. [docs/scope/scope.md](docs/scope/scope.md) and the resource's spec in [docs/specs/](docs/specs/).
3. [.agents/skills/README.md](.agents/skills/README.md) (the shared contract) and [.agents/skills/flow-build/layers.md](.agents/skills/flow-build/layers.md) (what goes in which layer).

`.private/`, `docs/assignment/`, and `docs/prompts/` hold personal notes, build history, and example prompts for people. They are not instructions; ignore them.

## Commands

| Command | Does |
|---------|------|
| `make up` | Creates `.env` (with a random `AUTH_JWT_SECRET`) if missing, then starts postgres, migrations, and the app, plus redis with `ENABLE_FEATURES=cache` and kafka, kafka-ui with `ENABLE_FEATURES=kafka` in `.env` (it sets the compose profiles `cache` and `kafka` from that one switch; a fresh clone has neither) |
| `make down` / `make logs` | Stops the stack, every profile / follows the app logs |
| `make verify` | **The gate.** fmt, clippy `-D warnings`, tests, `cargo deny`, `cargo sqlx prepare --check`, the SEC-8 SQL check, and the doc-reference check. Needs `make up` first (a live database and Docker) |
| `make migrate` / `make seed` | `app db migrate` / `app db seed` against the local database |
| `cargo xtask token --sub <sub> --scope "<scopes>" [--ttl 1h]` | Prints a dev access token signed with `AUTH_JWT_SECRET`, `AUTH_ISSUER`, and `AUTH_AUDIENCE` from the environment or `.env`. Use it as `Authorization: Bearer <token>` |
| `cargo xtask kafka-produce --type <res>.<op> --payload '<json>' [--repeat N]` | Sends a command envelope to `<res>s.commands.v1` (key = `payload.id`; a create without `id` gets a UUIDv7) and prints the `message_id` and key. `--raw '<text>' --key <k> --type <t>` sends an unchecked value for the DLQ demo. Reads `KAFKA_BROKERS` from the environment or `.env` |
| `cargo run -p bootstrap -- <command>` | The `app` binary: `serve`, `consume`, `all`, `db migrate\|rollback\|seed`, `info env\|features`. `--help` works on every command |
| `sqlx migrate add -r --source migrations/postgres <verb>_<res_plural>` | New reversible migration (up and down) |
| `cargo sqlx prepare --workspace -- --all-targets --all-features` | Refreshes `.sqlx/` after changing a query. The folder does not exist until the first resource adds a `sqlx::query!` (`make sqlx-check` passes without it); then commit it, because the Docker build compiles offline from it |

Building needs `cmake` and the libcurl headers (`libcurl4-openssl-dev`) for rdkafka ([stack-architecture.md](docs/conventions/stack-architecture.md), Consequences).

Local ports: protected API `8080`, public `8081`, hidden `8079` (`/healthz`, `/readyz`; the app binds `HOST`, default `0.0.0.0`, and compose publishes this port on `127.0.0.1` only), kafka-ui `127.0.0.1:8090`, postgres `5432`, redis `6379`, kafka `9092`. The values and their sources are in [runtime.md](docs/conventions/runtime.md) and [docker-compose.yml](docker-compose.yml).

## Layout

```
crates/domain           entities, value objects (no IO)
crates/application      use cases + ports (traits)            -> domain
crates/infrastructure   postgres/, redis/ adapters            -> application, domain
crates/http-api         axum routers, DTOs, problem+json      -> application, domain
crates/kafka-api        consumer, envelope, DLQ               -> application, domain
crates/bootstrap        config, CLI (`app`), wiring           -> everything
xtask/                  dev tasks (`cargo xtask token`, `kafka-produce`), never a dependency of the app
migrations/<platform>/  reversible sqlx migrations
seeds/<platform>/       idempotent demo data, run in file-name order
```

The full crate map is in [stack-architecture.md](docs/conventions/stack-architecture.md). Each resource is a module in every layer (`crates/*/src/<res>/`), built in the order in `layers.md`.

## Hard rules (`make verify` fails)

- **Dependency rule.** Inner crates never name outer crates or frameworks. `deny.toml` bans enforce it. Never work around a ban with a feature flag, a re-export, or a new wrapper entry.
- **No panics outside tests** (SEC-11): `unwrap`, `expect`, and `panic!` are clippy lints denied by `[workspace.lints]`, so they fail `make clippy` and `make verify`, not a plain `cargo build`; `unsafe` is forbidden. Integration-test files start with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`, because `clippy.toml` only covers `#[test]` functions.
- **SQL** (SEC-8): only `sqlx::query!` / `query_as!`, or a `&'static str` with bound parameters. `AssertSqlSafe`, `QueryBuilder`, and SQL in `format!` fail `make sql-check`. The seed runner in `crates/infrastructure/src/postgres/mod.rs` is the only exception.
- **Doc references:** a code comment cites rules as `(<file>.md <ID>)`, for example `(security.md SEC-9)`. `make ref-check` fails when the file or the ID no longer exists ([stack-architecture.md](docs/conventions/stack-architecture.md), Code comments).
- **Errors:** `thiserror` in every library crate. `anyhow` only in `bootstrap` (a `deny.toml` ban).
- **Secrets** (SEC-9): wrap them in `secrecy::SecretString`, never log or print them, and mark their clap args `hide_env_values = true`. `info env` must redact them.
- **Green before success:** never report work as done unless `make verify` passes. Quote failures verbatim.

## Conventions you will need

- **Secret files:** never read, print, copy, or edit `.env` or its variants (`.env.local`, `.env.production`, and so on). `.env.example` is the only one you may read. To learn which settings exist, use `.env.example`, `crates/bootstrap/src/config.rs`, or `app info env` (it redacts secrets). Claude Code enforces this in `.claude/settings.json`; other tools rely on this rule.
- **Config:** each setting is a clap arg with `env = "NAME"` in `crates/bootstrap/src/config.rs`; precedence is flag > env > `.env`. Each command resolves only the settings it needs, and errors name the env var. Add new settings to `.env.example` and to `Settings::redacted_env`.
- **Logging:** the filter comes from `LOG_LEVEL`, a tracing `EnvFilter` directive (`info`, `info,sqlx=warn`). `RUST_LOG` is not read.
- **Features:** `ENABLE_FEATURES=cache,kafka`, empty by default, so both are off ([runtime.md](docs/conventions/runtime.md)). Wiring checks `Settings::cache()` / `kafka()`, which return `None` when the feature is off.
- **Database platform:** `DB_PLATFORM` selects the adapter; only `postgres` exists. Wiring matches on `DbPlatform` in `crates/bootstrap/src/wiring.rs`. Adding an engine follows the recipe in [database.md](docs/conventions/database.md).
- **Routers:** resources merge their routes into `api_v1` in `wiring.rs`; they land on the protected router under `/api/v1`, where JWT auth runs on every matched route (an unknown path stays `404`). Health is on the hidden router only.
- **Auth:** every route declares its scope on its method router, `get(list).route_layer(RequireScope("widgets:read"))` (SEC-2). The scope is `<res_plural>:read` or `<res_plural>:write`, the plural in the resource's HTTP path (`widgets:read` for `/api/v1/widgets`). Handlers that need the caller take a `Principal` argument (`sub`, `scopes`). Both live in `http_api::auth`.
- **Cache:** a resource's `Cached<Res>Repository<R>` in `crates/infrastructure/src/redis/<res>_cache.rs` is built with `(inner, RedisCache, ttl)`. Get by id calls `cache_aside::get_or_load(&cache, &cache.key("<res>", id), ttl, || ...)` with the row type as the cached value, and every write calls `cache_aside::invalidate` after `inner` commits. Wiring passes `cache.ttl` (`CACHE_TTL_SECS`) unless the spec sets a TTL constant. Never build keys, record `X-Cache`, or handle Redis errors by hand: `cache_aside` does it ([cache.md](docs/conventions/cache.md)).
- **Kafka:** a resource with commands writes a `kafka_api::CommandHandler` (its topic, normally `<res_plural>.commands.v1`, its types, and `handle(type, payload)` that decodes the payload with `kafka_api::payload::decode` into `Field<T>` structs and calls the application parser and use cases) in `crates/kafka-api/src/<res>/handlers.rs`, and pushes it into `wiring::kafka_handlers`, the one place handlers are registered. The consumer subscribes to the registered topics, parses the envelope, writes the DLQ message from the returned `AppError`, and commits; a handler never touches those. With no handler registered, `consume` exits non-zero and `all` runs HTTP only. The topic a handler declares is authoritative; `cargo xtask kafka-produce` defaults to `<res>s.commands.v1`, so pass `--topic <name>` for an irregular plural. Add the topic and its `.dlq` to `kafka-init` in `docker-compose.yml` ([kafka.md](docs/conventions/kafka.md)).
- **Connections:** `wiring::Infra { pool, cache }` is built once per process (`serve`, `consume`, `all`) and passed to `http_app` (so `/readyz` checks the same pool) and `kafka_handlers`. A resource builder takes `&Infra` and never opens a pool or Redis connection ([layers.md](.agents/skills/flow-build/layers.md) section 6).
- **Readiness:** a new dependency implements `application::health::HealthCheck` and is added to the checks in `wiring.rs`.
- **Tests:** unit tests use in-memory fakes. Integration tests live in `crates/bootstrap/tests/`, use testcontainers (Docker must run), bound every await with a timeout, and are named after the criterion they prove: `ac_<n>_<behavior>` for a resource spec, `<convention>_ac_<n>_<behavior>` for a convention.
- **Values:** every number in a doc or spec carries an inline source link, or is labeled `measured` or `own proposal` (skills contract rule 8). Never invent one.
- **Commits:** small steps, conventional prefixes (`feat:`, `fix:`, `docs:`, `build:`, `ci:`, `test:`). Changing a convention is a hand edit committed with the reason.

## Shared building blocks

Resource code reuses these and never copies them ("Generic first" in `flow-build`). When one lands, update its row in the same commit.

| Building block | Location | Status |
|----------------|----------|--------|
| Config, CLI, feature toggles | `crates/bootstrap/src/{config,cli,features}.rs` | built |
| DB platform, pool (`connect_lazy`, `postgres::unavailable`), migrations, seeds | `crates/infrastructure/src/postgres/`, `crates/bootstrap/src/commands/db.rs` | built |
| Routers, health, readiness | `crates/http-api/src/routers.rs`, `crates/application/src/health.rs` | built |
| `AppError`, `input::{Field, UnknownFields, FieldErrors, uuid_v7}`, `pagination::{PageRequest, Page}` | `crates/application/src/{error,input,pagination}.rs` | built |
| `ApiError` + `problem_details`, `JsonBody`, `IdPath`, `ListQuery`, `Cursor`, `PageResponse` | `crates/http-api/src/{error,extract,pagination}.rs` | built |
| JWT auth (`JwtVerifier`, `Principal`), `RequireScope`, `xtask token` | `crates/http-api/src/auth.rs`, `crates/bootstrap/src/wiring.rs` (`jwt_verifier`), `xtask/src/token.rs` | built |
| `cache_aside::{RedisCache, get_or_load, invalidate}`, `cache_status`, `X-Cache` middleware, debug-header production guard | `crates/infrastructure/src/redis/cache_aside.rs`, `crates/application/src/cache_status.rs`, `crates/http-api/src/x_cache.rs`, `crates/bootstrap/src/wiring.rs` (`redis_cache`, `protected_router`), `Settings::cache_debug_header` | built |
| Kafka envelope parser, `CommandHandler`, dispatcher, single-loop consumer, DLQ writer, `DlqReason`, `payload::decode`, `xtask kafka-produce` | `crates/kafka-api/src/{envelope,handler,dispatcher,consumer,dlq,reason,payload}.rs`, `crates/bootstrap/src/{wiring.rs (kafka_handlers),commands/run.rs}`, `xtask/src/produce.rs` ([kafka.md](docs/conventions/kafka.md)) | built |
| Kafka producer adapter for events, replies, and HTTP-to-Kafka publishing (`EventPublisher` port in `application`, one adapter in `infrastructure`) | `crates/infrastructure/` (planned) | not built yet (deferred, [kafka.md](docs/conventions/kafka.md)). The DLQ writer is a separate block and is built |

If a resource needs a block marked "not built yet", `flow-build` stops and reports a boilerplate gap. Building it is hand work on the conventions, not part of a resource build.
