# rs-boilerplate

A Rust CRUD service boilerplate built on clean architecture: axum for HTTP, sqlx with PostgreSQL for storage, JWT auth with per-route scopes, one problem+json error format, and cursor paging. A Redis cache-aside layer and a Kafka command transport are opt-in. Spec-driven workflow skills (`/flow-spec` > `/flow-build` > `/flow-verify` > `/flow-review`) add resources for Claude Code and Codex; the Codex invocation syntax is not yet verified.

**No resource ships.** You add the first one with `/flow-spec` and `/flow-build` (see [Adding a resource](#adding-a-resource)).

## Prerequisites

| Tool | Version | Why |
|------|---------|-----|
| Rust | `1.99.0` ([rust-toolchain.toml](rust-toolchain.toml)) | Build; rustup installs the pinned version |
| Docker with Compose | any recent | The local stack and the integration tests |
| `cmake`, `libcurl4-openssl-dev` | any | rdkafka builds librdkafka with cmake, and that build includes the libcurl headers |
| `sqlx-cli` | `0.9.0` (as in CI) | `make verify` runs `cargo sqlx prepare --check` |
| `cargo-deny` | `0.20.2` (as in CI) | `make verify` enforces the dependency bans |

```bash
cargo install sqlx-cli@0.9.0 cargo-deny@0.20.2 --locked
```

## Quick start

```bash
make up
```

`make up` creates `.env` with a random `AUTH_JWT_SECRET`, builds the image, and starts postgres, the migrations, and the app. `ENABLE_FEATURES` is empty by default, so redis and kafka do not start. To start them too, set `ENABLE_FEATURES=cache,kafka` in `.env` (or just one of them) and run `make down && make up`. The same setting selects the compose profiles.

```bash
curl 127.0.0.1:8079/readyz
```

Get a dev token and call the protected API (the scope names here are examples):

```bash
TOKEN=$(cargo xtask token --sub dev --scope "widgets:read widgets:write")
curl -i -H "Authorization: Bearer $TOKEN" localhost:8080/api/v1/widgets
```

`/api/v1` has no routes until a resource exists, so an unknown path returns 404 with a problem+json body. That is expected.

## Commands

| Command | Does |
|---------|------|
| `make up` / `make down` / `make logs` | Start the stack / stop it, every profile / follow the app logs |
| `make verify` | The quality gate (see [Quality gate](#quality-gate)) |
| `make migrate` / `make seed` | Apply migrations / load seed data to the local database |
| `app serve` | Run the three HTTP routers |
| `app consume` | Run the Kafka consumer (exits non-zero if no handler is registered) |
| `app all` | HTTP and the consumer in one process |
| `app db migrate\|rollback\|seed` | Apply, revert the last, or seed |
| `app info env\|features` | Show the resolved config (secrets redacted) / the enabled features |
| `cargo xtask token --sub <sub> --scope "<scopes>" [--ttl 1h]` | Print a dev access token |
| `cargo xtask kafka-produce --type <res>.<op> --payload '<json>' [--topic <name>] [--repeat N]` | Send a command envelope to `<res>s.commands.v1`; `--topic` overrides an irregular plural; `--raw` sends an unchecked value for the DLQ demo |

Run `app` as `cargo run -p bootstrap -- <command>`; `--help` works on every command.

## Configuration

Each setting is a flag, an environment variable, or a line in `.env`; precedence is flag > env > `.env`. Local values: [.env.example](.env.example). The Default column is the code default. Logging uses `LOG_LEVEL` (a tracing filter), not `RUST_LOG`. `app info env` redacts secrets.

| Variable | Default | Meaning |
|----------|---------|---------|
| `APP_NAME`, `APP_ENV` | `rs-boilerplate`, `local` | Cache key prefix and Kafka consumer group / deployment environment |
| `LOG_LEVEL` | `info` | tracing filter, for example `info,sqlx=warn` |
| `ENABLE_FEATURES` | empty | Comma-separated: `cache`, `kafka` |
| `HOST` | `0.0.0.0` | Address every router binds to |
| `PROTECTED_API_PORT`, `PUBLIC_API_PORT`, `HIDDEN_API_PORT` | `8080`, `8081`, `8079` | Router ports |
| `DB_PLATFORM`, `DATABASE_URL` | `postgres`, none | Database engine and connection URL (`.env.example`: `postgres://app:app@localhost:5432/app`) |
| `REDIS_URL` | none | Needed only with `cache` (`.env.example`: `redis://localhost:6379`) |
| `KAFKA_BROKERS` | none | Needed only with `kafka` (`.env.example`: `localhost:9092`) |
| `AUTH_JWT_SECRET` | none | HS256 secret, at least 32 bytes; the app will not start without it |
| `AUTH_ISSUER`, `AUTH_AUDIENCE` | `rs-boilerplate-dev`, `rs-boilerplate` | Required `iss` and `aud` claims |
| `CACHE_TTL_SECS` | `300` | Default cache TTL |
| `CACHE_DEBUG_HEADER` | `false` | Adds `X-Cache`; refused when `APP_ENV=production` |

### Ports

| Port | What |
|------|------|
| 8080 | Protected API (`/api/v1`, JWT) |
| 8081 | Public router (no auth) |
| 127.0.0.1:8079 | Hidden router: `/healthz`, `/readyz` |
| 127.0.0.1:8090 | kafka-ui (with `kafka`) |

The binary binds the hidden router on `HOST` like the others; only the compose port mapping keeps it on loopback. Values and sources: [runtime.md](docs/conventions/runtime.md).

## Architecture

```
domain  <-  application  <-  infrastructure   (postgres/, redis/)
                         <-  http-api         (axum routers, DTOs)
                         <-  kafka-api        (consumer, envelope, DLQ)
                              bootstrap       (config, CLI, wiring: depends on all)
```

- `domain`: entities and value objects, no IO.
- `application`: use cases and ports (traits); depends on `domain` only.
- `infrastructure`, `http-api`, `kafka-api`: adapters; depend on `application` and `domain`, never on each other.
- `bootstrap`: config, the `app` CLI, and wiring. `xtask/` holds dev tasks and is never a dependency of the app.
- Inner crates never name outer crates or frameworks. `deny.toml` bans enforce this, so a layering mistake fails the build.

Details: [stack-architecture.md](docs/conventions/stack-architecture.md).

## Adding a resource

1. `/flow-spec <resource>`: interview you, write `docs/specs/NNNN-<slug>.md` with acceptance criteria, and add the scope row.
2. `/flow-build <resource>`: implement it layer by layer, inside out, and gate on `make verify`.
3. `/flow-verify <resource>`: prove each acceptance criterion on the running stack.
4. `/flow-review <resource>`: independent review against the conventions, then close the resource.

Skills and their shared contract: [.agents/skills/README.md](.agents/skills/README.md). Rules for AI tools: [AGENTS.md](AGENTS.md).

## Quality gate

```bash
make up
make verify
```

`make verify` needs the stack running (a live database and Docker). It runs `cargo fmt --check`, `clippy -D warnings`, the tests, `cargo deny`, `cargo sqlx prepare --check`, the SQL safety check (`sql-check`), and the doc-reference check (`ref-check`).

## Docs

| Path | Covers |
|------|--------|
| [docs/conventions/](docs/conventions/README.md) | Permanent rules: [stack-architecture](docs/conventions/stack-architecture.md), [api](docs/conventions/api.md), [database](docs/conventions/database.md), [runtime](docs/conventions/runtime.md), [cache](docs/conventions/cache.md), [kafka](docs/conventions/kafka.md), [security](docs/conventions/security.md) |
| [docs/scope/](docs/scope/scope.md) | The service scope and one row per resource |
| [docs/specs/](docs/specs/) | One spec per resource |
| [docs/reports/](docs/reports/) | `flow-verify` and `flow-review` reports |
| [AGENTS.md](AGENTS.md) | Commands, hard rules, and shared building blocks for AI tools |

## Notes

- Valkey works as a drop-in replacement for Redis.
- The Redis and Kafka code is present but inert unless enabled in `ENABLE_FEATURES`; compiling it out was deferred. See "Deferred" in [cache.md](docs/conventions/cache.md) and [kafka.md](docs/conventions/kafka.md) for the reasons and next steps.
