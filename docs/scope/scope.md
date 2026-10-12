# Scope: music-catalog

A music catalog service that stores track metadata for other services, read and written over an authenticated REST API. `/flow-spec <resource>` adds each resource's row.

## At a glance
| # | Feature | Phase | Status |
|---|---------|-------|--------|
| 1 | tracks | MVP | done |

## MVP

### 1. tracks
A shared catalog of music tracks (title, artist, duration) with full CRUD over HTTP and the cache decorator on. No owner and no Kafka commands.
**Done when:** a client with `tracks:write` creates, replaces, and deletes tracks, a client with `tracks:read` gets and pages through them, and with the cache on a repeat get is a `HIT`.
- [x] Spec it: /flow-spec tracks
- [x] Build it: /flow-build tracks
   - [x] Schema + domain (AC-1, AC-2, AC-5, AC-9, AC-10)
   - [x] Application (AC-1 to AC-6, AC-11)
   - [x] Infrastructure + adapters (AC-1 to AC-8, AC-11)
   - [x] Wiring + integration tests (AC-1 to AC-11)
- [x] Verify it: /flow-verify tracks
- [x] Review it: /flow-review tracks
[spec 0001](../specs/0001-tracks.md), code `crates/*/src/track/`, `crates/infrastructure/src/{postgres/track_repo.rs,redis/track_cache.rs}`, `migrations/postgres/20261012075121_create_tracks.*.sql`, `seeds/postgres/0001_tracks.sql`, `crates/bootstrap/tests/track_*.rs`

## Open questions
| # | Question | Recommendation |
|---|----------|----------------|

## Legend
- **Status** (in `At a glance` only): `planned` > `in-progress` > `done`. `dropped` is kept for history, never deleted.
- **Spec status:** `Proposed` > `Accepted`. Also `Superseded`, and `Assumed` (built before deciding; ratify with `/flow-spec`).
- **Flow:** `/flow-spec` > `/flow-build` > `/flow-verify` > `/flow-review`.
- **needs a decision:** run `/flow-spec` before `/flow-build`.
- **Next step:** the first unticked box.
- Box format and box owners: `.agents/skills/flow-spec/SKILL.md` ("Scope row").
