# API conventions for every resource

**Date**: 2026-10-09 (cut to MVP on 2026-10-10)
**Status**: Accepted
**Kind**: capability
**Built in**: [assignment feature 3](../assignment/scope.md)

## Summary

These are the shared rules every CRUD resource follows: URL shape, pagination, error format, and input validation. Because they are decided once here, a resource spec only has to cover what is specific to that resource. That is what lets the workflow handle any kind of resource.

This file describes the **MVP**. Everything that was designed but cut to keep the MVP small is kept under [Deferred](#deferred), each with its reason.

## Requirements

**Acceptance criteria** (these apply to every resource, and each resource's integration tests must cover them)
- **AC-1**: Paths are `/api/v1/{resources}` (plural, kebab-case). JSON fields are snake_case.
- **AC-2**: `POST` returns `201` with a `Location` header ([RFC 9110 15.3.2](https://www.rfc-editor.org/rfc/rfc9110#name-201-created)) and the resource body.
- **AC-3**: `GET /{id}` returns `200` with the resource, or `404` when it does not exist.
- **AC-4**: List uses cursor (keyset) pagination, newest first. `limit` defaults to 10 with a maximum of 100 ([Stripe pagination](https://docs.stripe.com/api/pagination)). Anything above the maximum returns `422`. The response is `{ "data": [...], "next_cursor": "..." | null }`. `limit` and `cursor` are the only query parameters, and any other one returns `422`.
- **AC-5**: `PUT /{id}` is a full replacement. It returns `200` with the full body, `404` when the resource does not exist, or `422` for invalid input. There is no precondition header in the MVP; the last write wins.
- **AC-6**: `DELETE /{id}` returns `204`. Deleting the same resource again returns `404`.
- **AC-7**: Every error is [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) `application/problem+json` (`type`, `title`, `status`, `detail`, `instance`). Validation errors return `422` with `errors: [{ field, code, message }]`.
- **AC-8**: Syntactically invalid JSON returns `400`. Every other bad input returns `422`, following the input error rules below. A body over the limit returns `413` ([RFC 9110 15.5.14](https://www.rfc-editor.org/rfc/rfc9110#name-413-content-too-large)).

AC-9 and AC-10 are deferred.

## Decision

- **PUT (full replacement)** instead of PATCH. The semantics are unambiguous, validation is simple, and AI generates it reliably. JSON Merge Patch is deferred.
- **Keyset pagination on `id`**, newest first (UUIDv7 is already time-ordered). The cost of a page stays constant however deep it is, unlike OFFSET. The cursor is a base64url-encoded key: opaque, and unsigned because it is not secret.
- **`version` column**: kept on every table and incremented on each update, so optimistic locking can be added later without a migration (own proposal). The MVP does not check it.
- **Write responses**:
  - `POST` returns `201` and `PUT` returns `200`, both with the full resource body ([RFC 9110 9.3.4](https://www.rfc-editor.org/rfc/rfc9110#name-put) allows 200 for a successful PUT; own proposal to return the body, so clients need no second GET).
  - `PUT` is a full replacement, so an omitted optional field becomes `null`.
  - An immutable field may appear in a `PUT` body. It is compared after the same normalization as on create. A different value returns `422` with code `immutable`.
- **Response shape** (G10):
  - Every field is always present, and optional fields are `null` rather than omitted (own proposal: one shape for clients to parse).
  - Timestamps are [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) in UTC.
  - `Location` is the relative path `/api/v1/<resources>/{id}` ([RFC 9110 10.2.2](https://www.rfc-editor.org/rfc/rfc9110#name-location) allows a URI reference).
- **Ids and timestamps** (G10):
  - On HTTP create, the application use case generates the UUIDv7. On Kafka create, the producer supplies it ([kafka.md](kafka.md)).
  - The database sets the timestamps: `DEFAULT now()` on insert and `updated_at = now()` on update, both returned with `RETURNING` (own proposal: one clock, the database's).
- **Input error rules** (G6, G7):
  - `400` only when the body is not syntactically valid JSON.
  - Everything else returns `422` with one entry per failing field. All failures are collected, not only the first. Codes:
    - `required`: missing or `null`
    - `unknown_field`
    - `invalid_type`: wrong JSON type, including a fractional or out-of-range number for an integer field
    - `invalid_format`, `too_short`, `too_long`, `out_of_range`: value rules
    - `immutable`
  - Text fields always reject U+0000, because PostgreSQL text cannot store it ([PostgreSQL character types](https://www.postgresql.org/docs/current/datatype-character.html)).
  - Single-line fields also reject other control characters (Unicode category Cc). Multi-line fields, which the resource spec declares, allow `\n`, `\r`, and `\t` (own proposal). The code is `invalid_format`.
  - Where validation lives: DTOs carry raw JSON values and do no checks. The application layer (`commands.rs`) parses the raw input into domain value objects and collects every field error into `AppError::Validation`. HTTP and Kafka call the same parser, so validation is identical, and `kafka-api` never depends on `http-api`.
  - How every failure is collected (decided 2026-10-10, feature 3): serde stops at the first error, so no input field is deserialized directly.
    - Every DTO field is `application::input::Field<T>`: `Missing`, `Null`, `Invalid` (wrong JSON type), or `Present(T)`. Deserializing it never fails, and the parser turns `Missing` and `Null` into `required`, and `Invalid` into `invalid_type`.
    - Unknown fields are captured with `#[serde(flatten)] unknown: UnknownFields` instead of `deny_unknown_fields`, which stops at the first one. The DTO is still an allowlist: the parser reports each unknown field as `unknown_field` ([security.md](security.md) SEC-4).
    - `Field<T>` and `UnknownFields` live in `application::input`, with `Deserialize` behind the `serde` feature, so HTTP and Kafka share one implementation.
  - A body that is valid JSON but not an object returns `422` with `{field: "body", code: "invalid_type"}` (own proposal).
  - A body without a JSON `Content-Type` returns `415`, as axum's `Json` extractor does ([axum `Json`](https://docs.rs/axum/latest/axum/struct.Json.html), [RFC 9110 15.5.16](https://www.rfc-editor.org/rfc/rfc9110#name-415-unsupported-media-type)).
- **List and path parameter errors**:
  - A bad `limit` or `cursor` returns `422` with a field entry (`invalid_type`, `out_of_range`, or `invalid_format`).
  - A path `{id}` that is not a UUID returns `404`, so the id format is not revealed (own proposal).

- **Error responses** (decided 2026-10-10, feature 3):
  - `AppError` maps to a status: `Validation` 422, `NotFound` 404, `Conflict` 409, `Unauthorized` 401, `Forbidden` 403, `Unavailable` 503 ([RFC 9110 15.6.4](https://www.rfc-editor.org/rfc/rfc9110#name-503-service-unavailable)), and `Internal` 500 ([RFC 9110 15.6.1](https://www.rfc-editor.org/rfc/rfc9110#name-500-internal-server-error)).
  - `type` is always `about:blank`, so `title` is the status phrase ([RFC 9457 4.2.1](https://www.rfc-editor.org/rfc/rfc9457#name-aboutblank)). Clients tell 422 errors apart by `errors[].code`.
  - `instance` is the request path ([RFC 9457 3.1.5](https://www.rfc-editor.org/rfc/rfc9457#name-instance) allows a URI reference; own proposal to use the path).
  - For `Unavailable` and `Internal`, `detail` is a fixed sentence. The cause goes to the log only ([security.md](security.md) SEC-6).
  - Requests the router cannot match (unknown path `404`, wrong method `405`) and a body over the limit (`413`) are problem+json too (AC-7).

### Shared building blocks (in http-api, written once, used by every resource)

| Module | Role |
|--------|------|
| `error::ApiError` | Maps `AppError` to problem+json |
| `pagination::{ListQuery, Cursor, PageResponse<T>}` | Parses and validates `limit` and `cursor` into `application::pagination::PageRequest` (the type repository ports take), rejects any other query parameter, and renders `{data, next_cursor}` |
| `extract::JsonBody<T>` | Checks the content type (415), applies the body limit (413), parses JSON (400 on a syntax error), and deserializes into a DTO of `Field<T>` values (input error rules). Value checks happen in the application parser, not here |
| `extract::IdPath` | Parses `{id}` as a UUID; anything else is `404` |
| `auth::{Principal, RequireScope}` | Authentication and authorization ([security.md](security.md)) |

### MVP limits (library defaults or a cited value)

| Limit | MVP value | Source |
|-------|-----------|--------|
| Request body | 2 MiB | [axum `DefaultBodyLimit` default](https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html) |
| Page size | default 10, maximum 100 | [Stripe pagination](https://docs.stripe.com/api/pagination) |
| JWT clock leeway | 60 s | [`jsonwebtoken` `Validation` default](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/struct.Validation.html), within [RFC 7519](https://www.rfc-editor.org/rfc/rfc7519)'s "no more than a few minutes" |
| Cache TTL | 300 s | own proposal: every write already deletes the key, so the TTL only bounds staleness when a delete is missed, and most resource data changes rarely ([AWS: pick TTL from the rate of change](https://docs.aws.amazon.com/whitepapers/latest/database-caching-strategies-using-redis/cache-validity.html)) |
| Kafka message size | broker default `message.max.bytes` | [Kafka broker configs](https://kafka.apache.org/documentation/#brokerconfigs_message.max.bytes) |
| Request timeout | none set in the MVP | deferred (N4) |
| Database connection acquire | `acquire_timeout` 1 s, `acquire_slow_threshold` 500 ms, `max_connections` 10; bounds getting a connection, not a query | [database.md](database.md) Pool row and P1, decided 2026-10-11 with measured latencies |
| Cache call timeout (Redis) | response timeout 500 ms, connection timeout 1 s; reconnect retries 0 (the library default is 6), so a call during a Redis outage makes one connection attempt and fails open | the redis-rs 1.7.1 defaults ([`DEFAULT_RESPONSE_TIMEOUT`, `DEFAULT_CONNECTION_TIMEOUT`](https://docs.rs/redis/1.7.1/src/redis/client.rs.html#180-182), [`ConnectionManagerConfig` defaults](https://docs.rs/redis/1.7.1/src/redis/aio/connection_manager.rs.html#113-115)), decided 2026-10-10; 0 retries decided 2026-10-10 with measured latencies ([cache.md](cache.md) C9). A timeout from a measured Redis p99.9 (N3) stays deferred ([cache.md](cache.md), Redis timeouts) |

## Build plan
1. `ApiError` + problem+json, with tests for every variant (AC-7, AC-8)
2. `JsonBody`, the body limit, `Field<T>` and `UnknownFields`, and the input error rules (AC-8)
3. `pagination` + cursor encode and decode, with tests (AC-4)

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

## Resolved questions (closed 2026-10-10 with the recommendation)

| # | Question | Decision |
|---|----------|----------|
| A3 | Hard delete or soft delete (`deleted_at`)? | Hard delete by default. Each resource spec may opt into soft delete |

## Deferred

Designed and decided, then cut on 2026-10-10 to keep the MVP small. Each item keeps its original text, so the reasoning is available in review and in the interview.

### ETag and If-Match (optimistic locking)
**Deferred:** the brief does not ask for concurrency control, and last-write-wins is enough for the demo. **MVP uses:** `PUT` without preconditions; the `version` column is kept, so this can return without a migration.

Original text:
- **AC-3**: `GET /{id}` returns `200` with `ETag: "<version>"` ([RFC 9110 8.8.3](https://www.rfc-editor.org/rfc/rfc9110#name-etag)), or `404` when the resource does not exist.
- **AC-5**: `PUT /{id}` requires `If-Match`. A missing header returns `428` ([RFC 6585 3](https://www.rfc-editor.org/rfc/rfc6585#section-3)), and a version mismatch returns `412` ([RFC 9110 13.1.1](https://www.rfc-editor.org/rfc/rfc9110#name-if-match)).
- **PUT (full replacement) with If-Match** instead of PATCH. The semantics are unambiguous, validation is simple, and AI generates it reliably. JSON Merge Patch is deferred.
- **ETag = version**, the integer `version` column.
- **Write responses** (added 2026-10-10, independent spec check G5):
  - `POST` returns `201` and `PUT` returns `200`, both with the full resource body and its `ETag` ([RFC 9110 9.3.4](https://www.rfc-editor.org/rfc/rfc9110#name-put) allows 200 for a successful PUT; own proposal to return the body, so clients get the new version without a second GET).
  - `PUT` is a full replacement, so an omitted optional field becomes `null`.
  - An immutable field may appear in a `PUT` body. It is compared after the same normalization as on create. A different value returns `422` with code `immutable`.

| Module | Role |
|--------|------|
| `concurrency::{IfMatch, ETag}` | Extractor and header |
4. `IfMatch` / `ETag` (AC-3, AC-5)

### List filters and sorting
**Deferred:** the brief asks for CRUD; a newest-first list with a cursor covers it. **MVP uses:** `limit` and `cursor` only.

Original text:
- **AC-4**: List uses cursor (keyset) pagination. `limit` has a default and a maximum (N7). Anything above the maximum returns `422`. The response is `{ "data": [...], "next_cursor": "..." | null }`. Only the filters and sort keys declared in the resource spec are accepted, using the list query syntax below. An unknown parameter returns `422`.
- **List query syntax** (added 2026-10-10, when the first resource spec needed it):
  - `sort=<key>` sorts ascending and `sort=-<key>` sorts descending ([JSON:API, sorting](https://jsonapi.org/format/#fetching-sorting)).
  - **Tie-break** (G4): sorting by `<key>` orders by `(<key>, id)`, and `id` follows the key's direction, so `sort=-k` means `k DESC, id DESC`. Keyset paging uses a row comparison such as `(k, id) < ($1, $2)` ([PostgreSQL row-wise comparison](https://www.postgresql.org/docs/current/functions-comparisons.html#ROW-WISE-COMPARISON)). One `(k, id)` index serves both directions, because a B-tree can be scanned backward ([PostgreSQL indexes and ORDER BY](https://www.postgresql.org/docs/current/indexes-ordering.html)). One sort key per request (own proposal: keyset pagination over several user-chosen keys adds complexity that no resource needs yet). With no `sort`, the resource spec's default applies.
  - Exact-match filter: `<field>=<value>`.
  - Range filter: `<field>_min` and `<field>_max`, both inclusive. `_min` greater than `_max` returns `422` (own proposal: no standard covers range naming, and this reads naturally next to the field name).
  - The cursor encodes the sort key and filters it was issued for. Reusing it with a different `sort` or different filters returns `422` (own proposal: a keyset cursor holds the last row's sort-key values, so applying it to another ordering would skip or repeat rows).

- **List and path parameter errors** (G8):
  - A bad `limit`, `cursor`, `sort`, or filter returns `422` with a field entry (`invalid_type`, `out_of_range`, or `invalid_format`).
  - Filter values must also satisfy the field's own value rules.
  - A path `{id}` that is not a UUID returns `404`, so the id format is not revealed (own proposal).

### Idempotency-Key on POST
**Deferred:** HTTP client retries are not in the brief; the Kafka path is idempotent by design. **MVP uses:** nothing; a retried POST can create a second row.

| # | Question | Recommendation (at the time) |
|---|----------|----------------|
| A1 | Support `Idempotency-Key` on POST? | Should-have. Use an `idempotency_keys` table (key, request hash, stored response, retention N5). If time runs out, cut it and say so in the README |

### Request id and OpenAPI
**Deferred:** not needed to demo the brief, and they add middleware and annotations to every route. **MVP uses:** structured logs, and request examples in the README.

- **AC-9**: Every response carries `X-Request-Id`. A client-supplied value is echoed back, otherwise one is generated.
- **AC-10**: OpenAPI is generated from the code. `/docs` is served only when `API_DOCS_ENABLED=true`.
5. Request-id middleware (AC-9), OpenAPI wiring (AC-10)

### Rate limit value (P4)
**Deferred:** rate limiting itself is deferred ([security.md](security.md)). **MVP uses:** no rate limit.

| # | Question | Recommendation (at the time) |
|---|----------|----------------|
| P4 | Per-client rate limit (requests per second per token `sub`) | Ask the team. It is a business number ([OWASP API4](https://owasp.org/API-Security/editions/2023/en/0xa4-unrestricted-resource-consumption/)). Until answered, the value is config. Moved here from the assignment scope |

### Performance budgets (method decided 2026-10-09, A2, A4, A5; numbers pending the baseline run)
**Deferred:** an SLO method (two grades, capacity search, Kneedle knee, baseline gate) is production practice that the MVP does not need. **MVP uses:** no load measurement; the demo shows the cache through `X-Cache` (the oha cache on/off comparison is deferred too, [cache.md](cache.md) Deferred).

Every number below either comes from a cited reference or is produced by measurement. Nothing is picked arbitrarily. Rules marked **(own proposal)** are reasoning, not taken from a reference.

1. **SLI**: the percentage of requests faster than a threshold, measured at the client (the load generator). Reports still show p50, p95, and p99 for readability. ([Google CRE](https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/))
2. **Two grades for each operation**: `90% < T90` and `99% < T99`. ([SRE workbook](https://sre.google/workbook/implementing-slos/), [Google CRE](https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/))
3. **Ceiling**: for every operation, `T99 <= 100 ms`, measured at the service. 100 ms is the "feels instantaneous" limit, and the service is only one hop of a user's request, so it must fit inside that limit. ([Nielsen](https://www.nngroup.com/videos/3-response-time-limits-interaction-design/), [RAIL](https://developer.mozilla.org/en-US/docs/Glossary/RAIL), [Tail at Scale](https://research.google/pubs/pub40801/))
4. **Targets come from a baseline**: the first `/flow-verify widgets perf` run (the first resource in [docs/scope/scope.md](../scope/scope.md)) measures the operations, rounds the results to manageable numbers, and records them in `docs/reports/perf-baseline.md`. They then become the Accepted budgets, and later runs are gated against them. ([SRE workbook](https://sre.google/workbook/implementing-slos/): "current performance can be a good place to start")
5. **Cache gate (own proposal)**: with the cache on, the hot-key GET `T99` must be lower than the cache-off GET `T99` at the same rate. Otherwise the cache adds cost without benefit.
6. **Error rate**: 0 non-intentional errors in every run. A fast run with errors fails.
7. **Load model**: an open model at a fixed arrival rate, `oha -q <rate> --latency-correction`, with keep-alive on (decided A5: API clients are services with pooled connections). ([Gil Tene](https://www.scylladb.com/2021/04/22/on-coordinated-omission/), [k6 docs](https://grafana.com/docs/k6/latest/using-k6/scenarios/concepts/open-vs-closed/), [oha README](https://github.com/hatoo/oha))
8. **Load rate** (decided A4): there is no real traffic forecast, so the capacity search records a curve of rate against T99. The highest step that meets the ceiling with 0 errors is the **capacity**, which is reported as a result. The **operating rate** for baseline and gate runs is the **knee** of that curve: the highest rate before T99 starts rising sharply. The knee is located with the Kneedle method ([Satopaa et al., 2011](https://doi.org/10.1109/ICDCSW.2011.20)), so it is reproducible and not chosen by eye. The curve is attached to the report. (No universal safe utilization exists, and queueing delay grows as 1/(1 - utilization): [third-bit](https://third-bit.com/2026/06/01/nonlinearity/), [oneuptime](https://oneuptime.com/blog/post/2026-09-08-latency-slo-safe-utilization/view), [metricgate](https://metricgate.com/docs/queuing-mm1/))
9. **Environment**: local docker compose with a release build. Record the CPU, RAM, OS, and `redis-cli --intrinsic-latency` output in every report, so results are only compared within the same environment. ([Redis docs](https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/latency/))

**What the references say**
| Topic | Guidance | Source |
|-------|----------|--------|
| SLO shape | An SLO is a target on an SLI (`SLI <= target`). Start from what users care about | [Google SRE book, ch. 4](https://sre.google/sre-book/service-level-objectives/) |
| Latency SLI format | Express it as "percentage of requests with latency < X ms", not as "p99 < X ms". Counting requests under a threshold is easy, while aggregating percentiles is hard | [Google CRE, "Building good SLOs"](https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/) |
| Multiple grades | Use two thresholds, for example "90% < 100 ms and 99% < 400 ms". One threshold hides the tail | [SRE workbook, ch. 2](https://sre.google/workbook/implementing-slos/); [Google CRE](https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/) |
| Avoid the mean | Averages hide outliers | [Google CRE](https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/) |
| Initial target | With no other information, measure current performance, then round it to a manageable number (the workbook rounds 432 ms up to 450 ms) | [SRE workbook, ch. 2](https://sre.google/workbook/implementing-slos/) |
| Where to measure | As close to the client as possible | [Google CRE](https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/) |
| User perception ceiling | 0.1 s feels instantaneous, 1 s keeps the flow of thought | [Nielsen](https://www.nngroup.com/videos/3-response-time-limits-interaction-design/); [Google RAIL (100 ms response)](https://developer.mozilla.org/en-US/docs/Glossary/RAIL) |
| Tail matters at scale | Responses within about 100 ms feel fluid, and rare slowdowns dominate as fan-out grows | [Dean & Barroso, "The Tail at Scale"](https://research.google/pubs/pub40801/) |
| Load model | A closed model (fixed connections) throttles itself when the server slows and hides the tail (coordinated omission). Use an open model at a fixed arrival rate | [Gil Tene (via ScyllaDB)](https://www.scylladb.com/2021/04/22/on-coordinated-omission/); [k6 docs, "Open and closed models"](https://grafana.com/docs/k6/latest/using-k6/scenarios/concepts/open-vs-closed/) |
| Tooling | `oha -q <rate> --latency-correction` corrects for coordinated omission (the flag only works together with `-q`) | [oha README](https://github.com/hatoo/oha) |
| Redis floor | Redis processing is sub-microsecond. The environment (kernel, hypervisor, docker) sets the floor, which you measure with `redis-cli --intrinsic-latency` | [Redis docs, "Diagnosing latency issues"](https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/latency/) |

[`docs/assignment/deferred-skills/flow-verify/perf.md`](../assignment/deferred-skills/flow-verify/perf.md) implements this method.



### Operational limits (2026-10-10: all decided except the rate-limit value, which waits on the team)
**Deferred:** each limit needs a measured baseline or a business number that the MVP does not have. **MVP uses:** the library defaults and cited values in "MVP limits" above.

Three of these are in force today: N1 (cache TTL 300 s), N6 (JWT leeway 60 s), and N7 (page size 10 and 100). Their values are in the "MVP limits" table above; the rows below keep the research behind them.

Every limit must come from a reference or from measurement, like the performance budgets.

| ID | Limit | Where | What the references say | Decision |
|----|-------|-------|-------------------------|---------------------|
| N1 | Cache TTL | [docs/conventions/cache.md](cache.md) AC-5 | Pick the TTL from the data's rate of change and the risk of serving stale data. Even seconds or minutes help. ([AWS caching whitepaper](https://docs.aws.amazon.com/whitepapers/latest/database-caching-strategies-using-redis/cache-validity.html)) | We DEL on every write, so the TTL only bounds staleness when invalidation fails or races. TTL = the longest stale window the business accepts for that resource, asked per resource in `flow-spec`. **Decided:** default 300 s (5 min), and `products` uses the default |
| N2 | TTL jitter | [docs/conventions/cache.md](cache.md) AC-5 | AWS: add "slight jitter" to avoid mass expiry, with no amount given. Practitioner write-ups use +-10 to 20% ([blogs, low authority](https://vetoralabs.com/system-design/concepts/caching/ttl-strategies)). Jitter cannot fix a single hot key; XFetch ([Vattani et al., VLDB 2015](https://doi.org/10.14778/2757807.2757813), beta = 1.0) is the peer-reviewed fix for that | **Decided:** one-sided jitter, uniform 0 to 10% of TTL. This is the low end of the practitioner range, because lazy loading already spreads expiry. XFetch is deferred |
| N3 | Cache call timeout | [docs/conventions/cache.md](cache.md) AC-4 | Choose an acceptable false-timeout rate (e.g. 0.1%), then use the dependency's matching latency percentile (p99.9) as the timeout. ([Amazon Builders' Library](https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/)) | Timeout = Redis GET p99.9 measured from the app during the perf baseline, rounded up. **Decided** |
| N4 | Request timeout, body limit | [docs/conventions/api.md](api.md) | Timeout: same [Builders' Library](https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/) method. Body: "define and enforce a maximum size of data on all incoming parameters and payloads". ([OWASP API4:2023](https://owasp.org/API-Security/editions/2023/en/0xa4-unrestricted-resource-consumption/)) [axum's own default body limit is 2 MiB](https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html). | Timeout = slowest operation's p99.9 from the baseline, rounded up. Body: keep the axum 2 MiB default as a global ceiling, plus a per-route limit computed from the DTO's maximum field sizes. Per-route limit = the sum of the DTO's maximum string lengths × 6 bytes (the worst case for a JSON `\uXXXX` escape, [RFC 8259 §7](https://www.rfc-editor.org/rfc/rfc8259#section-7)), plus overhead for keys and numbers, rounded up to a power of two (own proposal). The same limit applies to that resource's Kafka messages. **Decided** |
| N5 | Idempotency key retention | A1 | [Stripe prunes keys](https://docs.stripe.com/api/idempotent_requests) once they are "at least 24 hours old". The [IETF Idempotency-Key draft](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/) has expired and gives no number we could verify | **Decided:** 24 hours, [citing Stripe](https://docs.stripe.com/api/idempotent_requests) |
| N6 | JWT clock leeway | [docs/conventions/security.md](security.md) | [RFC 7519](https://www.rfc-editor.org/rfc/rfc7519): "a small leeway, usually no more than a few minutes". [`jsonwebtoken` defaults to 60 s](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/struct.Validation.html). [Keycloak advises](https://skycloak.io/blog/keycloak-clock-skew-token-not-active-fix/) seconds, not minutes, with NTP as the real fix | 60 s (the library default, within [RFC 7519](https://www.rfc-editor.org/rfc/rfc7519)). **Decided** |
| N7 | Page size default and maximum | AC-4 | [OWASP API4](https://owasp.org/API-Security/editions/2023/en/0xa4-unrestricted-resource-consumption/): validate the parameter that controls the number of records returned. [Stripe: `limit`](https://docs.stripe.com/api/pagination) defaults to 10 and ranges from 1 to 100 | **Decided:** default 10, maximum 100, [following Stripe](https://docs.stripe.com/api/pagination) |
| N8 | Rate limit, Kafka retry budget | [docs/conventions/kafka.md](kafka.md) AC-4, [docs/conventions/security.md](security.md) SEC-5 | [OWASP API4](https://owasp.org/API-Security/editions/2023/en/0xa4-unrestricted-resource-consumption/): rate limits "fine tuned based on the business needs", keyed by client identity, not IP. [Builders' Library](https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/): capped exponential backoff with jitter, with no fixed count. Kafka evicts a consumer that does not poll within [`max.poll.interval.ms` (default 300 s)](https://kafka.apache.org/documentation/#consumerconfigs) | **Rate limit:** per token `sub`, value pending the team (open question P4 below). The mechanism ships with the value as config. **Kafka retry (decided):** capped exponential backoff with full jitter, bounded by total elapsed time, not a count. Budget = `max.poll.interval.ms` minus (`max.poll.records` x the N4 request timeout). The initial delay is the N3 timeout. Once the budget is exhausted, the message goes to the DLQ |


## References
- AWS whitepaper, "Database caching strategies using Redis: cache validity": https://docs.aws.amazon.com/whitepapers/latest/database-caching-strategies-using-redis/cache-validity.html
- Amazon Builders' Library, "Timeouts, retries, and backoff with jitter": https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/
- OWASP API4:2023 Unrestricted Resource Consumption: https://owasp.org/API-Security/editions/2023/en/0xa4-unrestricted-resource-consumption/
- Vattani, Chierichetti, Lowenstein, "Optimal Probabilistic Cache Stampede Prevention", PVLDB 8(8), 2015
- Apache Kafka consumer configs (`max.poll.interval.ms`): https://kafka.apache.org/documentation/#consumerconfigs
- Stripe, pagination: https://docs.stripe.com/api/pagination
- Stripe, idempotent requests: https://docs.stripe.com/api/idempotent_requests
- RFC 7519, JSON Web Token: https://www.rfc-editor.org/rfc/rfc7519
- axum `DefaultBodyLimit`: https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html
- Google SRE book, ch. 4 "Service Level Objectives": https://sre.google/sre-book/service-level-objectives/
- Google SRE workbook, ch. 2 "Implementing SLOs": https://sre.google/workbook/implementing-slos/
- Google CRE, "Building good SLOs": https://cloud.google.com/blog/products/gcp/building-good-slos-cre-life-lessons/
- Nielsen, response time limits: https://www.nngroup.com/videos/3-response-time-limits-interaction-design/
- RAIL model (MDN): https://developer.mozilla.org/en-US/docs/Glossary/RAIL
- Dean & Barroso, "The Tail at Scale": https://research.google/pubs/pub40801/
- Satopaa et al., "Finding a 'Kneedle' in a Haystack: Detecting Knee Points in System Behavior" (ICDCS Workshops, 2011)
- third-bit, "Queue nonlinearity": https://third-bit.com/2026/06/01/nonlinearity/
- oneuptime, "Turn a latency SLO into a maximum safe utilization target": https://oneuptime.com/blog/post/2026-09-08-latency-slo-safe-utilization/view
- metricgate, M/M/1 queue: https://metricgate.com/docs/queuing-mm1/
- ScyllaDB, "On coordinated omission": https://www.scylladb.com/2021/04/22/on-coordinated-omission/
- k6, open vs closed models: https://grafana.com/docs/k6/latest/using-k6/scenarios/concepts/open-vs-closed/
- oha README: https://github.com/hatoo/oha
- Redis, diagnosing latency: https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/latency/
