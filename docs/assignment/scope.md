> Build history of the boilerplate (assignment). Not read by flow skills. The service scope is docs/scope/scope.md.

# Scope: rs-boilerplate

A Rust CRUD service boilerplate built on clean architecture. It has a Redis cache-aside layer that can be switched on or off, and it accepts commands over Kafka as an alternative to HTTP. It ships with workflow skills that let AI tools (Claude Code, Codex) add any new resource while following the boilerplate strictly.
It is a take-home assignment deliverable, and it is also meant to be a real project foundation.

**Build approach:** Tracer Bullet. Get one thin slice through every layer working first (HTTP > use case > Postgres > Redis > Kafka), then thicken it.
**Workflow tier (default):** `standard` (build, then verify). Core features are tagged `production`, which adds perf verification and review. (Tiers were removed at the MVP cut on 2026-10-10; see the Decisions log.)

_Features 1 to 4, 6, and 7 are build-time work, done with [docs/conventions/](../conventions/README.md) and chat. Flow skills are not used for them (they are narrowed to resource work), so their boxes are plain tasks. Feature 5 is resource work, tracked in the service scope (feature 8 was dropped and merged into it)._

_Every step is a recommendation. Skip what you already know how to do. Any load-bearing decision is always written down in a spec. You decide when a feature is `done`._

## Assignment mapping

| Deliverable | Covered by feature |
|-------------|--------------------|
| D1: CRUD API with a Redis cache that can be toggled on or off | 5, 6 |
| D2: An alternative transport to HTTP (our own proposal) | 7 |
| D3: Skills that make AI tools follow the boilerplate strictly, across tools | 1, 5 |
| D4: A demo video | 9 |
| D5: Email the submission with links by "next Wednesday" | 9 |

## At a glance

| # | Feature | Phase | Tier | Status |
|---|---------|-------|------|--------|
| 1 | Workflow skills | Foundation | production | in-progress |
| 2 | Stack & architecture | Foundation | production | in-progress |
| 3 | API conventions | Foundation | standard | in-progress |
| 4 | Security baseline | Foundation | production | in-progress |
| 5 | Walking skeleton: music | Slice 1 | production | planned |
| 6 | Cache-aside toggle | Slice 2 | production | in-progress |
| 7 | Kafka transport | Slice 3 | production | in-progress |
| 8 | Dogfood: AI builds a second resource | Slice 4 | standard | dropped |
| 9 | Demo video & submission | Delivery | | planned |

## Foundations

### 1. Workflow skills
Workflow skills that work for any CRUD resource: six at first (scope, spec, build, verify, review, sync), four after the MVP cut (spec, build, verify, review). We use them to build this project ourselves (dogfooding).
**Done when:** the four skills run in both Claude Code and Codex, the skills pass validation against the Agent Skills specification, and feature 5 was actually built through this workflow.
- [x] Spec it: [ai-workflow-skills.md](ai-workflow-skills.md)
- [ ] Build it (conventions + chat)
   - [x] Application (AC-3, AC-4): skill drafts v0.1 (done in docs), project context files
   - [ ] Wiring + integration tests (AC-1, AC-2, AC-5, AC-6): skill validation in CI, run in Claude Code and Codex
- [ ] Verify it (check the convention's ACs on the running stack)
- [x] Perf it: (skipped: no request path of its own; perf is measured from feature 5)
- [ ] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`)
- [ ] Sync it (update this file and the decisions log)
[docs/assignment/ai-workflow-skills.md](ai-workflow-skills.md)

### 2. Stack & architecture
Decide the stack and scaffold a clean architecture workspace that runs, so every later slice builds on real structure.
**Done when:** the whole local stack starts with one command, the verification gate passes, and a layer violation fails to compile.
- [x] Spec it: [docs/conventions/stack-architecture.md](../conventions/stack-architecture.md)
- [x] Build it (conventions + chat)
   - [x] Infrastructure + adapters (AC-3, AC-6): local stack, health endpoints
   - [x] Wiring + integration tests (AC-1, AC-2, AC-4, AC-5, AC-7): workspace, dependency bans, config, CI
- [ ] Verify it (check the convention's ACs on the running stack)
- [x] Perf it: (skipped: no request path of its own; perf is measured from feature 5)
- [ ] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`)
- [ ] Sync it (update this file and the decisions log)
[docs/conventions/stack-architecture.md](../conventions/stack-architecture.md)

### 3. API conventions (standard)
Rules shared by every resource: URLs, pagination, error format, and input validation. Concurrency, idempotency, the perf method, and the N1 to N8 limits moved to Deferred at the MVP cut. Deciding them once is what lets the workflow build any resource without re-deciding the basics.
**Done when:** the shared building blocks (errors, pagination, validation) are tested, and [docs/conventions/api.md](../conventions/api.md) is Accepted.
- [x] Spec it: [docs/conventions/api.md](../conventions/api.md)
- [ ] Build it (conventions + chat)
   - [x] Infrastructure + adapters (AC-1 to AC-10): shared HTTP building blocks (AC-9 and AC-10 are deferred)
   - [ ] Wiring + integration tests (AC-9, AC-10): request-id, API docs
