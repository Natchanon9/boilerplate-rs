> Build history of the boilerplate (assignment). Not read by flow skills. The service scope is docs/scope/scope.md.

# Scope: rs-boilerplate

A Rust CRUD service boilerplate built on clean architecture. It has a Redis cache-aside layer that can be switched on or off, and it accepts commands over Kafka as an alternative to HTTP. It ships with workflow skills that let AI tools (Claude Code, Codex) add any new resource while following the boilerplate strictly.
It is a take-home assignment deliverable, and it is also meant to be a real project foundation.

**Build approach:** Tracer Bullet. Get one thin slice through every layer working first (HTTP > use case > Postgres > Redis > Kafka), then thicken it.
**History note (2026-10-11):** this file is the build history. Its older formats are kept as they were and no longer apply: the `Tier` column and the `Workflow tier` / `production` wording (tiers were removed at the MVP cut on 2026-10-10), the `Perf it` and `Sync it` boxes (their skills are in [deferred-skills/](deferred-skills/README.md)), and the Timeline's old resource names and the `example/products` branch. The shipped flow is spec, build, verify, review.

_Features 1 to 4, 6, and 7 are build-time work, done with [docs/conventions/](../conventions/README.md) and chat. Flow skills are not used for them (they are narrowed to resource work), so their boxes are plain tasks. Feature 5 is resource work, tracked in the service scope._

_Every step is a recommendation. Skip what you already know how to do. Any load-bearing decision is always written down in a spec. You decide when a feature is `done`._

## Assignment mapping

| Deliverable | Covered by feature |
|-------------|--------------------|
| D1: CRUD API with a Redis cache that can be toggled on or off | 5, 6 |
| D2: An alternative transport to HTTP (our own proposal) | 7 (building blocks only: Kafka is out of the MVP since 2026-10-12) |
| D3: Skills that make AI tools follow the boilerplate strictly, across tools | 1, 5 |

## At a glance

| # | Feature | Phase | Tier | Status |
|---|---------|-------|------|--------|
| 1 | Workflow skills | Foundation | production | done (Claude Code; Codex is out of the MVP) |
| 2 | Stack & architecture | Foundation | production | done |
| 3 | API conventions | Foundation | standard | done |
| 4 | Security baseline | Foundation | production | done |
| 5 | Walking skeleton: tracks | Slice 1 | production | done |
| 6 | Cache-aside toggle | Slice 2 | production | done |
| 7 | Kafka transport | Slice 3 | production | dropped from the MVP (2026-10-12): the building blocks stay in the code, off by default, tested with a dummy handler (AC-3, AC-5); AC-1, AC-2, and AC-7 are not verified |

## Foundations

### 1. Workflow skills
Workflow skills that work for any CRUD resource: six at first (scope, spec, build, verify, review, sync), four after the MVP cut (spec, build, verify, review). We use them to build this project ourselves (dogfooding).
**Done when:** the four skills run in both Claude Code and Codex, the skills pass validation against the Agent Skills specification, and feature 5 was actually built through this workflow.
- [x] Spec it: [ai-workflow-skills.md](ai-workflow-skills.md)
- [x] Build it (conventions + chat)
   - [x] Application (AC-3, AC-4): skill drafts v0.1 (done in docs), project context files
   - [x] Wiring + integration tests (AC-1, AC-2, AC-5, AC-6): skill validation in CI, run in Claude Code and Codex. Claude Code: tracks went through `/flow-spec`, `/flow-build`, `/flow-verify`, and `/flow-review` on 2026-10-12 (`example/track`, one commit per step). `cargo xtask check-skills` is built and runs in `make verify` (AC-6). Negative tests: 7 pass, 1 partial ([reports/negative-tests.md](reports/negative-tests.md), AC-4). Open: Codex (W1, AC-1, AC-2). AC-5 is deferred
