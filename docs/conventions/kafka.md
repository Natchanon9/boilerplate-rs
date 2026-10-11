# Kafka as an alternative inbound transport

**Date**: 2026-10-09 (cut to MVP on 2026-10-10)
**Status**: Accepted
**Kind**: capability
**Built in**: [assignment feature 7](../assignment/scope.md)

## Summary

Besides HTTP, the service accepts create, update, and delete commands from a Kafka topic, and both paths call the same use cases. Duplicate messages never duplicate data, and bad messages go to a DLQ without stalling the consumer. Replies, events, and retries are designed but deferred (see [Deferred](#deferred)).

## Context: why Kafka

| | HTTP | Kafka | RabbitMQ | gRPC |
|---|---|---|---|---|
| Coupling | Sync | Async, decoupled | Async | Sync |
| Replay / audit | No | Yes (retention) | No | No |
| Ordering | None | Per partition key | Per queue | None |
| Best for | Client-facing APIs | Service-to-service, event-driven, bulk | Task queues / RPC | Internal RPC |

The proposal: HTTP for external clients, and Kafka for internal services and event-driven integration. The interviewers also named Kafka specifically.

### Why idempotency is mandatory
Kafka delivery is at-least-once. Duplicates are part of the design, not a bug:
- The consumer commits the DB write, then crashes or is rebalanced before committing the offset. The message is redelivered.
- An upstream service crashes and re-sends. That produces a new message at a new offset, so the idempotent producer ([`enable.idempotence`](https://kafka.apache.org/documentation/#producerconfigs_enable.idempotence)) cannot deduplicate it.
- Kafka's exactly-once semantics only cover Kafka-to-Kafka ([Kafka docs: message delivery semantics](https://kafka.apache.org/documentation/#semantics)). Our side effect is in Postgres, outside the Kafka transaction.

So every command handler must produce the same final state no matter how many times it runs.

## Requirements

- **AC-1**: Producing `<res>.create` to `<res_plural>.commands.v1` results in a DB row.
- **AC-2**: Re-sending the same command does not duplicate or re-apply it.

  AC-1 and AC-2 are proven by the integration tests of the first resource that declares commands. The example resource, `tracks`, declares none (engineer decision, 2026-10-12), so today only the boilerplate's own Kafka test runs: it uses a dummy handler, so it covers receiving, the DLQ, and ordering (AC-3, AC-5) but writes no row.
- **AC-3**: Malformed or invalid messages go straight to the DLQ with an `error.reason` header, and the consumer keeps going.
- **AC-5**: Commands for the same entity are processed in order (partition key = entity id).
- **AC-7**: Topics, messages, and consumer lag are visible in Kafka UI in docker compose.

AC-4 and AC-6 are deferred.

## Decision

### Topics (per resource, named after the resource, so this works for any resource)

| Topic | Direction | Key |
|-------|-----------|-----|
| `<res_plural>.commands.v1` | in | entity id |
| `<res_plural>.commands.v1.dlq` | out | original key |

The topic a handler declares is authoritative. `<res_plural>.commands.v1` is the standard form, where `<res_plural>` is the plural the resource uses in its HTTP path. The consumer subscribes to the registered handlers' topics only, and the compose `kafka-init` step must create the same names.

### Envelope

```json
{
  "message_id": "uuid",
  "correlation_id": "uuid",
  "type": "widget.update",
  "schema_version": 1,
  "occurred_at": "2026-10-09T10:00:00Z",
  "payload": {}
}
```

The envelope is strict (engineer decision, 2026-10-10). Every field is required:

| Field | Rule |
|-------|------|
| `message_id` | a string holding a UUID |
| `correlation_id` | a string holding a UUID |
| `type` | a non-empty string, `<res>.<op>` (`create`, `update`, or `delete`) |
| `schema_version` | the number `1`, matching the `v1` in the topic name (own proposal) |
| `occurred_at` | a string holding an [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339#section-5.6) date-time |
| `payload` | a JSON object whose `id` is a string |

The message key is required and must equal `payload.id` (AC-5 depends on it).

### Payloads (decided 2026-10-10)
- `<res>.create`: the HTTP POST body plus `id`.
- `<res>.update`: the HTTP PUT body plus `id`.
- `<res>.delete`: `{ "id": "<uuid>" }`.
- A Kafka handler declares its payload as a `#[serde(default)]` struct of `application::input::Field<T>` fields with `#[serde(flatten)] unknown: UnknownFields`, the same shape as the HTTP request DTO plus `id`, and passes it to the same application parser as HTTP. The HTTP DTO types themselves live in `http-api`, which `kafka-api` cannot depend on (`deny.toml`), so the shared parts are `Field<T>`, `UnknownFields`, and the parser, all in `application`.

### Idempotent by design
The MVP accepts only absolute CRUD commands, so every command is idempotent without an inbox table:
- **create**: the **producer supplies `payload.id`** (UUIDv7, required; the parser rejects any other version, nil included, with `invalid_format`, so the message goes to the DLQ as `validation`), and the insert uses `ON CONFLICT (id) DO NOTHING`. A duplicate is a no-op.
- **update**: a full replacement by id, where the last write wins (no version check in the MVP). Applying the same update twice gives the same state.
- **delete**: not found is treated as success.
- Relative (delta) commands such as "increase stock by 5" cannot be made idempotent this way. They need an inbox table in the same transaction as the write, which means a Unit of Work. That is deferred, together with the outbox.

### Consumer semantics
- At-least-once. Auto-commit is off (`enable.auto.commit=false`, [librdkafka CONFIGURATION.md](https://github.com/confluentinc/librdkafka/blob/v2.12.1/CONFIGURATION.md)). The offset is committed after successful processing, or after the DLQ write succeeds.
- If the DLQ write fails, the consumer logs an error, stops, and the process exits non-zero. The uncommitted message is read again on restart. Compose runs the `app` service with `restart: unless-stopped`, so the restart is automatic locally ([Docker restart policies](https://docs.docker.com/engine/containers/start-containers-automatically/#use-a-restart-policy)).
- `auto.offset.reset=earliest`. Without it a new consumer group starts at the end of the topic, because the default is `largest`, the same as `latest` ([librdkafka CONFIGURATION.md](https://github.com/confluentinc/librdkafka/blob/v2.12.1/CONFIGURATION.md), `auto.offset.reset`), and commands produced before the first start would be skipped.
- One receive loop processes one message at a time, so commands for one key (and so one partition) are handled in the order they were produced (AC-5). Parallelism across partitions is deferred ([Deferred](#cross-partition-parallelism)).
- On shutdown the loop finishes the message in flight, commits it, and stops.
- Payloads are parsed by the same application parser as HTTP input ([api.md](api.md), input error rules).
- **Every failure goes to the DLQ** with an `error.reason` header, and the consumer moves on: an unparsable message, a validation failure, an update of a missing entity (`not_found`), a create whose unique key is taken by another id (`conflict`), and any infrastructure error. There is no retry in the MVP. The DLQ is a topic, so its messages can be replayed once the cause is fixed (own proposal: simplest behaviour that never stalls the consumer or loses a command).

### DLQ messages (decided 2026-10-10)
The DLQ message keeps the original key, value, and headers, and adds two headers:
- `error.reason`: one code from the table below.
- `error.detail`: for `validation`, the JSON array of field errors (`[{"field": "...", "code": "...", "message": "..."}]`, the same fields as the HTTP `errors[]` in [api.md](api.md)); for every other reason, the short fixed sentence below. It never carries an internal cause: the consumer logs the cause instead (message id, topic, partition, offset, and reason, never the payload, [security.md](security.md)).

| `error.reason` | When | `error.detail` |
|----------------|------|----------------|
| `malformed_json` | the value is not valid JSON | the message value is not valid JSON |
| `invalid_envelope` | a field is missing or invalid, the key is missing, or the key differs from `payload.id` | the envelope is invalid |
| `unknown_type` | no handler is registered for the type, or the type is owned by another topic | no handler accepts this type on this topic |
| `validation` | `AppError::Validation` | the field errors |
| `not_found` | `AppError::NotFound` | the entity was not found |
| `conflict` | `AppError::Conflict` | the command conflicts with the current state |
| `unavailable` | `AppError::Unavailable` | a dependency was unavailable |
| `internal` | `AppError::Internal`, `Unauthorized`, or `Forbidden` | an internal error occurred |

The envelope is checked before the type, so a message with a bad key and an unknown type is `invalid_envelope`.

### Topic setup
- Local docker compose creates the topics in an init step: 3 partitions, replication factor 1, and the broker's default retention. These are own proposals: 3 partitions show that per-key ordering (AC-5) holds when keys spread over partitions, and RF 1 matches the single local broker. Production values belong to the platform.
- The init step's `TOPICS` list in `docker-compose.yml` is empty until a resource declares commands, so a fresh clone creates no topic, and `cargo xtask kafka-produce` has nothing to send to. This is intended: the resource build adds `<res_plural>.commands.v1` and its `.dlq` to `TOPICS`.
- The consumer group id is the `APP_NAME` config value: one group per service (own proposal).
- The consumer runs only when `ENABLE_FEATURES` contains `kafka` ([runtime.md](runtime.md)); it is off by default. A consumer with no registered handler has nothing to subscribe to: `consume` exits non-zero with a message, and `all` runs the HTTP routers only and logs a warning.
- The maximum message size is the broker default ([api.md](api.md), MVP limits).

### Security
- PLAINTEXT locally. Kafka input is validated like HTTP input ([security.md](security.md) SEC-7).

### Dev producer (decided 2026-10-10)
`cargo xtask kafka-produce --type <res>.<op> --payload '<json>' [--topic <name>] [--repeat N]` builds the envelope and sends it (reads `KAFKA_BROKERS` from the environment or `.env`, like `xtask token`):
- It generates `message_id`, `correlation_id` (UUIDv7), and `occurred_at` (now, RFC 3339), and sets `schema_version` to `1`. For `<res>.create` without `payload.id` it adds a UUIDv7.
- The key is `payload.id`. The default topic is `<res>s.commands.v1`: the resource name plus `s` (own proposal; it fits `widget` and does not fit irregular plurals).
- `--topic <name>` replaces the default topic, for an irregular plural such as `categories.commands.v1`. The topic the handler declares is authoritative (see Topics), so pass it whenever the default differs.
- It prints the `message_id` and the key. `--repeat N` sends the identical message N times, which demonstrates AC-2.
- `--raw '<text>' --key <k> --type <t>` sends the text as the value, unchecked, to the topic of `<t>` (or to `--topic`), so the DLQ can be demonstrated with a bad message.

## Build plan
1. Envelope + serde + parsing with the application parser (AC-3)
2. Consumer loop: manual commit, DLQ producer with `error.reason` (AC-3, AC-5)
3. Dispatcher from `type` to the walking-skeleton resource's use cases (AC-1)
4. Repository support for `ON CONFLICT DO NOTHING` (AC-2)
5. Integration tests with testcontainers Kafka (AC-1, AC-2, AC-3, AC-5), and kafka-ui plus the topic init step in compose (AC-7)
6. Dev helper `cargo xtask kafka-produce` for the demo

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| | None open | |

Closed:

| # | Question | Decision |
|---|----------|----------|
| K7 | Should the parser reject envelope fields it does not know? | No. Unknown envelope fields are ignored (the code already does this), so `reply_to` can be added later without breaking the consumer |
| K8 | The dev producer's plural rule (`<res>s`) is wrong for irregular plurals such as `categories` | The topic a handler declares is authoritative. `kafka-produce` has `--topic <name>` to override its default for irregular plurals |

## Deferred

Designed and decided, then cut on 2026-10-10 to keep the MVP small. The original text is kept.

### Removing Kafka from the whole project
**Deferred** on 2026-10-10 (engineer decision): a clean boilerplate should not make a project carry Kafka it never chose. The MVP keeps the Kafka code in the repository, switched off by default, because the alternative transport is one of this boilerplate's deliverables. Two ways to go further, in order of cost:
1. **Cargo feature** `kafka` (off by default). Without it, `kafka-api`, `rdkafka`, and the build-time needs of `rdkafka` (cmake, libcurl headers) are not compiled. CI must then build and test both with and without the feature.
2. **A generator** (a `cargo-generate` template or a setup skill) that asks at project creation and deletes the crate, its wiring, `xtask kafka-produce`, and its compose services.

**MVP uses:** the code is present but inert. Kafka is off unless `ENABLE_FEATURES` contains `kafka` ([runtime.md](runtime.md)). While it is off, the app needs no `KAFKA_BROKERS`, `all` runs the HTTP routers only, and `make up` does not start `kafka`, `kafka-init`, or `kafka-ui` (compose profile `kafka`). Per resource, `/flow-spec` asks for the messaging commands, and [layers.md](../../.agents/skills/flow-build/layers.md) builds a handler only when the spec declares commands.


### Cross-partition parallelism
**Deferred:** a single loop keeps per-key order with no rebalance handling; parallelism is not needed for the demo. **MVP uses:** one receive loop that processes one message at a time, across all partitions.

### Replies (request/reply), and K2, K5, K6
**Deferred:** the brief asks for an alternative transport, and one-way commands demonstrate it. Replies add a second producer, a reply contract, and correlation handling. **MVP uses:** one-way commands; the result is visible in the database, the API, and Kafka UI.

#### Replies (added 2026-10-10, independent spec check G1)
- A reply goes to `reply_to` with the same `correlation_id`. Its `type` is the command type plus `.reply`, for example `widget.create.reply`.
- The payload is `{ "status": "ok" | "error", "code": null | "<code>", "data": <snapshot> | null }`.
- On success: create, update, and get return the snapshot in `data`. Delete returns `data: null`.
- Error codes are the `AppError` variants in snake_case: `not_found`, `conflict`, `version_mismatch`.
- No reply is sent in two cases, and the message goes to the DLQ instead (own proposal following AC-3 and AC-4; see K6):
  - a message that fails validation
  - a message whose retry budget ran out


| # | Question | Decision (at the time) |
|---|----------|------------------------|
| K2 | Request/reply, or events only? | Both. The reply is optional, driven by `reply_to`, so the demo can show both styles |
| K5 | On a duplicate command, re-send the reply and re-emit the event? | Re-send the reply, so the requester always gets an answer. Do not re-emit the event, because the state did not change |
| K6 | Should a message that fails validation also get an error reply when `reply_to` is set (code `validation`, with field errors), in addition to the DLQ? | Yes: otherwise a requester waits for its timeout without learning why. This is currently "no" (Replies section), pending this decision |

### Events topic
**Deferred:** no consumer of events exists in the demo, and events bring the dual-write problem with them. **MVP uses:** no events.

#### Events (G3)
- An event payload is the resource snapshot, in the same fields and shape as the HTTP response ([api.md](api.md), response shape).
- `<res>.deleted` carries `id` and the last stored `version`. The repository's delete uses `RETURNING version`.
- Both HTTP and Kafka writes publish, through the use case's `EventPublisher` port.
- If the publish fails after the commit, the service logs it and increments `events_publish_failures_total`, and the write still succeeds. This is the dual-write loss accepted above.
- The repository's insert reports `Inserted` or `AlreadyExists(entity)`. A duplicate create therefore replies with the current state and emits no event (K5). Deleting a missing row, and replaying an update to an equal state, also succeed without an event.

#### One producer adapter for everything the app publishes
**Deferred** with Events and Replies. When the app gets a producer, build one Kafka producer adapter in `infrastructure` behind a port in `application` (the `EventPublisher` above), and let every outbound publish share it instead of creating its own. That covers events, replies, and an HTTP handler that accepts a request, publishes a command, and lets the consumer process it later. The DLQ writer in `kafka-api` stays separate, because its only caller is the consumer and it handles messages the consumer rejected. The accept-then-process-later case also needs a way to tell the caller the result, such as a status resource or a reply; that is a per-resource decision for `/flow-spec`, not a boilerplate default.


### Retry with backoff (AC-4, K4)
**Deferred:** a retry budget needs measured timeouts ([api.md](api.md), N8). **MVP uses:** failures go straight to the DLQ, and the DLQ can be replayed.

- **AC-4**: Transient errors (for example, the DB is down) are retried with capped exponential backoff and jitter until the retry time budget ([docs/conventions/api.md](api.md) N8) is used up, then sent to the DLQ.

| # | Question | Decision (at the time) |
|---|----------|------------------------|
| K4 | In-process retry with backoff, or retry topics? | In-process, because it is simpler. Bounded by total elapsed time, not a count ([docs/conventions/api.md](api.md) N8). Retry topics are deferred |

### `expected_version` on update, and get over Kafka (AC-6)
**Deferred:** optimistic locking is deferred for HTTP too ([api.md](api.md), Deferred), and reading over Kafka needs replies. **MVP uses:** last-write-wins updates; reads over HTTP.

- **AC-6**: `<res>.get` over Kafka uses the same cache path as HTTP.

Original idempotency, topic, envelope, and consumer text:
#### Topics (per resource, named after the resource, so this works for any resource)

| Topic | Direction | Key |
|-------|-----------|-----|
| `<res_plural>.commands.v1` | in | entity id |
| `<res_plural>.events.v1` | out | entity id |
| `<res_plural>.replies.v1` | out (when `reply_to` is set) | correlation_id |
| `<res_plural>.commands.v1.dlq` | out | original key |

#### Envelope

```json
{
  "message_id": "uuid",
  "correlation_id": "uuid",
  "type": "widget.update",
  "schema_version": 1,
  "occurred_at": "2026-10-09T10:00:00Z",
  "reply_to": "widgets.replies.v1",
  "payload": {}
}
```
Kafka headers carry `traceparent` and `x-request-id`.

#### Idempotent by design (decided 2026-10-09)
v1 accepts only absolute CRUD commands, so every command can be idempotent without an inbox table:
- **create**: the **producer supplies `payload.id`** (UUIDv7, required), and the insert uses `ON CONFLICT (id) DO NOTHING`. On a duplicate, reply with the current state.
- **update**: a full replacement that requires `expected_version`. On a duplicate the version no longer matches. If the current state equals the requested state, treat it as success. Otherwise reply with a conflict.
- **delete**: not found is treated as success.
- Relative (delta) commands such as "increase stock by 5" cannot be made idempotent this way. They need an inbox table in the same transaction as the write, which means a Unit of Work. That is deferred, together with the outbox.

#### Consumer semantics
- At-least-once. Auto-commit is off. The offset is committed after successful processing, or after the DLQ write succeeds.
- Messages within one partition are processed sequentially. Different partitions are processed in parallel.
- Payloads are parsed by the same application parser as HTTP input ([api.md](api.md), input error rules).
- An event is the entity snapshot plus its version (event-carried state transfer). Downstream consumers use the version to ignore stale or duplicate events.
- Events are published after the commit. If the process dies between the commit and the publish, the event is lost. This dual-write problem is accepted in v1, and the outbox fixes it later (deferred).


### SASL, ACLs, and request-id headers
**Deferred:** production hardening, and request-id propagation is deferred with request ids in [api.md](api.md). **MVP uses:** PLAINTEXT on the local broker.

#### Security
- PLAINTEXT locally. Config supports `SASL_SSL` (SCRAM) for prod.
- Authorization is service-to-service, through broker ACLs: one principal per producing service, and per-topic ACLs (documented in the README). No user tokens travel inside messages.
- The consumer's maximum message size matches the HTTP body limit.

#### Topic setup (G11)
- Local docker compose creates the topics in an init step: 3 partitions, replication factor 1, and the broker's default retention. These are own proposals: 3 partitions show per-key ordering (AC-5) and cross-partition parallelism, and RF 1 matches the single local broker. Production values belong to the platform.
- The consumer group id is the `APP_NAME` config value: one group per service (own proposal).
- The maximum message size per resource is the resource's per-route body limit ([api.md](api.md) N4).


Original first acceptance criterion:
- **AC-1**: Producing `<res>.create` to `<res_plural>.commands.v1` results in a DB row, a `<res>.created` event on `<res_plural>.events.v1`, and, when `reply_to` is set, a reply with the matching `correlation_id`.

Original build plan:
1. Envelope + serde + validation (AC-3)
2. Consumer loop: manual commit, retry/backoff, DLQ producer (AC-3, AC-4, AC-5)
3. Dispatcher from `type` to the walking-skeleton resource's use cases (AC-1, AC-6)
4. Repository support for `ON CONFLICT DO NOTHING` and `expected_version` (AC-2)
5. Event publisher (port in application, implementation in infrastructure) (AC-1)
6. Reply producer (AC-1)
7. Integration tests with testcontainers Kafka (AC-1 to AC-5), and kafka-ui in compose (AC-7)
8. Dev helper `cargo xtask kafka-produce` for the demo