- [ ] Verify it (check the convention's ACs on the running stack)
- [ ] Sync it (update this file and the decisions log)
[docs/conventions/api.md](../conventions/api.md)

### 4. Security baseline
A cross-cutting security standard, mapped to the OWASP API Security Top 10 and enforced by tooling wherever possible.
**Done when:** HS256 token validation, scopes, body limits, dependency auditing, and the no-panic lints are all enforced in CI.
- [x] Spec it: [docs/conventions/security.md](../conventions/security.md)
- [ ] Build it (conventions + chat)
   - [x] Infrastructure + adapters (SEC-1, SEC-2, SEC-5, SEC-6, SEC-9): auth, limits, headers, secrets. Built: JWT auth, `RequireScope`, `xtask token` (SEC-1, SEC-2), body limit and page cap (SEC-5), no internal detail in errors, `X-Cache` only with `CACHE_DEBUG_HEADER=true`, and no start with it under `APP_ENV=production` (SEC-6), secret types, redaction, and no header logging (SEC-9). Security headers are deferred
   - [ ] Wiring + integration tests (SEC-8, SEC-10, SEC-11, SEC-12): lints, audits, container
- [ ] Verify it (check the convention's ACs on the running stack)
- [x] Perf it: (deferred at the MVP cut; the video shows one `oha` cache on/off comparison)
- [ ] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`)
- [ ] Sync it (update this file and the decisions log)
[docs/conventions/security.md](../conventions/security.md)

## Slice 1: Walking skeleton

### 5. Walking skeleton: music
The first resource, a simple CRUD wired end to end through every layer, with the cache and the Kafka commands (create, update, delete). It proves the stack is connected. The engineer designs its fields with `/flow-spec music`, and the video shows `/flow-spec music` and then `/flow-build music` running from the bare boilerplate on the `example/music` branch ([docs/prompts/demo-music.md](../prompts/demo-music.md)). It replaces the earlier `products` example.
**Done when:** all five CRUD operations work over HTTP against the real database, every AC has an integration test, and the feature was built through the flow skills.
- [ ] Tracked as feature 1 in the service scope: [docs/scope/scope.md](../scope/scope.md), with its spec in [docs/specs/](../specs/)

## Slice 2: Cache

### 6. Cache-aside toggle
Reads check the cache first, fall back to the DB on a miss, and populate the cache. Writes delete the key after the DB commit. A config flag turns the whole thing on or off. It lives in a decorator, so use cases never know the cache exists.
**Done when:** every AC in [docs/conventions/cache.md](../conventions/cache.md) passes, including the cache-down case, and the video shows one `oha` comparison of cache on and cache off.
- [x] Spec it: [docs/conventions/cache.md](../conventions/cache.md)
- [ ] Build it (conventions + chat)
   - [ ] Infrastructure + adapters (AC-1 to AC-6): cache helper, decorator, debug header, metrics. Built: `cache_aside` (`RedisCache`, `get_or_load`, `invalidate`), the `cache_status` task-local, the `X-Cache` middleware, and the production guard. Open: the per-resource decorator, which comes with `/flow-build music`. Metrics and AC-6 are deferred
   - [ ] Wiring + integration tests (AC-1 to AC-4, AC-7): toggle, container tests, demo measurement. Built: shared `RedisCache` and `X-Cache` wiring, and container tests of the helper and the header (`crates/bootstrap/tests/cache.rs`). Open: AC-1 to AC-4 end to end need a resource behind the decorator, and the demo measurement. AC-7 is deferred
- [ ] Verify it (check the convention's ACs on the running stack)
- [x] Perf it: (deferred at the MVP cut; the video shows one `oha` cache on/off comparison)
- [ ] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`)
- [ ] Sync it (update this file and the decisions log)
[docs/conventions/cache.md](../conventions/cache.md)

## Slice 3: Alternative transport

### 7. Kafka transport
Accepts commands from topics and calls the same use cases as HTTP. Publishes events, and optionally replies. Processing is idempotent, and bad messages go to a DLQ.
**Done when:** every AC in [docs/conventions/kafka.md](../conventions/kafka.md) passes, and the flow is visible in the message-broker UI.
- [x] Spec it: [docs/conventions/kafka.md](../conventions/kafka.md)
- [ ] Build it (conventions + chat)
   - [ ] Application (AC-1): event publisher port
   - [ ] Infrastructure + adapters (AC-1 to AC-6): envelope, consumer, DLQ, idempotent repository writes. The generic parts are built in `crates/kafka-api` (strict envelope, `CommandHandler`, dispatcher, single-loop consumer, DLQ with `error.reason`; AC-3 and AC-5). Idempotent repository writes (AC-2) and the music handlers need the music resource
   - [ ] Wiring + integration tests (AC-1 to AC-7): container tests, broker UI, demo producer. Built: `wiring::kafka_handlers`, container tests with a dummy handler (AC-1 receive, AC-3, AC-5), `cargo xtask kafka-produce`, and kafka-ui in compose (AC-7, not yet checked in the UI). AC-1 and AC-2 end to end (a DB row, no duplicate on re-send) need the music resource
- [ ] Verify it (check the convention's ACs on the running stack)
- [x] Perf it: (deferred at the MVP cut; the video shows one `oha` cache on/off comparison)
- [ ] Review it (fresh session, using `.agents/skills/flow-review/checklist.md`)
- [ ] Sync it (update this file and the decisions log)
[docs/conventions/kafka.md](../conventions/kafka.md)

## Slice 4: Dogfood

### 8. Dogfood: AI builds a second resource (standard)
_Dropped on 2026-10-10 (engineer decision): the video shows `/flow-spec music` and `/flow-build music` from the bare boilerplate, so feature 5 covers it. The text below is kept for history._

Use the flow skills to have Claude Code and Codex build a new resource with a different shape (for example, one with a relation). This proves the workflow generalizes.
**Done when:** the new resource has the same structure as `products` (checked by eye in the video), and the verification gate passes.
- [ ] Added live in the video with `/flow-spec categories`, tracked in [docs/scope/scope.md](../scope/scope.md) and [docs/specs/](../specs/). (Dropped: `categories` no longer exists in the plan, see feature 5 and [docs/prompts/demo-music.md](../prompts/demo-music.md).)

## Delivery

### 9. Demo video & submission
A 6 to 8 minute video, a README, and an email to the team. This is a non-code feature, so it uses plain task boxes.
**Done when:** the email with the repo and video links has been sent on time.
- [ ] Write the video script from the ACs of features 5 to 8
- [ ] Record and edit
- [ ] Send the email

## Timeline (deadline: Wed 14 Oct 2026)

| Day | Work |
|-----|------|
| Fri 9 | Finish grilling. Conventions and the skills spec become Accepted |
| Sat 10 | Scaffold (feature 2), then the building blocks of features 3 and 4 |
| Sun 11 | The building blocks of features 6 and 7, then `/flow-spec music` (the grill) and `/flow-build music` (feature 5) |
| Mon 12 | Skill tooling (feature 1), then `/flow-verify music` and `/flow-review music` |
| Tue 13 | Codex check (open question W1), then record the video (feature 9) |
| Wed 14 | Buffer, review, submit |

Timeline before the music decision (kept for history):
| (old) Sun 11 | The building blocks of features 6 and 7, then `/flow-build products` (feature 5) |
| (old) Mon 12 | Skill tooling (feature 1) and dogfooding `categories` (feature 8) |
| (old) Tue 13 | Record the video (feature 9) |
| (old) Wed 14 | Buffer, review, submit |

Timeline before the build-order decision (kept for history):
| (old) Sat 10 | Scaffold (feature 2) and CRUD for products (features 3, 5) |
| Sun 11 | Cache (feature 6) and the four skills (feature 1) |
| Mon 12 | Kafka (feature 7) and dogfooding `categories` (feature 8) |

Timeline before the MVP cut (kept for history):
| (old) Fri 9 | Finish grilling. Conventions and the skills spec become Accepted |
| Sat 10 | Features 1 (skill tooling), 2, 3 |
| Sun 11 | Features 4, 5 |
| Mon 12 | Features 6, 7 |
| Tue 13 | Feature 8, perf report, record the video |
| Wed 14 | Buffer, review, submit |

## Deferred
Out of scope for this pass, kept here so the plan stays honest.
- **Transactional outbox and inbox**: fixes the event dual-write problem and supports relative (delta) commands (needs a decision)
- **Hot-key stampede protection** with XFetch (probabilistic early recomputation, VLDB 2015) or single-flight
- **Negative caching** and **delayed double delete**
- **Schema registry (Avro/Protobuf)** for Kafka
- **Mock OIDC provider in compose** for a full token-endpoint flow
- **A second resource that mirrors the first (dogfood)**: was feature 8, dropped on 2026-10-10
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
| 2026-10-10 | Example resource is `music` (engineer decisions): (1) the example changes from `products` to a simple `music` CRUD. The engineer designs its fields later in their own session with `/flow-spec music`, and it uses the cache and the Kafka commands (create, update, delete) so it can demo both. (2) `categories` is dropped completely, so there is no second dogfood resource. Feature 8 is dropped and merged into the music example: the video shows `/flow-spec music` and then `/flow-build music` from the bare boilerplate, and a second resource that mirrors the first moves to Deferred. (3) The branch is `example/music`, created from `main`. `example/products` stays local only (never pushed) and is deleted after music passes review |
| 2026-10-09 | Perf method ([docs/conventions/api.md](../conventions/api.md) A2, A4, A5): the SLI is the percentage of requests under a threshold, in two grades. The ceiling is 99% < 100 ms. Targets come from a rounded baseline. The load is an open model with keep-alive on. The operating rate is the measured knee. Every number comes from a reference or a measurement |

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| P4 | Per-client rate limit (requests per second per token `sub`) for the API | Moved to [docs/conventions/api.md](../conventions/api.md) (open question P4) |

## Resolved questions (closed 2026-10-10 with the recommendation)

| # | Question | Decision |
|---|----------|----------|
| P3 | Submit as a public GitHub repo, or a private repo with invites? | Public repo plus an unlisted video |

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
