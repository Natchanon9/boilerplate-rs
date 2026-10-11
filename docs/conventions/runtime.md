# Runtime: CLI, configuration, feature toggles, and routers

**Date**: 2026-10-10
**Status**: Accepted
**Kind**: capability
**Built in**: [assignment feature 2](../assignment/scope.md)

## Summary

The service is one binary with one CLI:
- `serve`, `consume`, and `all` run the processes.
- `db` manages the database.
- `info` shows the configuration.

Configuration comes from flags, then environment variables, then `.env`, so the environment alone can configure a deployment ([The Twelve-Factor App: config](https://12factor.net/config)) while a flag still wins for a one-off run. Optional features are switched on with `ENABLE_FEATURES`, and none is on by default. HTTP is split into three routers on three ports (public, protected, and hidden), so health checks never share a port with the API.

## Requirements
- **AC-1**: The CLI provides:
  - `serve` (all HTTP routers)
  - `consume` (the Kafka consumer)
  - `all` (both, for local development)
  - `db migrate`, `db rollback`, and `db seed` ([database.md](database.md))
  - `info env` and `info features`

  `--help` works on every command.
- **AC-2**: A value given as a flag wins over the environment variable, which wins over `.env`. Every setting can be given by environment variable alone.
- **AC-3**: `info env` prints the effective configuration with every secret redacted: `AUTH_JWT_SECRET`, and the password inside `DATABASE_URL` ([security.md](security.md) SEC-9).
- **AC-4**: `ENABLE_FEATURES` is a comma-separated list, empty by default, so every feature is off. In the MVP it accepts `cache` and `kafka`; an unknown name stops the app at startup and the error lists the supported names.
  - With `cache` disabled, the repository is not wrapped by the cache decorator, and every GET is a BYPASS ([cache.md](cache.md)).
  - With `kafka` disabled, `consume` refuses to start, and `all` runs only the HTTP routers.
  - With `kafka` enabled but no Kafka handler registered in `wiring.rs` (no resource declares commands yet), `consume` exits non-zero with a message that says so, and `all` runs only the HTTP routers and logs a warning ([kafka.md](kafka.md), consumer semantics).
  - If the consumer stops because a DLQ write failed, the process exits non-zero in both `consume` and `all` ([kafka.md](kafka.md)).
  - `info features` lists each feature as enabled or disabled.
- **AC-5**: Three routers run on separate ports:
  - **protected** carries every `/api/v1` route behind JWT auth
  - **hidden** carries `/healthz` and `/readyz`, with no auth, and must stay off the network outside the deployment. The app binds it on `HOST` like the other two, because Kubernetes probes reach the pod IP, so only the deployment keeps it private: compose publishes it on `127.0.0.1` only, and production adds a network policy and no ingress route
  - **public** carries routes that need no auth. It is empty in the MVP

## Decision

| Topic | Rule |
|-------|------|
| CLI | `clap` with derive. Subcommands as in AC-1 |
| Precedence | Each setting is a `clap` argument with `env = "NAME"`. A flag takes priority, and the environment variable is used only "when argument is not present" ([clap `Arg::env`](https://docs.rs/clap/latest/clap/struct.Arg.html#method.env), feature `env`). `.env` is loaded first with `dotenvy::dotenv()`, which keeps any variable already set in the environment ([dotenvy](https://docs.rs/dotenvy/latest/dotenvy/fn.dotenv.html)). Together this gives flag > env > `.env` |
| Feature toggles | `ENABLE_FEATURES=cache,kafka`: one setting lists every optional feature that is on, so a new feature needs no new flag (own proposal). An unset or empty value means every feature is off. Decided by the engineer, 2026-10-10: a fresh clone starts only what it uses, so Redis and Kafka are opt-in. This replaces the earlier `CACHE_ENABLED` flag, so there is only one switch. `CACHE_DEBUG_HEADER` stays a separate dev setting |
| Ports | `PROTECTED_API_PORT` 8080, `PUBLIC_API_PORT` 8081, and `HIDDEN_API_PORT` 8079, with `HOST` 0.0.0.0. 8080 is the registered HTTP alternate port ([IANA `http-alt`](https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml?search=8080)). The public and hidden routers take its neighbours 8081 and 8079, so the three are easy to remember (own proposal) |
| Hidden router exposure | The app binds the hidden router on `HOST:HIDDEN_API_PORT` (default `0.0.0.0:8079`), like the other two routers, because Kubernetes probes reach the pod IP (engineer decision, 2026-10-11). The restriction is in the deployment, not the binary: in docker compose the hidden port is published on `127.0.0.1` only (`docker-compose.yml`, `app` ports), so local tools such as `flow-verify` can reach `/readyz` while the network cannot (own proposal), and in production a network policy and no ingress route keep it private |
| Infrastructure ports | Published on `127.0.0.1` only, as listed in `docker-compose.yml`: postgres `5432`, redis `6379`, kafka `9092` (host listener), and kafka-ui `8090` (mapped to the container's `8080`; 8090 because the protected router uses 8080, own proposal). |
| Logging | The filter comes from `LOG_LEVEL`, a `tracing_subscriber::EnvFilter` directive such as `info` or `info,sqlx=warn` ([EnvFilter](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)); default `info`. `RUST_LOG` is not read, so setting it has no effect |
| `/readyz` | Checks the database, plus Redis when `cache` is enabled |
| Settings names | Connections are URLs: `DATABASE_URL`, `REDIS_URL`, and `KAFKA_BROKERS` (comma-separated `host:port`). `APP_NAME` (default `rs-boilerplate`) and `APP_ENV` (default `local`) name the service and its environment, for the cache key ([cache.md](cache.md)) and the consumer group ([kafka.md](kafka.md)). The name is `APP_ENV`, not `ENV`, because POSIX `sh` reads `ENV` as a file to source ([POSIX sh, ENV](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sh.html)) |
| Compose topology | One `app` container runs `all`, so HTTP and the consumer share one environment, and the cache toggle cannot differ between them (a consumer with the cache off would never delete keys that HTTP reads). A one-shot `migrate` container runs `db migrate` before it. `redis` is in compose profile `cache`, and `kafka`, `kafka-init`, and `kafka-ui` are in profile `kafka`; a service with a profile starts only when that profile is active, and a service with none always starts ([Compose profiles](https://docs.docker.com/compose/how-tos/profiles/)). The profile names equal the feature names, so `ENABLE_FEATURES` is the single switch: `make up` copies it into `COMPOSE_PROFILES` ([COMPOSE_PROFILES](https://docs.docker.com/compose/how-tos/environment-variables/envvars/#compose_profiles)), and `make down` stops every profile with `--profile '*'` (own proposal, checked on Docker Compose v5.5.1). The `app` marks `redis` and `kafka-init` in `depends_on` with `required: false`, so it starts when their profile is off ([Compose spec, `depends_on`](https://docs.docker.com/reference/compose-file/services/#depends_on): "When set to `false` Compose only warns you when the dependency service isn't started or available"). Plain `docker compose up` needs `COMPOSE_PROFILES` set by hand to the same value. Production runs `serve` and `consume` as separate deployments ([stack-architecture.md](stack-architecture.md) O2) |

## Build plan
1. `clap` CLI with the subcommands, `dotenvy`, and typed `Config` (AC-1, AC-2)
2. `info env` with redaction, and `info features` (AC-3, AC-4)
3. `ENABLE_FEATURES` parsing, and its use in wiring (AC-4)
4. Three routers on three listeners, with `/healthz` and `/readyz` on hidden (AC-5)
5. Compose: publish protected and public, and publish hidden on `127.0.0.1` only (AC-5; the app itself binds `HOST`)

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## Deferred

### Dev mode flag, watcher, and tasks
**Deferred:** a `--dev` flag, a background watcher, and one-off tasks were considered. None of them is needed to demo the brief. **MVP uses:** `LOG_LEVEL` and `CACHE_DEBUG_HEADER` for development, and `db seed` for demo data.