- [x] Verify it (check the convention's ACs on the running stack): tracks built through the four skills in Claude Code (AC-1, AC-2, AC-7), `flow-build` stopped on a missing spec (AC-3, negative test 2), negative tests 7 pass and 1 partial (AC-4), `check-skills` in `make verify` (AC-6). Codex is out of the MVP (decision of 2026-10-12)
- [x] Perf it: (skipped: no request path of its own; perf is measured from feature 5)
- [x] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`): fresh Claude Code sessions ran the negative tests ([reports/negative-tests.md](reports/negative-tests.md)), and `flow-review` reviewed tracks. The prompt 5 follow-up (ask for a TTL) is left open
- [x] Sync it (update this file and the decisions log): 2026-10-12
[docs/assignment/ai-workflow-skills.md](ai-workflow-skills.md)

### 2. Stack & architecture
Decide the stack and scaffold a clean architecture workspace that runs, so every later slice builds on real structure.
**Done when:** the whole local stack starts with one command, the verification gate passes, and a layer violation fails to compile.
- [x] Spec it: [docs/conventions/stack-architecture.md](../conventions/stack-architecture.md)
- [x] Build it (conventions + chat)
   - [x] Infrastructure + adapters (AC-3, AC-6): local stack, health endpoints
   - [x] Wiring + integration tests (AC-1, AC-2, AC-4, AC-5, AC-7): workspace, dependency bans, config, CI
- [x] Verify it (check the convention's ACs on the running stack): shown in part with tracks on 2026-10-12: `make up` with `cache`, `/readyz` 200, and `make verify` green (AC-1 to AC-4, AC-6, AC-8); AC-5 and AC-7 are covered by `crates/bootstrap/tests/cli.rs`. AC-3 with `ENABLE_FEATURES=kafka` is not in the MVP (decision of 2026-10-12)
- [x] Perf it: (skipped: no request path of its own; perf is measured from feature 5)
- [x] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`): [reports/feature-2-stack-review.md](reports/feature-2-stack-review.md), the major finding fixed
- [x] Sync it (update this file and the decisions log): 2026-10-12
[docs/conventions/stack-architecture.md](../conventions/stack-architecture.md)

### 3. API conventions (standard)
Rules shared by every resource: URLs, pagination, error format, and input validation. Concurrency, idempotency, the perf method, and the N1 to N8 limits moved to Deferred at the MVP cut. Deciding them once is what lets the workflow build any resource without re-deciding the basics.
**Done when:** the shared building blocks (errors, pagination, validation) are tested, and [docs/conventions/api.md](../conventions/api.md) is Accepted.
- [x] Spec it: [docs/conventions/api.md](../conventions/api.md)
- [x] Build it (conventions + chat)
   - [x] Infrastructure + adapters (AC-1 to AC-10): shared HTTP building blocks (AC-9 and AC-10 are deferred)
   - [x] Wiring + integration tests (AC-9, AC-10): request-id, API docs (deferred at the MVP cut)
