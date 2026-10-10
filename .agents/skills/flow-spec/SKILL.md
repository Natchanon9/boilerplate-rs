---
name: flow-spec
description: Grill the engineer on one CRUD resource, write its build spec in docs/specs/NNNN-slug.md with IDed acceptance criteria and a layer-ordered build plan, and add its row to docs/scope/scope.md. Use before building a new resource, or when flow-build reports a missing decision. Conventions in docs/conventions/ are read-only.
---

# flow-spec

Owns `docs/specs/` and the resource rows in `docs/scope/scope.md`. It writes no code.

## Resource specs only
Every spec has `**Kind**: resource`: one CRUD resource (an entity plus its operations).

If the request would change the stack, a transport, or any convention (API rules, cache, Kafka, security), **stop**. Those live in [docs/conventions/](../../../docs/conventions/README.md), which is edited by hand and committed with a reason. Name the convention file that would change, and suggest the edit for the engineer to make.

## Process

### 1. Pre-flight
- Read `AGENTS.md`, the matching row in `docs/scope/scope.md` (if any), and every file in [docs/conventions/](../../../docs/conventions/README.md). These are the boilerplate-wide rules, and they are read-only. Never put anything from a `## Deferred` section into a spec.
- List existing specs in `docs/specs/`. Read the titles first, then the first 20 lines of any whose title overlaps this topic.
- If an overlap exists, ask whether this is a new spec, an update to the existing one, or a supersede.
- If the overlapping spec is `Assumed`, this is a **ratify** (see step 7).
- Next number = the highest number in `docs/specs/` + 1, zero-padded to 4 digits (the first spec is number 1). Only `docs/specs/` counts. Files ending in `-assumptions.md` share their parent spec's number, so they do not count. Re-check right before writing.

### 2. Sort every unknown
- **INFER** anything that existing specs, `AGENTS.md`, or code already settle. Do not ask about these. For example, pagination, the error format, and concurrency come from [docs/conventions/api.md](../../../docs/conventions/api.md).
- **ASK** only what the engineer alone knows: business rules, ownership, data volume, and compliance.
- **RECOMMEND** anything expertise settles: types, indexes, and TTLs. State your pick, a one-line reason, and the runner-up.

### 3. Grill in rounds
Ask up to 4 questions per round. Each question has 2 to 4 options, and exactly one is marked `(recommended)`. Use the agent's picker when it has one, otherwise plain text. Continue until the decision checklist is complete.

**Resource decision checklist**
- [ ] Fields: name, Rust type, SQL type, required or optional, constraints, default, **source** (client, server, or derived)
- [ ] Invariants and uniqueness, with where each is enforced (domain, DB, or both)
- [ ] Operations enabled (create, get, list, update, delete)
- [ ] Relations and referential actions (restrict or cascade)
- [ ] Delete semantics (hard, or soft with `deleted_at`)
- [ ] Authorization: scope per operation, plus an ownership rule if the resource is owned (SEC-3)
- [ ] Cache policy: on or off, TTL override ([docs/conventions/cache.md](../../../docs/conventions/cache.md))
- [ ] Messaging: which commands the Kafka consumer accepts ([docs/conventions/kafka.md](../../../docs/conventions/kafka.md))
- [ ] Scope row: a one-line intent and a "Done when" line

**Source check:** every value an AC needs must name its source. A value with no source is an open question, never a guess.

### 4. Write the spec
Read `spec-template.md` and fill it in.
- Set the status to `Proposed`.
- Group `## Build plan` by layer, in the fixed order from `../flow-build/layers.md`.
- Every task cites the AC IDs it satisfies.
- Every AC has at least one task and one test scenario.
- **Cite at the value:** put an inline link next to every number, limit, default, or standards claim, where it appears. Values without a source are labeled `measured` (link the report) or `own proposal` (with a reason), or they become open questions. Never invent a number silently.

