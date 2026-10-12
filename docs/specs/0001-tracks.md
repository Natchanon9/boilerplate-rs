# 0001. Tracks resource

**Date**: 2026-10-12
**Status**: Proposed
**Kind**: resource
**Scope feature**: 1. tracks
**Builds on**: [api.md](../conventions/api.md), [security.md](../conventions/security.md), [database.md](../conventions/database.md), [cache.md](../conventions/cache.md), [runtime.md](../conventions/runtime.md), [stack-architecture.md](../conventions/stack-architecture.md)

## Summary
A `track` is one piece of music in a shared catalog: a title, an artist, and a duration. It is the service's first resource, so it exercises every HTTP operation (create, get, list, update, delete) and the cache decorator, and later resources mirror its shape. It has no owner, no unique key besides `id`, and no Kafka commands.

## Context
The music catalog service stores track metadata that other services read and write over the authenticated REST API. The catalog is shared: any caller with the right scope sees and changes every track. Real catalogs hold legitimate duplicates (the same title on several releases), so the service does not dedupe tracks. Track metadata rarely changes after it is created, so reads by id are a good fit for the cache.

## Requirements
**User stories**
- As a catalog client, I want to create, read, list, replace, and delete tracks so that the catalog stays current.
- As a client with read access only, I want writes to be refused so that I cannot change the catalog by mistake.
- As a demo presenter, I want seeded tracks and a cache I can switch on so that paging and cache hits are visible without setup.

**Acceptance criteria**
- **AC-1**: `POST /api/v1/tracks` with a valid body returns `201`, a `Location` of `/api/v1/tracks/{id}`, and the full track: a server-generated UUIDv7 `id`, `title` and `artist` with surrounding whitespace trimmed, `duration_ms`, `version` 1, and `created_at` equal to `updated_at`.
- **AC-2**: An invalid create or update body returns one `422` that lists every failing field:
  - `title`, `artist`, or `duration_ms` missing or `null`: `required`
  - `title` or `artist` empty after trimming: `too_short`; over 200 characters after trimming: `too_long`; containing a control character (U+0000 included): `invalid_format`
  - `duration_ms` below 1 or above 86 400 000: `out_of_range`; a string, a fraction, or a number outside `i32`: `invalid_type`
  - any other field, `id` included: `unknown_field`
- **AC-3**: `GET /api/v1/tracks/{id}` returns `200` with the track, and `404` for an unknown id or a path id that is not a UUID.
- **AC-4**: `GET /api/v1/tracks` lists tracks newest first with `limit` and `cursor` (api.md AC-4). With the 25 seeded tracks and `limit=10`, three pages return 10, 10, and 5 tracks, with no repeats, and the last `next_cursor` is `null`.
- **AC-5**: `PUT /api/v1/tracks/{id}` replaces `title`, `artist`, and `duration_ms`, and returns `200` with the stored track. `version` goes up by 1, `updated_at` is not earlier than before, and `id` and `created_at` stay the same. An unknown id returns `404`, and an invalid body returns `422` (AC-2).
- **AC-6**: `DELETE /api/v1/tracks/{id}` returns `204` and removes the row (hard delete). A second `DELETE`, and a `GET` of the same id, return `404`.
- **AC-7**: Every tracks route requires a valid token (security.md SEC-1). Reads need `tracks:read`, and writes need `tracks:write` (SEC-2). A token with only `tracks:read` gets `403` with `scope="tracks:write"` on `POST`, `PUT`, and `DELETE`. A token with only `tracks:write` gets `403` with `scope="tracks:read"` on get and list.
- **AC-8**: With `cache` in `ENABLE_FEATURES`, the first get of a track is `X-Cache: MISS` and the second is `HIT`. After a `PUT` or `DELETE` of that track, the next get is a `MISS` that returns the new value or `404`. Without `cache`, get is `BYPASS` and Redis holds no `track` keys (cache.md AC-1 to AC-3).
- **AC-9**: The database enforces the same rules as the domain. A raw `INSERT` with `duration_ms` 0, with `duration_ms` 86 400 001, with an empty `title`, or with a 201-character `artist` fails with a check violation.
- **AC-10**: `app db seed` inserts 25 tracks with fixed UUIDv7 ids, and running it a second time leaves exactly 25.
- **AC-11**: `track_repository_contract` passes against both the in-memory fake and `PgTrackRepository` (database.md AC-4).

## Decision
**Chosen**: a shared-catalog track with three required fields, full CRUD over HTTP, the cache decorator on, and no Kafka commands. This is the smallest resource that still exercises every HTTP path, a text value object, an integer range, and the cache.

