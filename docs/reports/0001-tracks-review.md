# 0001. Tracks: review report

Spec: [0001-tracks.md](../specs/0001-tracks.md). Verify report: [0001-tracks-verify.md](0001-tracks-verify.md) (11 pass, 0 fail). Each run is appended under its own dated heading.

## 2026-10-12, commit 9366a8c (uncommitted tracks build in the working tree), reviewer claude-sonnet-5-5 (fresh session, did not write the code; same model family as the builder, so it shares some blind spots)

Checked: `make ref-check` and `make sql-check` pass. The rest of `make verify` was not re-run; the verify report shows it passing.

| # | Severity | Rule | Location | Finding | Failure scenario | Suggested fix |
|---|----------|------|----------|---------|------------------|---------------|
| 1 | minor | ARCH-7 | `.sqlx/` (untracked) | The offline query data is not committed yet. Spec task 6 says to commit it. | The Docker build compiles offline from `.sqlx/`. Merging the code without the folder makes `docker build` fail with "set DATABASE_URL or run cargo sqlx prepare". | Commit `.sqlx/` with the code, after a final `cargo sqlx prepare --workspace -- --all-targets --all-features`. |
| 2 | minor | PERF-5 | [track_cache.rs:36-44](../../crates/infrastructure/src/redis/track_cache.rs#L36-L44) | A cache-aside race: a reader that loaded the old row before a write can store it after the write's `invalidate`. | Reader A misses and loads v1. A `PUT` commits v2 and runs `DEL`. A then runs `SET` with v1. Gets return v1 until the TTL (300 s) expires. | None needed for the MVP, because cache.md bounds staleness by the TTL (AC-5). If you want it closed, note the race in cache.md as accepted. The fix belongs in the shared `cache_aside`, not in this resource. |
| 3 | nit | SEC-6 | [track_repo.rs:79-81](../../crates/infrastructure/src/postgres/track_repo.rs#L79-L81) | A unique violation is mapped to `AppError::Conflict(db.message())`. `ApiError` shows a `Conflict` detail to the client, and the comment there says the use case writes that text. | Not reachable for tracks: the only unique key is the primary key, which `ON CONFLICT (id) DO NOTHING` handles. But later resources copy this adapter, and a unique violation on one of them would return the Postgres message (constraint name and key value) in the 409 body. | Have the adapter return a fixed message, or leave the message to the use case. Raise it with layers.md section 4, because the pattern comes from there. |
| 4 | nit | ARCH-6 | [commands.rs:108-109](../../crates/application/src/track/commands.rs#L108-L109) | `field_error` maps `TrackError::InvalidVersion` to field `version` with `out_of_range`. No request has a `version` field, and no input can produce this error. | If a future caller passed it, the client would get a 422 for a field it never sent. | Make the parse-time error type exclude `InvalidVersion`, or make the arm `unreachable`-free by returning `AppError::Internal`. |
| 5 | nit | ARCH-7 | `script.txt` (untracked, repo root) | A scratch file holds two `cargo xtask token` command lines. It is not in the spec. | Committed with `git add -A`, it adds a stray root file. | Delete it, or move the commands into the README demo section. Do not commit it. |

AC coverage: 11/11 ACs have passing tests. Missing: none. AC-1 to AC-7 are `ac_<n>_*` in `track_http.rs`, AC-8 in `track_cache.rs`, AC-9 to AC-11 in `track_db.rs`. The AC-11 fake run is `track::fakes::tests::the_fake_passes_the_repository_contract`, a unit test named for the contract rather than `ac_11_*`. The Postgres run is `ac_11_postgres_repository_passes_the_contract`. Awaits are bounded by `support::STEP` through the shared helpers, with no sleeps (TEST-3), and failure paths are covered (TEST-2).

Other checks:
- **Dependency rule (ARCH-1):** `domain` uses `thiserror`, `time`, and `uuid` only. `application` gets `time` only behind the `testing` feature. The adapters carry the serde derives.
- **Thin handlers (ARCH-2):** the handlers only extract, parse, call the use case, and map.
- **DTOs (ARCH-3):** `TrackResponse` lists all seven fields. The cached `TrackRow` follows the AGENTS.md cache rule (the row type is the cached value).
- **Auth (SEC-2):** every method carries its own `RequireScope`.
- **Unknown fields (SEC-4):** the requests use `UnknownFields`.
- **SQL (SEC-8):** all statements are `query!` / `query_as!` macros.
- **Pagination (API-4, PERF-1):** the list is keyset on the primary key with `LIMIT`. No N+1 (PERF-2).
- **Invalidation (PERF-5):** `update` and `delete` invalidate after the inner commit, and get has a TTL.
- **Documentation (DOC-1):** the limits are labeled `own proposal` and the bounds link sources.
- **Panics (SEC-11):** no `unwrap`, `expect`, or `panic!` outside tests.

Verdict: ready to merge. No blocker or major finding. Findings 1 and 5 are housekeeping for the commit.

**To close the resource on acceptance** (not done yet, because the engineer has not accepted):
- Tick `Verify it` and `Review it` in the tracks row of [scope.md](../scope/scope.md). `Verify it` is still unticked there although the verify report exists.
- Set its status to `done` in the `At a glance` table.
- Move the spec from `Proposed` to `Accepted`.
- `Assumed` decisions to ratify with `/flow-spec`: none. The spec lists no open questions.

## 2026-10-12, resolution
Findings 3 and 4 were fixed by the engineer and `make verify` passes. Finding 2 is accepted as a known trade-off (the TTL bounds staleness). Findings 1 and 5 are left for the engineer's commit. The engineer accepted the verdict, so the resource is closed: scope row ticked and `done`, spec `Accepted`.