### 5. Offer an independent check
Offer, and recommend for a first resource: a fresh session, or a different model, reads the spec and lists missing decisions only. Present each gap with a recommended fix. The engineer decides.

### 6. Add or update the scope row
Edit `docs/scope/scope.md`. If it does not exist, create it from the template below. Re-read the file right before writing, and make surgical edits only.
- **New resource:** append a row to `At a glance` with the next free number and status `planned`, and append a section in the "Scope row" shape below, with `Spec it` ticked.
- **Existing row:** tick `Spec it`, fill in the AC IDs of the milestones, and drop the `needs a decision` tag.

### 7. Ratify an assumption
When the topic has an `Assumed` spec or assumptions file:
1. Grill only the assumed decisions.
2. Write the real decision into the main spec. Update it in place, or create it if the assumption was a whole `Assumed` spec.
3. Set the assumed file's status to `Superseded by NNNN`. Never delete it.
4. Remove the `assumed decision (spec NNNN)` tag from the scope row.
5. If the decision differs from what was built, add the needed changes to the spec's Build plan, and suggest `/flow-build <feature>`.

## Scope row
Every resource in `docs/scope/scope.md` has exactly this shape:

```markdown
### N. <resource> (<tags>)
<1 to 2 lines of intent>
**Done when:** <observable outcome>
- [x] Spec it: /flow-spec <resource>
- [ ] Build it: /flow-build <resource>
   - [ ] Schema + domain (AC-..)
   - [ ] Application (AC-..)
   - [ ] Infrastructure + adapters (AC-..)
   - [ ] Wiring + integration tests (AC-..)
- [ ] Verify it: /flow-verify <resource>
- [ ] Review it: /flow-review <resource>
[spec NNNN](../specs/NNNN-<slug>.md)
```

- **Tags:** only `needs a decision` (no spec yet) and `assumed decision (spec NNNN)`. Status is not a tag. It lives only in the `At a glance` table.
- **Milestones** map onto the eight build steps: schema + domain = steps 1 to 2, application = 3, infrastructure + adapters = 4 to 5, wiring + integration tests = 6 to 8.
- **Who ticks which box:**

  | Box | Ticked by |
  |-----|-----------|
  | `Spec it` | `flow-spec` |
  | `Build it` and its milestones | `flow-build` |
  | `Verify it` | `flow-verify` |
  | `Review it` | `flow-review` |

  A box is ticked only when the result passes and the engineer confirms.
- **Status** (in `At a glance`): `planned` (`flow-spec`) > `in-progress` (`flow-build`) > `done` (`flow-review`, after the engineer accepts). `dropped` is kept for history, and rows are never deleted.

**Template for a new `docs/scope/scope.md`:**

```markdown
# Scope: <service>

<1 to 2 sentences: what the service is and who it serves.>

## At a glance
| # | Feature | Phase | Status |
|---|---------|-------|--------|

## <Phase name>

## Open questions
| # | Question | Recommendation |
|---|----------|----------------|

## Legend
- **Status** (in `At a glance` only): `planned` > `in-progress` > `done`. `dropped` is kept for history, never deleted.
- **Spec status:** `Proposed` > `Accepted`. Also `Superseded`, and `Assumed` (built before deciding; ratify with `/flow-spec`).
- **Flow:** `/flow-spec` > `/flow-build` > `/flow-verify` > `/flow-review`.
- **needs a decision:** run `/flow-spec` before `/flow-build`.
- **Next step:** the first unticked box.
```


## Never
- Write code, or choose a tool without telling the engineer.
- Create an `Assumed` spec. Only `flow-build` records assumptions. This skill ratifies them.
- Restate a convention from [docs/conventions/](../../../docs/conventions/README.md) inside a resource spec. Reference it by ID instead.

## Report
End with: the spec path, the AC count, the scope row you added or updated, any remaining open questions, and the next step `/flow-build <resource>`.
