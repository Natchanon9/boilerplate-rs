# 0001. Tracks: verify report

Spec: [0001-tracks.md](../specs/0001-tracks.md). Each run is appended under its own dated heading.

## 2026-10-12 16:05, commit 9366a8c (uncommitted tracks build in the working tree)

**Setup:** `make verify` passed (`verify: all checks passed`). `make up` rebuilt the app image. `/readyz` returned 200. `app info features`: `cache enabled`, `kafka disabled`. `CACHE_DEBUG_HEADER=true`, `CACHE_TTL_SECS=300`. Tokens came from `cargo xtask token` with the scopes `tracks:read tracks:write`, `tracks:read`, `tracks:write`, and `widgets:read`. HTTP went to `127.0.0.1:8080`, the DB was read with `psql` in the `postgres` container, and Redis with `redis-cli` in the `redis` container.

| AC | Result | Evidence |
|----|--------|----------|
| AC-1 | PASS | `POST {"title":"  Clair de lune ","artist":"Debussy","duration_ms":300000}` returned `HTTP/1.1 201 Created` with `location: /api/v1/tracks/01a12f5b-b06b-71bb-9461-b9d755ace17a`. The body has `"title":"Clair de lune"` (trimmed), `"version":1`, and `created_at` = `updated_at` = `2026-10-12T09:05:00.523694Z`. The id's version nibble is `7` (UUIDv7). The DB row matches: `Clair de lune \| Debussy \| 300000 \| 1` |
| AC-2 | PASS | `{"title":"","artist":"a\u0007b","duration_ms":0,"id":"x"}` returned one `422` listing `id` `unknown_field`, `title` `too_short`, `artist` `invalid_format`, `duration_ms` `out_of_range`. `{"title":null}` returned `required` for `title`, `artist`, and `duration_ms`. A 201-character title with `\u0000` in `artist` returned `too_long` and `invalid_format`. `86400001` returned `out_of_range`. `"300"`, `1.5`, and `2147483648` each returned `invalid_type`. A 200-character title with `duration_ms` 1, and `duration_ms` 86400000, each returned `201`. A PUT with `{"title":"   ","duration_ms":-5,"version":3}` returned `422` with `version` `unknown_field`, `title` `too_short`, and `duration_ms` `out_of_range` |
| AC-3 | PASS | `GET /tracks/01a12f5b-b06b-…` returned `200` with the full body. An unknown UUIDv7 returned a problem+json `404`. `/tracks/not-a-uuid` also returned `404` |
| AC-4 | PASS | With `limit=10`, the pages returned 10, 10, and 6 items, and the last `next_cursor` was `null`. That is 26 ids, all unique, in strictly descending order (newest first), and none of the 25 seed ids is missing. The table held 26 rows, not 25, because of one pre-existing non-seed row ("Imagine", created 08:28 UTC, not made by this run), so the last page has 6 items instead of the spec's 5. With no `limit`, 10 items came back. `limit=101` and `limit=0` returned `422 limit out_of_range`, `sort=title` returned `422 sort unknown_field`, and a garbage cursor returned `422 cursor invalid_format` |
| AC-5 | PASS | PUT `{"title":"Rêverie","artist":"Claude Debussy","duration_ms":290000}` returned `200`. In the DB, version went from 1 to 2, `created_at` stayed `09:05:00.523694`, and `updated_at` moved from `09:05:00.523694` to `09:05:00.949815`. A PUT to an unknown id returned `404`. The invalid body is covered in AC-2 |
| AC-6 | PASS | `DELETE` returned `204 No Content` and the DB had `rows with id: 0`. A second `DELETE` returned `404`, and a `GET` after the delete returned `404` |
| AC-7 | PASS | No token on any of the five routes returned `401` with `www-authenticate: Bearer`. A malformed token returned `401` with `Bearer error="invalid_token"`. A read-only token on POST, PUT, or DELETE returned `403` with `Bearer error="insufficient_scope", scope="tracks:write"`, and the track was still there. A write-only token on list or get returned `403` with `scope="tracks:read"`. A `widgets:read` token on list returned `403` with `scope="tracks:read"` |
| AC-8 | PASS | Get 1 was `x-cache: MISS` and get 2 was `x-cache: HIT`. Key `rs-boilerplate:local:v1:track:01a12f5b-b2b2-…`: `EXISTS 1`, `TTL 300`, and the value is the row JSON. After the PUT, `EXISTS 0` and the next get was `MISS` with `"title":"Cache me again","version":2`. After the DELETE, `EXISTS 0` and the next get was `404` with `x-cache: MISS`. The list response had no `X-Cache` header. The no-cache branch (`BYPASS`, no keys) was not run live, because the stack runs with `ENABLE_FEATURES=cache`. It is covered by `ac_8_without_the_cache_get_is_bypass_and_redis_holds_no_track_keys`, which passed in `make verify` |
| AC-9 | PASS | Raw inserts with `duration_ms` 0 and with 86400001 failed with `ERROR: 23514 … violates check constraint "tracks_duration_ms_range"`. An empty title failed on `"tracks_title_length"`, and a 201-character artist failed on `"tracks_artist_length"`. `\d tracks` lists all three constraints, with `char_length(...) >= 1 AND <= 200` and `duration_ms >= 1 AND <= 86400000` |
| AC-10 | PASS | `make seed` ran twice and logged `seeds applied … 0001_tracks.sql` both times. Afterwards, `seeded rows present: 25` (counted by the 25 ids in `seeds/postgres/0001_tracks.sql`) |
| AC-11 | PASS | From the `make verify` output: `test track::fakes::tests::the_fake_passes_the_repository_contract ... ok`, `test ac_11_postgres_repository_passes_the_contract ... ok`, `test ac_11_cached_repository_passes_the_contract_too ... ok` |

**Cross-cutting checks**
| Check | Result | Evidence |
|-------|--------|----------|
| 401 and 403 (security.md SEC-1, SEC-2) | PASS | See AC-7 |
| 404, and 422 with field codes (api.md AC-3, AC-7, AC-8) | PASS | See AC-2, AC-3, and AC-4. Invalid JSON returned `400` with `"detail":"The request body is not valid JSON."`, and the body `[1]` returned `422` with `body` `invalid_type`. Every error was `application/problem+json` with an `instance` |
| Fail-open with Redis stopped (cache.md AC-4) | PASS | After `docker compose stop redis`, a GET returned `200` with `x-cache: ERROR` in 0.009 s, and the log had `WARN "cache read failed; reading from the database" … "error":"broken pipe"` (target `infrastructure::redis::cache_aside`). After `docker compose start redis`, a probe 2 s later was still `ERROR` (`"timed out"`, Redis not ready yet). Probes from 16:05:18 onward were `MISS`, then `HIT`, so the cache recovers by itself |
| Kafka commands | N/A | The spec has `**Messaging**: none` |

**Cleanup:** the run deleted the rows it created through the API. That cleanup also deleted seed row `0192f1a2-0000-7000-8000-000000000001` by mistake, because its artist matched the cleanup filter. `make seed` restored it. Final state: the 25 seed rows plus the pre-existing "Imagine" row, and no `track` keys in Redis.

Summary: 11 pass, 0 fail, 0 not run. Next: `/flow-review tracks`.
