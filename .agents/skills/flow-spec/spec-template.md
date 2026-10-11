# Spec template

Path: `docs/specs/NNNN-kebab-slug.md`. Omit any section that does not apply. Never write `n/a`.

**Cite at the value:** every number, limit, default, or standards claim gets an inline link at the place it appears, e.g. `limit max 100 ([Stripe](https://docs.stripe.com/api/pagination))`. Otherwise label it `measured` (link the report) or `own proposal` (with a reason). Also collect the links under `## References`.

```markdown
# NNNN. <Title as a noun phrase>

**Date**: YYYY-MM-DD
**Status**: Proposed
**Kind**: resource
**Scope feature**: <N. name>
**Builds on**: <links to the files in docs/conventions/ that apply, e.g. [api.md](../conventions/api.md)>

## Summary
<2 to 4 plain sentences: what is being decided, why, and what it means for the build.>

## Context
<The problem and its forces. No options here.>

## Requirements
**User stories**
- As a <role>, I want <capability> so that <outcome>.

**Acceptance criteria**
- **AC-1**: <an observable, independently testable outcome>
- **AC-2**: <the key failure or edge case>

## Options considered
| Option | Pros | Cons |
<At most 4. Omit this section if there was no real alternative.>

## Decision
**Chosen**: <option> because <one sentence>.

## Resource design
**Fields**
| Field | Rust type | SQL type | Required | Constraints | Default | Source |
|-------|-----------|----------|----------|-------------|---------|--------|
| id | `Uuid` (v7) | `uuid` PK | yes | | | server, or client for Kafka create |
| version | `i64` | `bigint` | yes | >= 1 | 1 | server |
| created_at | `OffsetDateTime` | `timestamptz` | yes | | now() | server |
| updated_at | `OffsetDateTime` | `timestamptz` | yes | | now() | server |

**Invariants**: <rule, and where it is enforced (domain, DB, or both)>
**Relations**: <FK, and its on-delete action>
**Operations**: create | get | list | update | delete <list the enabled ones>
**List**: newest first, `limit` and `cursor` only (see api.md)
**Indexes**: <one index for each unique rule or declared query, with the reason>
**Authorization**: read `<res_plural>:read`, write `<res_plural>:write`, ownership `<rule or none>`
**Cache**: enabled | disabled, TTL <secs or inherit>
**Messaging**: Kafka commands accepted, from `<res>.create|update|delete`, or none

## Configuration
- `ENV_VAR`: purpose, default

## Test scenarios
- Happy path: <flow>. Verifies AC-1.
- Failure: <flow>. Verifies AC-2.
- Auth: <who is denied, and the status code>. Verifies AC-n.

## Build plan
<!-- Order is fixed by flow-build/layers.md. Every task cites ACs. -->
### 1. Schema
- [ ] <task> (AC-n)
### 2. Domain
### 3. Application
### 4. Infrastructure
### 5. Inbound adapters
### 6. Wiring
### 7. Integration tests
### 8. Gate: make verify

## Consequences
<Trade-offs accepted, and what becomes harder.>

## Open questions
| # | Question | Recommendation |

## References
- <links actually used>
```
