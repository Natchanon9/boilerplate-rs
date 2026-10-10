---
name: flow-scope
description: Plan the service scope in docs/scope/scope.md, add a CRUD resource to it, or report where the whole service stands and detect drift. Decides which resources to build and in what order, never how. Owns the feature and checkbox format that every flow skill uses.
---

# flow-scope

Owns `docs/scope/scope.md`, the service's coarse, living plan. It reads and writes only that file: no specs, no code, no `AGENTS.md`.

Every feature is a **CRUD resource**. The stack, transports, cache, and security are already decided in [docs/conventions/](../../../conventions/README.md), so they are never features. A request to change them is a hand edit to the conventions, not a scope change.
This file also defines the **feature shape**, which the other flow skills follow when they tick boxes.

## Modes (inferred from the input, there are no subcommands)

| Situation | Mode |
|-----------|------|
| No scope file exists, or the engineer is starting a new service | **plan**: write a new service scope |
| Scope exists, and the argument names one resource | **add**: append one resource |
| Scope exists, no argument | **status**: report on the whole service and detect drift |

### plan
1. Ask what the service is and who it serves, if that is not already given.
2. List its resources. The first one is the **walking skeleton**: the simplest real resource, wired through every layer. It becomes the reference implementation that later resources imitate, and its first perf run sets the baseline.
3. Order the remaining resources so that a resource comes after any resource it references.
4. Recommend a default tier and any per-resource tier (see Tiers below).
5. Write the file using the template below. Every new resource starts with `needs a decision` and a single `Spec it` box.

### add
Append one resource to `docs/scope/scope.md`, with the next free number. Ask at most one round of questions: intent, phase placement, and tier.

### status (whole service, read-only unless the engineer approves a fix)
Use this mode to answer "where are we, and what is safe to pick up next". It covers every resource in `docs/scope/scope.md`. `flow-sync` covers one resource after its change.
1. Read the git state (branch, ahead or behind its upstream), `docs/scope/scope.md`, the statuses in `docs/specs/`, and the latest reports in `docs/reports/`.
2. Report counts by status, plus the first unticked box of every `in-progress` feature.
3. Detect drift, and propose a fix for each case. Apply a fix only after the engineer approves it:
   - a spec or code exists with no scope row
   - a row is `done`, but its latest verify, perf, or review report has failures
   - a row is `in-progress`, but nothing has changed in a long time
   - an `Assumed` spec has not yet been ratified
   - a row's boxes do not match its tier (see the feature shape below)

## Feature shape (the format every flow skill follows)

```markdown
### N. <Feature> (<tags>)
<1 to 2 lines of intent>
**Done when:** <observable outcome; seeds the ACs>
- [ ] Spec it: /flow-spec <feature>
```

**Heading tags.** Use only:
- the tier, when it differs from the project default
- `needs a decision`
- `assumed decision (spec NNNN)`

Status is **not** a tag. It lives only in the `At a glance` table.

**Once the spec exists** (`flow-spec` writes this shape when it links the spec):

```markdown
- [x] Spec it: /flow-spec <feature>
- [ ] Build it: /flow-build <feature>
   - [ ] Schema + domain (AC-..)
   - [ ] Application (AC-..)
   - [ ] Infrastructure + adapters (AC-..)
   - [ ] Wiring + integration tests (AC-..)
- [ ] Verify it: /flow-verify <feature>
- [ ] Perf it: /flow-verify <feature> perf
- [ ] Review it: /flow-review <feature>
- [ ] Sync it: /flow-sync <feature>
spec NNNN, code <paths>
```

**Build milestones.** The four milestones map onto the eight build steps in `../flow-build/SKILL.md`:

| Milestone | Build steps |
|-----------|-------------|
| Schema + domain | 1 to 2 |
| Application | 3 |
| Infrastructure + adapters | 4 to 5 |
| Wiring + integration tests | 6 to 8 |

Keep all four milestones for a resource. A resource without Kafka commands still has HTTP adapters, so "Infrastructure + adapters" never disappears.

**Closing boxes by tier:**

| Tier | Verify it | Perf it | Review it | Sync it |
|------|-----------|---------|-----------|---------|
| `spike` | | | | yes |
| `standard` | yes | | | yes |
| `production` | yes | yes | yes | yes |

**Who ticks which box.** A skill ticks only its own boxes. It ticks a box only when the result passes **and** the engineer confirms.

| Box | Ticked by |
|-----|-----------|
| `Spec it` | `flow-spec` |
| `Build it` and its milestones | `flow-build` |
| `Verify it`, `Perf it` | `flow-verify` |
| `Review it` | `flow-review` (verdict "ready to merge") |
| `Sync it` | `flow-sync` |

A step the engineer skips is ticked and marked `(skipped: <reason>)`, never deleted.

**Pointer line.** Write `spec NNNN` after `Spec it` is ticked. Append `code <paths>` once `flow-build` has created code, for example `code crates/*/src/product/, migrations/*_products.sql`.

## Rules
- **Keep it coarse.** One feature is one resource. Build tasks belong in the spec's `## Build plan`, never here.
- **Never name tools or libraries** in intent or "Done when". Specs decide those. Command names in boxes and code paths in the pointer line are fine.
- **Edit in place.** Re-read the file immediately before writing. Make surgical edits only. Never delete a row; set it to `dropped` instead.
- **Statuses:** `planned`, `in-progress`, `done`, `dropped`, all in the `At a glance` table.
  - `flow-build` sets `in-progress`.
  - Only the engineer declares `done`, through `flow-sync`.

## Tiers (what is suggested after flow-build)
- `spike`: nothing
- `standard`: `/flow-verify <feature>`
- `production`: `/flow-verify <feature>`, then `/flow-verify <feature> perf`, then `/flow-review <feature>`

## Template

```markdown
# Scope: <service>

<1 to 2 sentences: what the service is and who it serves.>

**Build approach:** Tracer Bullet (a thin slice through every layer first, then thicken).
**Workflow tier (default):** <tier>

## At a glance
| # | Feature | Phase | Tier | Status |
|---|---------|-------|------|--------|

## <Phase name>

### N. <Feature> (<tags>)
<intent>
**Done when:** <observable outcome>
- [ ] Spec it: /flow-spec <feature>

## Deferred
- **<Resource>**: <one line>

## Open questions
| # | Question | Recommendation |
|---|----------|----------------|

## Legend
- **Feature status** (in `At a glance` only): `planned` > `in-progress` > `done`. `dropped` is kept for history, never deleted.
- **Spec status:** `Proposed` > `Accepted`. Also `Superseded`, and `Assumed` (built before deciding; ratify with `/flow-spec`).
- **Tier:** `spike` = build only; `standard` = build, verify; `production` = build, verify, perf, review. Every tier ends with `/flow-sync`.
- **needs a decision:** run `/flow-spec` before `/flow-build`.
- **Next step:** the first unticked box.
- Box format and box owners: `.agents/skills/flow-scope/SKILL.md` ("Feature shape").
```

## Report
End with: the mode you ran, counts by status, the single next step (the first unticked box), and any drift you found.
