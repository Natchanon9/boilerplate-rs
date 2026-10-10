# Review checklist (rule IDs)

## Architecture
- **ARCH-1** Dependency rule: no inner crate imports an outer crate or framework (see `layers.md`).
- **ARCH-2** Handlers and consumers are thin: extract, map, call the use case, map. No business rules and no SQL.
- **ARCH-3** No entity or row is serialized directly to JSON or a Kafka payload. DTOs are explicit.
- **ARCH-4** Ports are defined in `application` and use domain or application types only.
- **ARCH-5** Generic building blocks are reused, not copied into a resource module.
- **ARCH-6** Invariants are enforced in the domain, and mirrored by DB constraints where they can be expressed.
- **ARCH-7** Code matches the spec. Any behavior not in the spec is flagged as an undocumented decision.

## Security (from [docs/conventions/security.md](../../../docs/conventions/security.md))
- **SEC-1** JWT validation: HS256 is pinned, the secret is at least 32 bytes, and exp, iss, and aud are checked.
- **SEC-2** Every route declares a scope.
- **SEC-3** Owned resources filter by owner in every query.
- **SEC-4** `deny_unknown_fields` on requests, and explicit response fields.
- **SEC-5** Limits: body size and page size.
- **SEC-6** No internal detail in errors. Debug features are off in prod config.
- **SEC-7** Kafka input is validated like HTTP input.
- **SEC-8** SQL is built only through sqlx macros or bound parameters.
- **SEC-9** Secrets use secret types. No secret or token is logged.
- **SEC-10** New dependencies pass `cargo deny`, and each is justified.
- **SEC-11** No `unsafe`, and no `unwrap`, `expect`, or `panic!` outside tests.

## API conventions (from [docs/conventions/api.md](../../../docs/conventions/api.md))
- **API-1** Path and naming conventions are followed.
- **API-2** Status codes: 201 with Location, 200 with body on PUT, 404, 422 (with field codes), 400 only for invalid JSON, 413.
- **API-3** problem+json on every error path.
- **API-4** Keyset pagination with the limit enforced. `limit` and `cursor` are the only list parameters.

## Performance
- **PERF-1** An unbounded query, or a missing `LIMIT`.
- **PERF-2** N+1: a query inside a loop.
- **PERF-3** A query with no supporting index in the migration.
- **PERF-4** A blocking call in async code (sync IO, heavy CPU without `spawn_blocking`).
- **PERF-5** A write path that does not invalidate the cache, or a cached read with no TTL.
- **PERF-6** A DB connection or transaction held across an external IO await.
- **PERF-7** Needless clones or allocations on a hot path (a minor finding unless measured).

## Documentation
- **DOC-1** Every number, limit, default, or standards claim added to a spec, config default, or code comment has an inline source link, or is labeled `measured` or `own proposal`.

## Tests
- **TEST-1** Every AC has a test named `ac_<n>_*`, and it passes.
- **TEST-2** Failure paths are tested, not just the happy path.
- **TEST-3** Tests are deterministic: no sleep-based ordering, and every await has a timeout.
- **TEST-4** Domain and application tests use fakes, not containers.

## Deferred
- **API-5** ETag and If-Match on get and update: deferred with optimistic locking.
- **PERF-8** An over-budget perf result: deferred with the perf method.
- The rate-limit and timeout parts of **SEC-5**: deferred with rate limiting.

See the `## Deferred` sections in `docs/conventions/` for the reasons.