## Resource design
**Fields**
| Field | Rust type | SQL type | Required | Constraints | Default | Source |
|-------|-----------|----------|----------|-------------|---------|--------|
| id | `Uuid` (v7) | `uuid` PK | yes | | | server |
| title | `TrackTitle(String)` | `text` | yes | single-line; trimmed; 1 to 200 characters ([own proposal](#title-and-artist-length)) | | client |
| artist | `TrackArtist(String)` | `text` | yes | single-line; trimmed; 1 to 200 characters ([own proposal](#title-and-artist-length)) | | client |
| duration_ms | `TrackDuration(i32)` | `integer` | yes | 1 to 86 400 000 (24 h, [own proposal](#duration-bounds)) | | client |
| version | `i64` | `bigint` | yes | >= 1 | 1 | server |
| created_at | `OffsetDateTime` | `timestamptz` | yes | | now() | server |
| updated_at | `OffsetDateTime` | `timestamptz` | yes | | now() | server |

The response body lists all seven fields (the layers.md section 5 response DTO rule). A character is a Unicode scalar value: Rust `str::chars().count()`, which is what PostgreSQL `char_length` counts in a UTF-8 database ([PostgreSQL string functions](https://www.postgresql.org/docs/current/functions-string.html)).

**Invariants**
| Rule | Domain | DB |
|------|--------|----|
| `title` and `artist` are trimmed, then 1 to 200 characters | `TrackTitle` / `TrackArtist` `TryFrom` (trim with `str::trim`, then count) | `CHECK (char_length(title) BETWEEN 1 AND 200)`, the same for `artist` |
| `title` and `artist` hold no control characters | the application parser (`FieldErrors::text`, `Lines::Single`, api.md input error rules) | none: PostgreSQL rejects U+0000 in `text` by itself, and the other control characters are an input rule |
| `duration_ms` is 1 to 86 400 000 | `TrackDuration` `TryFrom<i32>` | `CHECK (duration_ms BETWEEN 1 AND 86400000)` |

The DB does not enforce trimming. `str::trim` removes Unicode whitespace, which `btrim` does not, so a DB check could disagree with the domain. Every write passes through the domain, so stored values are always trimmed.

**Relations**: none.
**Operations**: create, get, list, update, delete.
**Delete**: hard delete, the api.md default (A3).
**List**: newest first, `limit` and `cursor` only (api.md AC-4).
**Indexes**: the primary key only. There is no unique rule, and the list's keyset on `id` uses the primary key index.
**Uniqueness**: none besides `id`. Duplicate title and artist pairs are allowed, so no path returns `409`.
**Authorization**: read `tracks:read`, write `tracks:write`, ownership none (a shared catalog, so SEC-3 does not apply).
**Cache**: enabled, TTL inherited from `CACHE_TTL_SECS` (cache.md AC-5). The key is `RedisCache::key("track", id)`.
**Messaging**: none. No Kafka handler is registered and no topic is added to `kafka-init`.

### Title and artist length
200 characters is an own proposal. No standard caps track titles, and 200 holds real-world titles with room to spare while bounding the row size. Changing it later needs a migration (to change the `CHECK`) and a domain change.

### Duration bounds
1 to 86 400 000 ms (24 h) is an own proposal. It is longer than any real track, a zero or negative length is never valid, and it fits in `i32` (maximum 2 147 483 647, [Rust `i32::MAX`](https://doc.rust-lang.org/std/primitive.i32.html#associatedconstant.MAX)), so `integer` is enough ([PostgreSQL numeric types](https://www.postgresql.org/docs/current/datatype-numeric.html)).

## Test scenarios
- Happy create: POST `{"title": "  Clair de lune ", "artist": "Debussy", "duration_ms": 300000}` returns 201, a `Location`, and a trimmed `title`, `version` 1. Verifies AC-1.
- Validation, all at once: POST `{"title": "", "artist": "a\u0007b", "duration_ms": 0, "id": "x"}` returns one 422 with `title` `too_short`, `artist` `invalid_format`, `duration_ms` `out_of_range`, and `id` `unknown_field`. Separate cases cover `required` (missing and `null`), `too_long` (201 characters), `invalid_type` (`"300"`, `1.5`, `2147483648`), and 86 400 001. Verifies AC-2.
- Get: an existing id returns 200, a random UUIDv7 returns 404, and `/tracks/not-a-uuid` returns 404. Verifies AC-3.
- List: after seeding, `limit=10` pages through 10, 10, and 5 tracks, newest first and without repeats, and the last `next_cursor` is `null`. `limit=101` and `?sort=title` return 422. Verifies AC-4.
- Update: PUT a valid body and check `version` 2, the same `created_at`, and the new values. PUT to an unknown id returns 404, and PUT with an invalid body returns 422. Verifies AC-5.
- Delete: DELETE returns 204, then DELETE again and GET both return 404. Verifies AC-6.
- Auth: no token returns 401 on all five routes. A read-only token is refused POST, PUT, and DELETE with 403 `insufficient_scope`, `scope="tracks:write"`. A write-only token is refused get and list with 403, `scope="tracks:read"`. Verifies AC-7.
- Cache: with the cache on and `CACHE_DEBUG_HEADER=true`, GET twice gives MISS then HIT. PUT, then GET is a MISS with the new title. DELETE, then GET is a 404. With the cache off, GET is BYPASS and `KEYS *:track:*` is empty. Verifies AC-8.
- DB constraints: raw `INSERT`s that break each `CHECK` fail with SQLSTATE `23514`. Verifies AC-9.
- Seeds: run `app db seed` twice and count 25 rows. Verifies AC-10.
- Contract: the fake runs `track_repository_contract` in a unit test, and the Postgres adapter runs it with testcontainers. Verifies AC-11.

## Build plan
<!-- Order is fixed by flow-build/layers.md. Every task cites ACs. -->
### 1. Schema
- [x] `sqlx migrate add -r --source migrations/postgres create_tracks`. Up: the `tracks` table with the standard columns, `title text NOT NULL`, `artist text NOT NULL`, `duration_ms integer NOT NULL`, and the three `CHECK` constraints from Invariants. Down: `DROP TABLE tracks` (AC-1, AC-9)
- [x] `seeds/postgres/0001_tracks.sql`: 25 tracks with fixed, time-ordered UUIDv7 ids and `ON CONFLICT (id) DO NOTHING` (AC-4, AC-10)
### 2. Domain
- [x] `crates/domain/src/track/`: `Track` with private fields and getters, `Track::new`, and `replace(title, artist, duration)` for update. Value objects `TrackTitle`, `TrackArtist` (trim, then 1 to 200 characters), and `TrackDuration` (1 to 86 400 000). `TrackError` via `thiserror`, with unit tests for each bound (AC-1, AC-2, AC-5)
### 3. Application
- [x] `ports.rs`: `TrackRepository` with `insert`, `find_by_id`, `list`, `update`, and `delete` (AC-1, AC-3 to AC-6)
- [x] `commands.rs`: `CreateTrackInput` and `UpdateTrackInput` (`title`, `artist`: `Field<String>`; `duration_ms`: `Field<i32>`; `unknown`). `parse_create` and `parse_update` collect every error and map `TrackError` to `too_short`, `too_long`, and `out_of_range` (AC-2)
- [x] `use_cases.rs`: `TrackUseCases` with `create` (generates the UUIDv7), `get`, `list`, `update`, and `delete`. Not found maps to `AppError::NotFound` (AC-1, AC-3 to AC-6)
- [x] `fakes.rs` and `contract.rs`, plus the first-resource setup from layers.md section 3: the `testing` feature in `application`, enabled as a dev-dependency feature in `infrastructure` and `bootstrap` (AC-11)
### 4. Infrastructure
- [x] `postgres/track_repo.rs`: `TrackRow`, `PgTrackRepository` with the insert, update, list, and delete rules from layers.md section 4, and errors mapped through `postgres::unavailable` (AC-1, AC-3 to AC-6, AC-11)
- [x] `redis/track_cache.rs`: `CachedTrackRepository<R>` with `get_or_load` on `find_by_id`, and `invalidate` after `update` and `delete` (AC-8)
### 5. Inbound adapters
- [x] `crates/http-api/src/track/`: `dto.rs` (`CreateTrackRequest`, `UpdateTrackRequest`, `TrackResponse` with all seven fields), `handlers.rs`, and `routes.rs` with `tracks:read` on get and list, and `tracks:write` on post, put, and delete (AC-1 to AC-7)
- No Kafka handler (Messaging: none).
### 6. Wiring
- [x] `track_use_cases(&Infra)` in `wiring.rs`: `PgTrackRepository`, wrapped in `CachedTrackRepository` when `infra.cache` is `Some` with `cache.ttl`. Merge the routes into `api_v1`. `kafka_handlers` is unchanged (AC-1 to AC-8)
- [x] Update the domain, application, infrastructure, and http-api `Cargo.toml` files for the first resource (`domain` dependency, `uuid` with `serde` in infrastructure; layers.md, Dependency rule) (AC-1)
- [x] `cargo sqlx prepare --workspace -- --all-targets --all-features` and commit `.sqlx/` (AC-1)
### 7. Integration tests
- [x] `crates/bootstrap/tests/track_*.rs` with testcontainers, one or more `ac_<n>_<behavior>` tests per AC: `ac_1` to `ac_7` over HTTP, `ac_8` with Redis, `ac_9` with raw SQL, `ac_10` through the seed command, and `ac_11` with the Postgres contract run (AC-1 to AC-11)
### 8. Gate: make verify
- [x] `make verify` passes (AC-1 to AC-11)

## Consequences
- With no unique key, a client that retries a `POST` can create a duplicate track (Idempotency-Key is deferred in api.md). This is accepted, because duplicate tracks are already legal.
- With no Kafka commands, kafka.md AC-1 and AC-2 (a command writes a row, and a re-sent command does not duplicate it) remain unproven until a later resource declares commands.
- Trimming is enforced only in the domain. A row written outside the app (a manual `INSERT`) could hold untrimmed text, and `GET` would return it as stored.
- As the first resource, this sets the pattern later resources mirror (flow-build "Generic first"), so its shape is worth reviewing closely.

## Open questions
| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## References
- PostgreSQL string functions (`char_length`): https://www.postgresql.org/docs/current/functions-string.html
- PostgreSQL numeric types (`integer` range): https://www.postgresql.org/docs/current/datatype-numeric.html
- Rust `i32::MAX`: https://doc.rust-lang.org/std/primitive.i32.html#associatedconstant.MAX
