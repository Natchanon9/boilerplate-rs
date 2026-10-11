# Workflow skills for multiple AI tools

**Date**: 2026-10-09
**Status**: Proposed
**Kind**: capability
**Built in**: [assignment feature 1](scope.md)
**Formerly**: spec 0006 (renamed when the docs were split on 2026-10-10; this is the only place the old number is kept)

## Summary

Instead of task-specific skills such as `add-crud-resource`, we ship four workflow skills that walk an AI through spec > build > verify > review. All state lives in files: the scope, specs, reports, and `AGENTS.md`. The skills are not tied to any one resource, so they work for every kind of CRUD resource. Strictness comes from checkable gates, not from prompting alone.
The structure is adapted from [jsmastery-pro/skills](https://github.com/jsmastery-pro/skills) (MIT), and rewritten for Rust, clean architecture, and this brief.

## Context

AI generates code quickly, but it drifts from the pattern when (1) it has to make a decision nobody wrote down, (2) it does not know the right order of work, or (3) nothing tells it that it is wrong. A task-specific template skill fixes only (2), and only for resources that look like the template.

## Decision

Build-time work (scaffold, conventions) is done with docs/conventions + chat. No build-time skills. `.agents/skills` is the shipped set, narrowed to resource work.

The docs are split by lifetime:

| Folder | Holds | Skills |
|--------|-------|--------|
| [docs/conventions/](../conventions/README.md) | Permanent boilerplate rules | read only |
| `docs/scope/`, `docs/specs/`, `docs/reports/` | The service's resource work, starting with `tracks` | read and write |
| [docs/assignment/](README.md) | This build history | ignored |
| [docs/prompts/](../prompts/README.md) | Example and demo prompts for people | ignored |

### Skills (after the MVP cut on 2026-10-10)

| Skill | Role | Writes |
|-------|------|--------|
| `flow-spec` | Grill the engineer on one resource, write its spec with ACs and a layer-ordered build plan, and add its scope row | `docs/specs/NNNN-*.md`, `docs/scope/scope.md` |
| `flow-build` | Implement inside out, layer by layer, stop on a missing decision, and keep the scope row current | Code + tests |
| `flow-verify` | Prove the ACs on the real stack | `docs/reports/` |
| `flow-review` | Review in a fresh session or on another model, against rule IDs, then close the resource | `docs/reports/` |

There is one flow, with no tiers: spec, build, verify, review. `flow-scope`, `flow-sync`, and the perf mode of `flow-verify` were moved to [deferred-skills/](deferred-skills/README.md).

The `flow-` prefix avoids clashing with built-in commands such as `/review` in Claude Code and Codex.

### Why it works for every kind of CRUD resource
- Everything resources have in common is already decided in [docs/conventions/](../conventions/README.md). `flow-spec` grills only on what differs: fields, invariants, relations, operations, filters, authorization, cache policy, messaging, and perf budgets.
- `flow-build` maps any resource spec to code through `layers.md`, a fixed order: schema > domain > application > infrastructure > inbound adapters > wiring > tests.
- **Generic first:** shared building blocks (pagination, errors, the cache helper) are reused or extended in the shared module, never copied into a resource.

### Strictness comes from checkable gates
| Gate | Mechanism |
|------|-----------|
| No silent decisions | `flow-build` stops and routes back to `flow-spec` when the spec leaves a choice open. If the engineer proceeds anyway, the assumption is recorded as an `Assumed` spec |
| Dependency rule | Separate crates plus `cargo deny` bans ([docs/conventions/stack-architecture.md](../conventions/stack-architecture.md) AC-2), so a layer violation fails to compile |
| AC traceability | AC > Build plan task > test named `ac_<n>_*` > evidence in the verify report |
| Done means green | `make verify` must pass before a skill reports success |
| Perf per best practice (deferred at the MVP cut) | `flow-verify perf` runs standard scenarios and compares them to the spec's budgets, always with real numbers. It lives in [deferred-skills/](deferred-skills/README.md) |
| Independent review | `flow-review` recommends a fresh session or another model, and each finding cites a rule ID |

### Multi-tool support
- The canonical location is `.agents/skills/<name>/SKILL.md` (the open [Agent Skills format](https://agentskills.io/specification)). `.claude/skills` is a symlink to it, and Claude Code already discovers the skills there.
- Each skill has `agents/openai.yaml` with picker metadata for Codex.
- `AGENTS.md` is the canonical project context. `CLAUDE.md` contains only `@AGENTS.md`.
- `.private/` holds personal, non-authoritative notes such as Thai translations. Skills ignore it.
- `cargo xtask check-skills` (planned: not built yet; `xtask` has only `token` and `kafka-produce`) validates each skill against the [Agent Skills specification](https://agentskills.io/specification): `name` is at most 64 characters, lowercase with hyphens, and matches its directory; `description` is at most 1024 characters; the `SKILL.md` body is under 500 lines (under 5000 tokens recommended); and file references are one level deep. It can wrap the spec's own validator, [`skills-ref validate`](https://github.com/agentskills/agentskills/tree/main/skills-ref). It also checks that every referenced file and `agents/openai.yaml` exist.

## Requirements

- **AC-1**: All four skills can be invoked from both Claude Code and Codex.
- **AC-2**: Running `/flow-spec <new resource>` and then `/flow-build` gives the same file layout (per `layers.md`) in both tools, and `make verify` passes.
- **AC-3**: When the spec omits a decision (for example, a field type), `flow-build` stops and asks rather than guessing.
- **AC-4**: The prompts in [docs/prompts/negative-tests.md](../prompts/negative-tests.md) are each stopped as listed there: the build fails, or the skill stops and cites the rule ID.
- **AC-5 (deferred at the MVP cut)**: `flow-verify perf` follows the [docs/conventions/api.md](../conventions/api.md) method: an open-model load, a capacity search, the percentage of requests under T90 and T99, p50/p95/p99, error rate, cache hit ratio, and an EXPLAIN for every query, each compared against its budget, with the environment recorded.
- **AC-6** (planned: not built yet): `cargo xtask check-skills` passes in CI. CI runs `make verify` only today.
- **AC-7**: Running the run sheet in [docs/prompts/demo-tracks.md](../prompts/demo-tracks.md) (`/flow-spec tracks`, `/flow-build tracks`, `/flow-verify tracks`, `/flow-review tracks`) from the bare boilerplate builds `tracks` (the finished result is on `example/track`) with the file layout in `layers.md` under `crates/` and `migrations/`, checked by eye, and `make verify` passes.

## Build plan
1. Draft skills v0.1 (done), then cut to four skills at the MVP cut (done)
2. `CLAUDE.md` and `AGENTS.md`, after the feature 2 scaffold (AC-1)
3. `xtask check-skills` (AC-6, planned: not built yet)
4. Dogfood on feature 5 (`tracks`), then refine the skills based on what we learn (AC-2, AC-3)
5. The negative tests (AC-4)
6. Test with Codex (AC-1, AC-2)
7. Run the demo prompts and check the structure (AC-7)

## Open questions

| # | Question | Recommendation |
|---|----------|----------------|
| W1 | Does the current Codex version read skills from `.agents/skills`, and what is the invocation syntax? | Check against real Codex in build step 6, and record the result in the README |

## Resolved questions (closed 2026-10-10 with the recommendation)

| # | Question | Decision |
|---|----------|----------|
| W2 | Do we need a `flow-debug` skill? | Not in this pass. Keep it deferred |
| W3 | `oha` only, or k6 as well for mixed workloads? | `oha` only. Use an xtask to generate URL lists when a scenario needs random ids |
