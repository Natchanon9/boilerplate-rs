# Toggleable cache-aside with Redis

**Date**: 2026-10-09 (cut to MVP on 2026-10-10)
**Status**: Proposed
**Kind**: capability
**Built in**: [assignment feature 6](../assignment/scope.md)

## Summary

Reads by id check Redis first. On a miss they read Postgres and write the result back to Redis. Writes commit to the DB first and then delete the key. The cache is a decorator around the repository, so config can switch it on or off without touching any use case. If Redis is down, the service keeps working.

## Context

The brief asks for cache-aside, a delete after each write, and the ability to turn the cache on or off. Putting cache logic inside use cases would duplicate it in every resource and make use cases harder to test.

## Requirements

- **AC-1**: With the `cache` feature enabled (the default), the first GET is a MISS and the next one is a HIT (visible in `X-Cache` and in the logs). HIT, MISS, and ERROR are `debug` events, so the demo runs with `LOG_LEVEL=info,infrastructure=debug` (decided 2026-10-10).
- **AC-2**: After a successful `PUT` or `DELETE`, the next GET is a MISS and returns the new value (or 404).
- **AC-3**: Without `cache` in `ENABLE_FEATURES` ([runtime.md](runtime.md)), the default, every request is a BYPASS, and Redis holds no app keys.
- **AC-4**: If Redis is down, the API still returns 200 (fail-open) and logs a warning.
- **AC-5**: TTL is `CACHE_TTL_SECS`, default 300 s ([api.md](api.md), MVP limits). A resource spec may set its own TTL:
  - The decorator takes `ttl: Duration` at construction. Wiring passes `CACHE_TTL_SECS` by default.
  - A resource whose spec sets its own TTL passes a constant instead, and the constant links to that spec (for example `pub const PRODUCT_CACHE_TTL: Duration = ...; // docs/specs/0001-products.md`).
  - There are no per-resource env vars (decided 2026-10-10).

AC-6 and AC-7 are deferred.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| **A. Decorator around the repository port** (chosen) | Use cases never see the cache. Toggled at wiring time. Testable in isolation | If a write ever spans several repositories in one transaction, invalidation must move to a post-commit hook |
| B. Call the cache from use cases | Fine-grained control | Duplicated in every resource, and the application layer has to know about the cache |
| C. Axum middleware keyed by URL | Quick to build | Does not cover Kafka, and URL-based invalidation is imprecise |

## Decision

Option A, made of these parts:
- A generic helper `cache_aside::get_or_load(cache, key, ttl, loader)` in infrastructure, written once, with `cache_aside::invalidate(cache, key)` for writes. `cache` is a `RedisCache`: the `ConnectionManager` plus the key prefix, built once in wiring.
- A thin per-resource decorator, `Cached<Res>Repository<R>`, built with `(inner, cache, ttl)`. Reads go through `get_or_load`. Writes call `inner.write()`, then `invalidate` the key.
- Wiring in bootstrap: `if features.cache { Arc::new(Cached...::new(repo, redis, ttl)) } else { Arc::new(repo) }`, where `features` comes from `ENABLE_FEATURES` ([runtime.md](runtime.md)).