- [x] Verify it (check the convention's ACs on the running stack): shown with tracks on 2026-10-12 ([verify report](https://github.com/Natchanon9/boilerplate-rs/blob/example/track/docs/reports/0001-tracks-verify.md)): AC-1 to AC-8 on the running stack, plus http-api unit tests for 413 and 415
- [x] Sync it (update this file and the decisions log): 2026-10-12; review: [reports/feature-3-api-review.md](reports/feature-3-api-review.md), findings accepted
[docs/conventions/api.md](../conventions/api.md)

### 4. Security baseline
A cross-cutting security standard, mapped to the OWASP API Security Top 10 and enforced by tooling wherever possible.
**Done when:** HS256 token validation, scopes, body limits, dependency auditing, and the no-panic lints are all enforced in CI.
- [x] Spec it: [docs/conventions/security.md](../conventions/security.md)
- [x] Build it (conventions + chat)
   - [x] Infrastructure + adapters (SEC-1, SEC-2, SEC-5, SEC-6, SEC-9): auth, limits, headers, secrets. Built: JWT auth, `RequireScope`, `xtask token` (SEC-1, SEC-2), body limit and page cap (SEC-5), no internal detail in errors, `X-Cache` only with `CACHE_DEBUG_HEADER=true`, and no start with it under `APP_ENV=production` (SEC-6), secret types, redaction, and no header logging (SEC-9). Security headers are deferred
   - [x] Wiring + integration tests (SEC-8, SEC-10, SEC-11, SEC-12): lints, audits, container. `make verify` runs `make sql-check` (SEC-8), `cargo deny` (SEC-10), and the clippy panic lints (SEC-11); SEC-12 is deferred
- [x] Verify it (check the convention's ACs on the running stack): shown with tracks on 2026-10-12: SEC-1 and SEC-2 (401 and 403 on every route), SEC-4 (`unknown_field`), SEC-6 (fixed error details; review finding 3 fixed), and SEC-8 (`make sql-check`). SEC-9: `app info env` prints `AUTH_JWT_SECRET=REDACTED` and `postgres://app:REDACTED@...`. SEC-13: after requests with valid and invalid tokens, no log line holds `Bearer`, a JWT, or `authorization`. SEC-5: `limit=500` is 422, and 413 is unit-tested. SEC-10 and SEC-11: `make verify`. SEC-7 is not in the MVP (Kafka, decision of 2026-10-12). SEC-3 does not apply (tracks has no owner)
- [x] Perf it: (deferred at the MVP cut; no load measurement in the demo, decision of 2026-10-12)
- [x] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`): [reports/feature-4-security-review.md](reports/feature-4-security-review.md), the major finding fixed and the SQL check widened
- [x] Sync it (update this file and the decisions log): 2026-10-12
[docs/conventions/security.md](../conventions/security.md)

## Slice 1: Walking skeleton

### 5. Walking skeleton: tracks
The first resource, a simple CRUD of music tracks (title, artist, duration) wired end to end through every layer, with the cache. It has no Kafka commands: Kafka is not part of the demo (decision of 2026-10-12). It proves the stack is connected. The engineer designed its fields with `/flow-spec tracks`, and the demo runs `/flow-spec tracks` and then `/flow-build tracks` from the bare boilerplate ([docs/prompts/demo-tracks.md](../prompts/demo-tracks.md)). The finished result is on the `example/track` branch, one commit per flow step. It replaces the earlier `products` and `music` examples.
**Done when:** all five CRUD operations work over HTTP against the real database, every AC has an integration test, and the feature was built through the flow skills.
- [x] Tracked as feature 1 in the service scope: [docs/scope/scope.md](../scope/scope.md), with its spec in [docs/specs/](../specs/) (on `example/track`: spec 0001 `Accepted`, 11 of 11 ACs pass in the verify report, review findings 3 and 4 fixed)

## Slice 2: Cache

### 6. Cache-aside toggle
Reads check the cache first, fall back to the DB on a miss, and populate the cache. Writes delete the key after the DB commit. A config flag turns the whole thing on or off. It lives in a decorator, so use cases never know the cache exists.
**Done when:** every AC in [docs/conventions/cache.md](../conventions/cache.md) passes, including the cache-down case, shown in the demo through `X-Cache`.
- [x] Spec it: [docs/conventions/cache.md](../conventions/cache.md)
- [x] Build it (conventions + chat)
   - [x] Infrastructure + adapters (AC-1 to AC-6): cache helper, decorator, debug header, metrics. Built: `cache_aside` (`RedisCache`, `get_or_load`, `invalidate`), the `cache_status` task-local, the `X-Cache` middleware, and the production guard. The per-resource decorator came with `/flow-build tracks` (`CachedTrackRepository` on `example/track`). Metrics and AC-6 are deferred
   - [x] Wiring + integration tests (AC-1 to AC-4, AC-7): toggle, container tests. Built: shared `RedisCache` and `X-Cache` wiring, and container tests of the helper and the header (`crates/bootstrap/tests/cache.rs`). AC-1 to AC-4 pass end to end with tracks on `example/track` (`track_cache.rs`, and the verify report including Redis stopped). The demo measurement is dropped (decision of 2026-10-12). AC-7 is deferred
- [x] Verify it (check the convention's ACs on the running stack): shown with tracks on 2026-10-12: AC-1 to AC-4, including Redis stopped (verify report on `example/track`), and AC-5 (key TTL 300 from `CACHE_TTL_SECS`; `ac_8_first_get_is_a_miss_and_the_second_a_hit` checks a non-default TTL)
- [x] Perf it: (deferred at the MVP cut; no load measurement in the demo, decision of 2026-10-12)
- [x] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`): [reports/feature-6-cache-review.md](reports/feature-6-cache-review.md), findings accepted
- [x] Sync it (update this file and the decisions log): 2026-10-12
[docs/conventions/cache.md](../conventions/cache.md)

## Slice 3: Alternative transport

### 7. Kafka transport
Accepts commands from topics and calls the same use cases as HTTP. Publishes events, and optionally replies. Processing is idempotent, and bad messages go to a DLQ.
**Done when:** every AC in [docs/conventions/kafka.md](../conventions/kafka.md) passes, and the flow is visible in the message-broker UI.
- [x] Spec it: [docs/conventions/kafka.md](../conventions/kafka.md)
- [x] Build it (conventions + chat) (not in the MVP: nothing runs with `ENABLE_FEATURES=kafka`, decision of 2026-10-12)
   - [x] Application (AC-1): event publisher port (deferred at the MVP cut, with events, [kafka.md](../conventions/kafka.md) Deferred)
   - [x] Infrastructure + adapters (AC-1 to AC-6): envelope, consumer, DLQ, idempotent repository writes. The generic parts are built in `crates/kafka-api` (strict envelope, `CommandHandler`, dispatcher, single-loop consumer, DLQ with `error.reason`; AC-3 and AC-5). No resource handler: Kafka is not part of the demo (decision of 2026-10-12), and tracks declares no commands. Idempotent repository writes (`ON CONFLICT (id) DO NOTHING`, `AlreadyExists`) are built in the tracks repository, but no Kafka command exercises them (not in the MVP: nothing runs with `ENABLE_FEATURES=kafka`, decision of 2026-10-12)
   - [x] Wiring + integration tests (AC-1 to AC-7): container tests, broker UI, demo producer. Built: `wiring::kafka_handlers`, container tests with a dummy handler (AC-1 receive, AC-3, AC-5), `cargo xtask kafka-produce`, and kafka-ui in compose (AC-7, not yet checked in the UI). AC-1 and AC-2 end to end (a DB row, no duplicate on re-send) stay unproven until a resource declares commands (not in the MVP: nothing runs with `ENABLE_FEATURES=kafka`, decision of 2026-10-12)
- [x] Verify it (check the convention's ACs on the running stack) (not in the MVP: nothing runs with `ENABLE_FEATURES=kafka`, decision of 2026-10-12)
- [x] Perf it: (deferred at the MVP cut; no load measurement in the demo, decision of 2026-10-12)
- [x] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`) (not in the MVP: nothing runs with `ENABLE_FEATURES=kafka`, decision of 2026-10-12)
- [x] Sync it (update this file and the decisions log) (not in the MVP: nothing runs with `ENABLE_FEATURES=kafka`, decision of 2026-10-12)
[docs/conventions/kafka.md](../conventions/kafka.md)

## Timeline (deadline: Wed 14 Oct 2026)

| Day | Work |
|-----|------|
| Fri 9 | Finish grilling. Conventions and the skills spec become Accepted |
| Sat 10 | Scaffold (feature 2), then the building blocks of features 3 and 4 |
| Sun 11 | The building blocks of features 6 and 7 |
| Mon 12 | `/flow-spec tracks` (the grill), `/flow-build tracks`, `/flow-verify tracks`, and `/flow-review tracks` (feature 5) on `example/track` |
| Tue 13 | Codex check (open question W1): out of the MVP since 2026-10-12 |
| Wed 14 | Buffer, review, submit |

Timeline before the music decision (kept for history):
| (old) Sun 11 | The building blocks of features 6 and 7, then `/flow-build products` (feature 5) |
| (old) Mon 12 | Skill tooling (feature 1) |
| (old) Wed 14 | Buffer, review, submit |

Timeline before the build-order decision (kept for history):
| (old) Sat 10 | Scaffold (feature 2) and CRUD for products (features 3, 5) |
| Sun 11 | Cache (feature 6) and the four skills (feature 1) |
| Mon 12 | Kafka (feature 7) |

Timeline before the MVP cut (kept for history):
| (old) Fri 9 | Finish grilling. Conventions and the skills spec become Accepted |
| Sat 10 | Features 1 (skill tooling), 2, 3 |
| Sun 11 | Features 4, 5 |
| Mon 12 | Features 6, 7 |
| Tue 13 | Perf report |
| Wed 14 | Buffer, review, submit |

## Deferred
Out of scope for this pass, kept here so the plan stays honest.
- **Transactional outbox and inbox**: fixes the event dual-write problem and supports relative (delta) commands (needs a decision)
- **Hot-key stampede protection** with XFetch (probabilistic early recomputation, VLDB 2015) or single-flight
- **Negative caching** and **delayed double delete**
- **Schema registry (Avro/Protobuf)** for Kafka
- **Mock OIDC provider in compose** for a full token-endpoint flow
- **A second resource that mirrors the first (dogfood)**: dropped on 2026-10-10
- **gRPC transport**, **cloud deploy**, **full RBAC**, **multi-tenancy**, **frontend**

## Decisions log (project level)

| Date | Decision |
|------|----------|
| 2026-10-09 | Stack: Rust (axum, sqlx), PostgreSQL, Redis, Kafka |
| 2026-10-09 | AI tools: Claude Code and Codex |
| 2026-10-09 | Walking-skeleton resource: `products` (replaced on 2026-10-10, see the music row below) |
| 2026-10-09 | Team-facing docs are in English. Personal Thai translations are generated on demand into `.private/th/` (gitignored, non-authoritative) |
| 2026-10-09 | Deadline: Wed 14 Oct 2026 (P1) |
| 2026-10-09 | Auth: the service is a resource server that validates JWTs. Key source is set by config. Dev tokens come from `xtask`. No IdP in this repo ([docs/conventions/security.md](../conventions/security.md) S1) |
| 2026-10-10 | Runtime grooming (credited in [docs/conventions/README.md](../conventions/README.md)): `DB_PLATFORM` with a Postgres adapter, a recipe, and repository contract tests ([database.md](../conventions/database.md)); a CLI with `db migrate|rollback|seed` and `info env|features`; flag > env > `.env`; `DISABLE_FEATURES` replacing `CACHE_ENABLED` (renamed to the opt-in `ENABLE_FEATURES` later the same day, see the row "Redis and Kafka opt-in" below); public, protected, and hidden routers ([runtime.md](../conventions/runtime.md)). More adapters (SQLite, MySQL) are deferred |
| 2026-10-10 | Feature 2 grill: compose runs one `app` container with `all`; CI runs a Postgres service so `make verify` (with `cargo sqlx prepare --check`) is the same gate locally and in CI; local builds use sqlx online mode, and only the Docker build is offline; settings are `REDIS_URL`, `KAFKA_BROKERS`, `APP_NAME`, and `APP_ENV` ([runtime.md](../conventions/runtime.md)). Until feature 7, `consume` is a placeholder that checks config and the `kafka` toggle, then waits for shutdown |
| 2026-10-10 | After feature 2: `AppError::VersionMismatch` and database TLS move to Deferred ([stack-architecture.md](../conventions/stack-architecture.md), [database.md](../conventions/database.md)). Build order: every shared building block from features 3, 4, 6, and 7 first, then `/flow-build products` once, so feature 5 goes through the workflow without boilerplate gaps |
| 2026-10-10 | `main` is the bare boilerplate, and nothing on it refers to an example: a user builds their first resource from zero with `/flow-spec` and `/flow-build`, using `layers.md` (with short snippets for the hard parts). The demo does the same on the `example/products` branch, which holds spec 0001, its scope row, and its Kafka topics. Within a project, `flow-build` mirrors that project's first resource once it exists |
| 2026-10-10 | Cut to MVP: CRUD without filters, sorting, or ETag; cache-aside with the toggle and fail-open; Kafka one-way commands with a DLQ; HS256 auth; four skills (spec, build, verify, review). Everything else moved to `## Deferred` sections, not deleted |
| 2026-10-10 | Operational limits N1 to N8 decided from references ([docs/conventions/api.md](../conventions/api.md)), except the rate-limit value (P4) |
| 2026-10-10 | Feature 4 auth details ([docs/conventions/security.md](../conventions/security.md)): `WWW-Authenticate` follows [RFC 6750 3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1) (no error code without credentials, `invalid_token` for any bad token, `insufficient_scope` with the `scope` parameter on `403`); unknown paths on the protected router return `404` before auth (`route_layer`); `cargo xtask token` requires `--sub` and `--scope`, and `--ttl` defaults to `1h` (own proposal) |
| 2026-10-10 | Feature 6 cache details ([docs/conventions/cache.md](../conventions/cache.md) C4 to C8): `X-Cache` is `HIT`, `MISS`, `BYPASS`, or `ERROR` (Redis failed, read from the DB); an undecodable cached value is a MISS that is overwritten; HIT, MISS, and ERROR log at `debug` and Redis failures warn, shown with `LOG_LEVEL=info,infrastructure=debug`; startup fails with `CACHE_DEBUG_HEADER=true` and `APP_ENV=production` (SEC-6); Redis calls use the redis-rs default timeouts (500 ms response, 1 s connect, [api.md](../conventions/api.md) MVP limits); the decorator takes its TTL at construction, `CACHE_TTL_SECS` by default or a constant linked to the resource spec, with no per-resource env vars |
| 2026-10-10 | Feature 7 build decisions ([docs/conventions/kafka.md](../conventions/kafka.md)), MVP only: DLQ headers `error.reason` (one of eight codes) and `error.detail` (field errors for `validation`, a fixed sentence otherwise, never an internal cause); the offset is committed after success or after the DLQ write, a failed DLQ write stops the consumer and the process, and compose restarts `app` with `unless-stopped`; `auto.offset.reset=earliest` (the librdkafka default is `largest`, the same as `latest`); one receive loop, so cross-partition parallelism moves to Deferred; a strict envelope with six required fields and a key equal to `payload.id`; payloads are the HTTP bodies plus `id`, parsed by the shared `Field<T>`, `UnknownFields`, and application parser; `cargo xtask kafka-produce` sends a generated envelope or a raw value. With no handler registered, `consume` exits non-zero and `all` runs HTTP only ([runtime.md](../conventions/runtime.md)) |
| 2026-10-10 | Redis and Kafka opt-in (engineer decision): `ENABLE_FEATURES` (empty by default, accepts `cache` and `kafka`) replaces `DISABLE_FEATURES`, so a fresh clone starts only what it uses. The compose services sit in profiles with the same names (`redis` in `cache`; `kafka`, `kafka-init`, and `kafka-ui` in `kafka`), `make up` sets `COMPOSE_PROFILES` from the switch, and `make down` stops every profile ([runtime.md](../conventions/runtime.md), Compose topology; [Compose profiles](https://docs.docker.com/compose/how-tos/profiles/)). Removing the code with Cargo features stays deferred ([cache.md](../conventions/cache.md), [kafka.md](../conventions/kafka.md)) |
| 2026-10-10 | Independent review of the `products` example fixed two gaps in the boilerplate snippets (they were copied faithfully into the resource): `application::input::uuid_v7` now parses every producer-supplied id, since a non-v7 id sorts above every v7 id and breaks the newest-first list; and `wiring::Infra` builds the Postgres pool and the Redis connection once per process for `/readyz`, HTTP, and Kafka, instead of one pool per resource builder. `layers.md` sections 3 and 6 carry the new snippets (engineer-approved plan) |
| 2026-10-10 | Example resource is `music` (engineer decisions): (1) the example changes from `products` to a simple `music` CRUD. The engineer designs its fields later in their own session with `/flow-spec music`, and it uses the cache and the Kafka commands (create, update, delete) so it can demo both. (2) `categories` is dropped completely, so there is no second dogfood resource. The demo runs `/flow-spec music` and then `/flow-build music` from the bare boilerplate, and a second resource that mirrors the first moves to Deferred. (3) The branch is `example/music`, created from `main`. `example/products` stays local only (never pushed) and is deleted after music passes review (superseded on 2026-10-12, see the tracks row below) |
| 2026-10-12 | Example resource is `tracks`, and neither Kafka nor an `oha` comparison is part of the demo (engineer decisions): (1) the example built through the flow is `tracks` (title, artist, duration; full CRUD; cache on), spec 0001 on the `example/track` branch, one commit per flow step. It replaces `music`. (2) tracks declares no Kafka commands, and no more work is added for the demo, so kafka.md AC-1 and AC-2 stay proven only up to the dummy-handler tests (receive, DLQ, ordering); the Kafka code stays in the boilerplate, off by default. (3) The demo `.env` needs `ENABLE_FEATURES=cache` only. (4) There is no `oha` cache on/off comparison: the demo shows the cache through `X-Cache` (MISS, HIT, invalidation, and `ERROR` with Redis stopped) |
| 2026-10-12 | Kafka is out of the MVP (engineer decision): nothing is run or verified with `ENABLE_FEATURES=kafka` (stack AC-3 for the `kafka` profile, SEC-7, kafka.md AC-1, AC-2, and AC-7). The Kafka code stays in the boilerplate, off by default, with its dummy-handler tests (AC-3, AC-5). Feature 7 is `dropped` for this pass |
| 2026-10-12 | Closing the MVP (engineer decisions): Codex is out of the MVP, so the skills are verified in Claude Code only ([ai-workflow-skills.md](ai-workflow-skills.md) Deferred, W1). `cargo xtask check-skills` is built and runs in `make verify`. The negative tests ran in fresh sessions: 7 pass, 1 partial. Convention reviews of features 2, 3, 4, and 6: the `.env` parse-error leak (SEC-9) is fixed, the SQL check also catches lowercase statements (SEC-8), and the other findings are accepted as known trade-offs. Features 1 to 6 are `done`, and feature 7 is `dropped` |
| 2026-10-09 | Perf method ([docs/conventions/api.md](../conventions/api.md) A2, A4, A5): the SLI is the percentage of requests under a threshold, in two grades. The ceiling is 99% < 100 ms. Targets come from a rounded baseline. The load is an open model with keep-alive on. The operating rate is the measured knee. Every number comes from a reference or a measurement |

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| P4 | Per-client rate limit (requests per second per token `sub`) for the API | Moved to [docs/conventions/api.md](../conventions/api.md) (open question P4) |

## Resolved questions (closed 2026-10-10 with the recommendation)

| # | Question | Decision |
|---|----------|----------|
| P3 | Submit as a public GitHub repo, or a private repo with invites? | Public repo |

## Legend

- **Feature format, box owners, and milestone mapping:** see [`deferred-skills/flow-scope/SKILL.md`](deferred-skills/flow-scope/SKILL.md) ("Feature shape"; since the MVP cut, the shipped format is in `.agents/skills/flow-spec/SKILL.md`). Build-time features in this file use plain task boxes.
- **Feature status** (in the `At a glance` table only): `planned` > `in-progress` > `done`. Also `dropped` (kept for history, never deleted).
- **Spec status:** `Proposed` (draft, being grilled) > `Accepted` (decided, or shipped). Also `Superseded`, and `Assumed` (built before deciding; must be ratified later).
- **Tier**, meaning what runs after build:
  - `spike`: nothing
  - `standard`: `/flow-verify`
  - `production`: `/flow-verify`, `/flow-verify <feature> perf`, `/flow-review`
  - Every tier ends with `/flow-sync`.
- **needs a decision:** run `/flow-spec` before `/flow-build`.
- **Next step** for a feature is its first unticked box.
- Atomic build tasks live in each spec's `## Build plan`, never here.
