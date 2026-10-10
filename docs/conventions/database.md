# Database platform, migrations, and seeds

**Date**: 2026-10-10
**Status**: Proposed
**Kind**: capability
**Built in**: [assignment feature 2](../assignment/scope.md)

## Summary

The database sits behind the repository ports in `application`, so each database engine is an **adapter**. `DB_PLATFORM` chooses the adapter at startup. The MVP ships one adapter, PostgreSQL. A single repository **contract test suite** defines what any adapter must do, so adding an engine is a mechanical recipe: write the adapter, then make it pass the contract.

## Context

The brief does not ask for several databases, but a boilerplate should make swapping one cheap. In Go, boilerplates usually get this from an ORM such as [GORM](https://gorm.io/docs/connecting_to_the_database.html), which supports several engines behind one API. Rust has no equivalent that keeps both portability and compile-time SQL checks:
- `sqlx::Any` selects the engine at runtime, but the `query!` macros check SQL at compile time against one concrete database ([sqlx](https://github.com/launchbadge/sqlx)).
- SQL dialects differ. MySQL has no `RETURNING` ([DataCamp](https://www.datacamp.com/doc/mysql/mysql-returning)) and uses `INSERT IGNORE` rather than `ON CONFLICT`. SQLite has supported `RETURNING` since 3.35 ([SQLite](https://www2.sqlite.org/lang_returning.html)).

So portability lives in the port, not in shared SQL. Each adapter writes SQL natively for its engine, and the contract tests prove the adapters behave the same.

## Requirements
- **AC-1**: `DB_PLATFORM` selects the adapter. It defaults to `postgres`, the only value the MVP accepts. Any other value stops the app at startup with an error that lists the supported values and points to the recipe below ([stack-architecture.md](stack-architecture.md) AC-5).
- **AC-2**: Migrations live in `migrations/<platform>/`, and each one is reversible: it has an up script and a down script ([sqlx-cli: `migrate add -r`, `migrate revert`, `--source`](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md)). `app db migrate` applies pending migrations, and `app db rollback` reverts the last one ([runtime.md](runtime.md)).
- **AC-3**: Seeds live in `seeds/<platform>/*.sql`, run in file-name order by `app db seed`. Each seed uses fixed ids and `ON CONFLICT DO NOTHING`, so running it twice changes nothing.
- **AC-4**: Every repository port has a contract test suite in `application`, written once against the port trait. Both the in-memory fake and every database adapter run it, and all of them must pass.
- **AC-5**: While the database is down, `/readyz` answers `503` within the pool's acquire timeout instead of hanging, and so does every repository call (`503` on HTTP, `unavailable` in the DLQ). The `Unavailable` cause shows the pool state, so the log tells a full pool from an unreachable database (Pool row below).

## Decision

| Topic | Rule |
|-------|------|
| Selection | `DB_PLATFORM` is read into `Config` as an enum. Wiring in `bootstrap` matches on it and builds that platform's repositories. Nothing outside `bootstrap` and the adapter knows which engine runs |
| Adapter layout | `crates/infrastructure/src/<platform>/` holds the pool and one `<res>_repo.rs` per resource, for example `crates/infrastructure/src/postgres/widget_repo.rs` |
| Pool | One pool per process, built by `postgres::connect_lazy` and shared through `wiring::Infra` ([layers.md](../../.agents/skills/flow-build/layers.md) section 6). `max_connections` stays at 10, the sqlx 0.9.0 default ([`PgPoolOptions::new`](https://docs.rs/sqlx-core/0.9.0/src/sqlx_core/pool/options.rs.html), also the [HikariCP](https://github.com/brettwooldridge/HikariCP) `maximumPoolSize` default). `acquire_timeout` is 1 s, against the sqlx default of 30 s, and `acquire_slow_threshold` is 500 ms, against the default of 2 s (own proposal, P1). One-off commands (`db migrate`, `rollback`, `seed`) use `postgres::connect` with the defaults |
| Pool errors | A repository maps a pool or connection error with `postgres::unavailable(&pool, &err)`, never by hand. On `PoolTimedOut` the cause ends in `(open <size>/<max>, idle <n>)`: `open 10/10, idle 0` is a full pool, `open 0/10` is an unreachable database. Only numbers are added, never the URL ([security.md](security.md) SEC-9). `/readyz` logs the same cause |
| SQL | Native to the engine, with compile-time-checked `sqlx::query!` macros ([security.md](security.md) SEC-8). Portability comes from the port and the contract tests, not from shared SQL |
| Migrations | `migrations/postgres/`, created with `sqlx migrate add -r --source migrations/postgres <name>` |
| Seeds | `seeds/postgres/0001_products.sql`: 25 products with fixed UUIDv7 ids (own proposal: more than two pages at the default `limit` of 10, so the demo shows `next_cursor`) |
| Contract tests | `crates/application/src/<res>/contract.rs`, behind a `testing` feature: `pub async fn <res>_repository_contract(repo: &dyn <Res>Repository)`. It covers insert (including `AlreadyExists` for a repeated id), unique conflicts, get, keyset paging, update, and delete. The fake runs it in a unit test. The Postgres adapter runs it in an integration test with testcontainers |

### Recipe: add a database platform
1. Add the variant to the `DbPlatform` enum, and its connection settings to `Config`.
2. Create `crates/infrastructure/src/<platform>/` with a pool constructor and one repository per resource, implementing the same ports. Write SQL natively for the engine, and keep each operation's observable behavior (for example, return the stored row after insert even without `RETURNING`).
3. Create `migrations/<platform>/` and `seeds/<platform>/`, mirroring the Postgres files.
4. Add a match arm in `bootstrap` wiring, and a compose service for local runs.
5. Add an integration test that runs every `<res>_repository_contract` against the new adapter. The platform is supported only when all of them pass.
6. Add the platform to the supported list in AC-1.

## Build plan
1. `DbPlatform` config and the startup check (AC-1)
2. `migrations/postgres/` with reversible migrations, and the `db migrate` and `db rollback` commands (AC-2)
3. `seeds/postgres/` and the `db seed` command (AC-3)
4. The contract-suite pattern, starting with the first resource: the fake and the Postgres adapter both run it (AC-4)
5. Pool timeouts, `postgres::unavailable`, and the database-down readiness test (AC-5)

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## Decisions log

| # | Question | Decision |
|---|----------|----------|
| P1 | The pool used every sqlx default, so `acquire_timeout` was 30 s. With the database down, `/readyz` and every repository call hung that long, because sqlx retries a refused connection until the deadline ([`PoolInner::connect`](https://docs.rs/sqlx-core/0.9.0/src/sqlx_core/pool/inner.rs.html)). Keep it? | Decided 2026-10-11 (engineer): `acquire_timeout` 1 s, the same bound as the Redis connection timeout ([cache.md](cache.md) C9) and the Kubernetes readiness probe `timeoutSeconds` default ([`Probe`](https://raw.githubusercontent.com/kubernetes/api/master/core/v1/types.go)). `acquire_slow_threshold` 500 ms, so sqlx's slow-acquire warning fires before the timeout; at the 2 s default it never would. The timeout bounds only getting a connection, not a query. Measured 2026-10-11 on a WSL2 dev machine (Linux 6.18, Docker via testcontainers, Postgres 18.6), three runs each: healthy `/readyz` (2000 calls) p50 0.87 to 0.90 ms, p99 1.26 to 1.32 ms, p99.9 35 to 41 ms, so 1 s leaves more than 20x headroom. Database down, before: probe 1 1.5 ms or 4.1 s, probes 2 and 3 30.0 s each. After: probe 1 1.1 ms (once) or 1.0 s, probes 2 and 3 1.0 s each (1.0009 to 1.0030 s over the nine probes). `database_ac_5_readyz_fails_fast_while_the_database_is_down` asserts each probe stays under 5 s, which still catches the 30 s hang. A full pool now fails at 1 s instead of queueing; accepted for the MVP, revisit with the N4 request timeout ([api.md](api.md)) |

## Deferred

### More adapters (SQLite, MySQL), as the recommended path for the future
**Deferred:** one adapter is enough to show the seam within the 14 Oct deadline. A second adapter means a second repository for every resource, which also doubles what the AI must generate in the live demo. **MVP uses:** Postgres only, plus the recipe and the contract tests above, so a second adapter can be added without touching use cases.

Notes for when this returns:
- **SQLite** has no UUID or timestamp column types. Store them as `TEXT` (RFC 3339) and convert them in the adapter. `RETURNING` is available from 3.35 ([SQLite](https://www2.sqlite.org/lang_returning.html)).
- **MySQL** has no `RETURNING` ([DataCamp](https://www.datacamp.com/doc/mysql/mysql-returning)). Insert, then select the row in the same transaction. Use `INSERT IGNORE` or `ON DUPLICATE KEY` for idempotent creates, and remember that `INSERT IGNORE` suppresses more than unique violations.
- sqlx macros check against one `DATABASE_URL` per build ([sqlx](https://github.com/launchbadge/sqlx)). So either give each adapter its own crate, with its own offline query data, or use runtime `sqlx::query_as` with bound parameters in the second adapter. Bound parameters still satisfy SEC-8.

### TLS to the database
**Deferred** on 2026-10-10: the local and CI databases run without TLS, and the demo needs nothing more. **MVP uses:** plaintext to Postgres, so a managed database that requires TLS cannot be reached yet.

Notes for when this returns: enable one of sqlx's TLS features (for example `tls-rustls-ring-webpki`), set `sslmode=verify-full` in `DATABASE_URL` ([PostgreSQL SSL modes](https://www.postgresql.org/docs/current/libpq-ssl.html#LIBPQ-SSL-SSLMODE-STATEMENTS)), allow the TLS crates' licenses in `deny.toml`, and add `ca-certificates` to the runtime image.

### Options considered (and not chosen)

| Option | Pros | Cons |
|--------|------|------|
| `sqlx::Any` with one set of SQL | Switch engines with a URL | No compile-time SQL checks; SQL limited to what every engine supports (no `RETURNING`, no `ON CONFLICT`) |
| SeaORM ([docs](https://www.sea-ql.org/SeaORM/docs/install-and-config/database-and-async-runtime/)) | Closest to a multi-engine ORM like GORM: one entity model for MySQL, Postgres, and SQLite, plus up/down migrations in Rust ([migrations](https://www.sea-ql.org/SeaORM/docs/migration/writing-migration/)) | Replaces the sqlx decision in [stack-architecture.md](stack-architecture.md), drops compile-time SQL checks, and is a new API to learn within four days |