| Topic | Value |
|-------|-------|
| Key | `{APP_NAME}:{APP_ENV}:v1:{resource}:{id}` (bump `v1` when the value shape changes), built by `RedisCache::key("<res>", id)` |
| Value | JSON of the infrastructure row struct (see C1), written with `SET key json EX ttl` |
| What is cached | Get by id only. Lists are not cached, because invalidating query results is not worth the complexity |
| Read path | HIT: decode and return. MISS: run the loader, then `SET` only when it returns a row (no negative caching). A Redis error fails open: run the loader, log a warning, and do not `SET` (ERROR) |
| Undecodable value | A cached value that fails to decode (the shape changed without bumping `v1`, or the data is corrupt) counts as a MISS: log a warning, load from the DB, and `SET` the fresh value over it. The client still gets 200 (C4) |
| Write path | DB commit, then `DEL`. If the DEL fails, log a warning and never fail the write. The TTL is the safety net |
| Create | Does not write to the cache (lazy load on read) |
| Logs | HIT, MISS, and ERROR are `debug` events from `infrastructure::redis::cache_aside`. Redis failures and undecodable values are warnings. To see the events, run with `LOG_LEVEL=info,infrastructure=debug` (C5). The Redis URL is never logged, because it may hold a password ([security.md](security.md) SEC-9) |
| Redis timeouts | Response timeout 500 ms and connection timeout 1 s, the redis-rs defaults ([`DEFAULT_RESPONSE_TIMEOUT`, `DEFAULT_CONNECTION_TIMEOUT`](https://docs.rs/redis/1.7.1/src/redis/client.rs.html#180-182)). Reconnect retries are set to 0, against the library default of 6 retries from 100 ms with exponential backoff ([`ConnectionManagerConfig` defaults](https://docs.rs/redis/1.7.1/src/redis/aio/connection_manager.rs.html#113-115)), because commands issued while a reconnect is in progress wait for it ([`ConnectionManager`, Behavior](https://docs.rs/redis/1.7.1/redis/aio/struct.ConnectionManager.html)) (C9). While Redis is down, each cache call makes at most one connection attempt and then fails open, so one call is bounded by the 1 s connection timeout plus the 500 ms response timeout. Measured numbers are in C9 |
| `X-Cache` header | `HIT`, `MISS`, `ERROR` (Redis failed and the read fell back to the DB), or `BYPASS` (the `cache` feature is not enabled) (C6). The decorator records the outcome in a `tokio::task_local!` slot (`application::cache_status`), and the `x_cache` middleware on the protected router turns it into the header. Enabled only with `CACHE_DEBUG_HEADER=true`. The app refuses to start with `CACHE_DEBUG_HEADER=true` and `APP_ENV=production`; `production` is the reserved value, matched in any letter case (own proposal) |
| Which responses carry `X-Cache` | Only authenticated `GET`s on a matched route. With the cache enabled, only a response whose request read the cache (get by id) carries it, so lists and writes do not. With the cache disabled there is no decorator, so the middleware reports `BYPASS` on every such `GET`, lists included (AC-3). A `401` or `403` never carries it, in any mode, so `BYPASS` never appears on an auth rejection (decided 2026-10-10). Outside an HTTP request (Kafka), recording the outcome does nothing |

### Demo measurement (MVP)
A single comparison for the video, with no budget and no report format (own proposal: shows the cache's effect without the deferred perf method):
1. Create one row of the example resource, and get a token with `cargo xtask token`.
2. With the cache enabled, run `oha -z 30s -q <rate> --latency-correction -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/<resources>/<id>` against that one id (a hot key).
3. Restart without `cache` in `ENABLE_FEATURES`, and run the same command at the same `<rate>`.
4. Show both outputs side by side in the video.

`<rate>` is picked at recording time as a rate the machine sustains with the cache off, and it is stated in the video (measured). The 30 s duration is an own proposal: long enough for stable percentiles, short enough for the video. `-q` with `--latency-correction` avoids coordinated omission ([oha README](https://github.com/hatoo/oha)).

## Known trade-offs (covered in the video)

1. **The cache-aside race.** Reader A misses and reads the old value from the DB. Writer B commits and deletes the key. Reader A then writes the old value back. The value stays stale until the TTL expires. The mitigation is a short TTL. Delayed double delete is deferred.
2. **Cache stampede** on a hot key at expiry. TTL jitter and XFetch are both deferred.
3. **No negative caching** (caching 404s). Adding it would also require a DEL on create.
4. **The decorator's limit.** It works as long as each write is a single repository call that commits on its own. If a Unit of Work is introduced (for example, for the outbox), invalidation must move to after the commit.

## Build plan
1. `cache_aside` helper with unit tests (hit, miss, error) (AC-1, AC-4, AC-5)
2. Redis connection (`ConnectionManager`), config, readiness check (AC-4)
3. Decorator for the walking-skeleton resource, and the toggle in wiring (AC-1, AC-2, AC-3)
4. `X-Cache` task-local + middleware (AC-1)
5. Integration tests with testcontainers, including stopping the Redis container mid-test (AC-1 to AC-4)
6. The demo measurement above, recorded for the video

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## Resolved questions (closed 2026-10-10 with the recommendation)

| # | Question | Decision |
|---|----------|----------|
| C1 | Serialize values as JSON, or as MessagePack/bincode? | JSON. It is debuggable with `redis-cli`, which helps in the video, and CRUD payloads are small, so the perf difference is negligible |
| C2 | Default TTL | 300 s, own proposal ([api.md](api.md), MVP limits). Jitter is deferred |
| C3 | Toggle at startup only, or at runtime too? | Startup only, because it is simple and safe. The demo restarts the container |
| C4 | What happens to a cached value that fails to decode? | Decided 2026-10-10 (engineer): it counts as a MISS. Log a warning, load from the DB, and `SET` the fresh value over it. The client still gets 200 |
| C5 | Log levels for cache events | Decided 2026-10-10 (engineer): HIT, MISS, and ERROR at `debug`, Redis failures as warnings. AC-1 is shown with `LOG_LEVEL=info,infrastructure=debug` |
| C6 | `X-Cache` values, and is the header safe in production? | Decided 2026-10-10 (engineer): `HIT`, `MISS`, `BYPASS`, and `ERROR`. Startup fails with `CACHE_DEBUG_HEADER=true` and `APP_ENV=production`, with an error that names both env vars ([security.md](security.md) SEC-6) |
| C7 | Redis timeouts in the MVP | Decided 2026-10-10 (engineer): the redis-rs defaults (Decision table, Redis timeouts); reconnect retries changed to 0 in C9. A measured cache call timeout (api.md N3) stays deferred |
| C8 | Per-resource TTL | Decided 2026-10-10 (engineer): the decorator takes `ttl: Duration`; wiring passes `CACHE_TTL_SECS`, or a constant that links to the resource spec (AC-5). No per-resource env vars |
| C9 | With the redis-rs defaults, every cache call during a Redis outage waited about 9.5 s for the background reconnect (6 retries) before it failed open, and so did `/readyz`. Keep that? | Decided 2026-10-10 (engineer): `set_number_of_retries(0)` in `infrastructure::redis::connect_lazy`, other values at the library defaults. Measured 2026-10-10 after stopping the Redis container (`stop_with_timeout(0)`), three runs each, on a WSL2 dev machine (Linux 6.18, Docker via testcontainers, Redis 8.10.2): in `cache_ac_4_redis_down_fails_open_with_error`, the 1st cache call 0.14 ms, the 2nd 1.00 s (the connection timeout), the 3rd 0.4 to 0.46 ms; in `stack_ac_6_healthz_and_readyz_with_cache_enabled`, `/readyz` 1.4 to 1.6 ms, 34 to 37 ms, and 1.1 to 1.2 ms. Both tests assert each call stays under 5 s: the design bound is 1.5 s (one 1 s connection attempt plus one 500 ms command), so 5 s leaves more than 3x headroom for CI and still catches the 9.5 s retry cycle |

## Deferred

Designed and decided, then cut on 2026-10-10 so the MVP ships by 14 Oct. The original text is kept.

### Removing Redis from the whole project
**Deferred** on 2026-10-10 (engineer decision): a clean boilerplate should not make a project carry Redis it never chose. The MVP keeps the cache code in the repository, switched off by default, because the brief asks for a CRUD API whose Redis cache can be toggled on or off. Two ways to go further, in order of cost:
1. **Cargo features** `cache` (off by default) across `infrastructure`, `http-api`, and `bootstrap`. Without the feature, the cache code and the `redis` crate are not compiled. CI must then build and test both with and without the feature.
2. **A generator** (a `cargo-generate` template or a setup skill) that asks at project creation and deletes the cache code, its tests, and its compose service. Every combination of choices needs its own test run.

**MVP uses:** the code is present but inert. The cache is off unless `ENABLE_FEATURES` contains `cache` ([runtime.md](runtime.md)). While it is off, the app needs no `REDIS_URL`, `/readyz` skips Redis, and `make up` does not start the `redis` container (compose profile `cache`). Per resource, `/flow-spec` asks for the cache policy, and a resource with `**Cache**: disabled` gets no decorator ([layers.md](../../.agents/skills/flow-build/layers.md), sections 4 and 6).


### Cache metrics and cache call timeout
**Deferred:** metrics need a `/metrics` endpoint and dashboards that the demo does not use, and the timeout needs a measured Redis p99.9. **MVP uses:** `debug` log lines for HIT, MISS, and ERROR, and warnings for Redis failures; BYPASS shows in `X-Cache` and in the startup `feature` log line. The cache calls use the redis-rs default timeouts (Decision table, Redis timeouts).

Original text:
- **AC-1**: With `CACHE_ENABLED=true`, the first GET is a MISS and the next one is a HIT (visible in `X-Cache` and in metrics).
- **AC-4**: If Redis is down or slower than `CACHE_TIMEOUT_MS` (value: [docs/conventions/api.md](api.md) N3), the API still returns 200 (fail-open). It logs a warning and increments `cache_errors_total`.
| Metrics | `cache_requests_total{resource,result=hit|miss|bypass|error}`, `cache_latency_seconds` |
| Write path | DB commit, then `DEL`. If the DEL fails, log it and increment a metric. The TTL is the safety net |

### TTL jitter
**Deferred:** jitter only helps when many keys expire together, and lazy loading already spreads expiry. **MVP uses:** a fixed TTL.

Original text:
- **AC-5**: TTL is `CACHE_TTL_SECS` plus jitter (values: [docs/conventions/api.md](api.md) N1, N2). A resource spec sets its own TTL.
| C2 | Default TTL | 300 s with one-sided 0 to 10% jitter ([docs/conventions/api.md](api.md) N1, N2) |

### Get by id over Kafka (AC-6)
**Deferred:** the MVP consumer handles create, update, and delete only ([kafka.md](kafka.md)). **MVP uses:** GET over HTTP.

- **AC-6**: Get by id over Kafka ([docs/conventions/kafka.md](kafka.md)) uses the same cache path.

### Perf gate (AC-7)
**Deferred:** depends on the deferred perf method ([api.md](api.md), Deferred). **MVP uses:** the demo measurement above.

- **AC-7 (perf)**: At the same arrival rate, the hot-key GET `T99` with the cache on is below the cache-off GET `T99` (the cache gate in [docs/conventions/api.md](api.md)). In the hot-key scenario, every request after the first is a HIT until the TTL expires. Any other MISS is a bug, not a budget question.

Original build plan step: `6. Perf scenarios P1 and P2, run as part of /flow-verify products perf (AC-7)`.

Original trade-offs text (before the cut):

1. **The cache-aside race.** Reader A misses and reads the old value from the DB. Writer B commits and deletes the key. Reader A then writes the old value back. The value stays stale until the TTL expires. The mitigation is a short TTL. Delayed double delete is deferred.
2. **Cache stampede** on a hot key at expiry. Jitter does not fix a single hot key ([docs/conventions/api.md](api.md) N2). XFetch or single-flight is deferred.
3. **No negative caching** (caching 404s). Adding it would also require a DEL on create.
4. **The decorator's limit.** It works as long as each write is a single repository call that commits on its own. If a Unit of Work is introduced (for example, for the outbox), invalidation must move to after the commit.

