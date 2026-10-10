---
name: flow-build
description: Implement a CRUD resource from its spec in the fixed clean architecture order (schema, domain, application, infrastructure, inbound adapters, wiring, integration tests), then gate on make verify. Mirrors the project's first resource once one exists, otherwise layers.md. Stops and asks when the spec is missing a decision.
---

# flow-build

Turns a resource spec into code. It never invents a design, and it never changes [docs/conventions/](../../../docs/conventions/README.md) or the shared building blocks that implement them.

## Gate (before writing any code)
1. Find the spec: the resource's row in `docs/scope/scope.md` links to it. If there is no spec, offer `/flow-spec <resource>` (recommended), or build on an assumption (see step 3).
2. Read `AGENTS.md`, the spec, the specs it builds on, and `layers.md` in full.
3. **Decision check.** For every Build plan task, can you write it without choosing something the spec leaves open (a type, a status code, a TTL, an index, an error mapping)? For each gap, ask:
   - `Run /flow-spec to decide (recommended)`: decisions belong in specs, where every flow skill can find them
   - `Use <your proposed answer> as an assumption`

   Record assumptions as follows:
   - **The spec exists:** write or append to `docs/specs/NNNN-<slug>-assumptions.md`, using the spec's own number, with `**Status**: Assumed`.
   - **No spec exists:** create `docs/specs/NNNN-<slug>.md` with the next free number and `**Status**: Assumed`, holding the assumed decisions and a minimal Build plan.
   - In both cases, add the tag `assumed decision (spec NNNN)` to the resource's scope row. If the row does not exist, add it in the shape from `../flow-spec/SKILL.md` ("Scope row").
4. Run `git status`. If there are unrelated uncommitted changes, warn and ask before continuing.

## Build order (strict, inside out)
Follow `layers.md` for paths, naming, and the rules of each layer. Finish each step, and make sure it compiles (`cargo check -p <crate>`), before starting the next.

| # | Layer | Tests written in this step |
|---|-------|----------------------------|
| 1 | Schema (migration) | |
| 2 | Domain | unit tests for invariants, written test first |
| 3 | Application (ports, use cases, contract suite) | unit tests with in-memory fakes, written test first; the fake passes `<res>_repository_contract` |
| 4 | Infrastructure (Postgres repo, cache decorator) | the Postgres repo passes `<res>_repository_contract` |
| 5 | Inbound adapters (HTTP, then Kafka if the spec declares commands) | DTO validation tests |
| 6 | Wiring (bootstrap) | |
| 7 | Integration tests | one or more per AC, named `ac_<n>_<behavior>`, using testcontainers |
| 8 | Gate | `make verify` must pass |

## Reference implementation
If a resource already exists under `crates/*/src/<res>/`, the first one built is the reference implementation. Mirror its file layout, naming, and patterns layer by layer. Where the spec needs something it does not have, follow `layers.md`. For the first resource of a project, `layers.md` is the only reference.

## Generic first
Before writing per-resource code, look for the shared building block: pagination, `ApiError`, `JsonBody`, the application parser, `cache_aside`, the Kafka envelope and DLQ producer, `RequireScope`, the router registration, or the `DB_PLATFORM` wiring. Reuse it. Never copy and paste a generic piece into a resource module.

If a generic building block is missing, **stop** and report it as a boilerplate gap. Adding one is build-time work on the conventions (done by hand and chat), not part of a resource build. Never build anything a convention lists under `## Deferred`.

## Progress
Keep the resource's row in `docs/scope/scope.md` current. It uses the shape and milestone mapping in `../flow-spec/SKILL.md` ("Scope row").
- When you start, set the resource's status to `in-progress` in the `At a glance` table.
- Tick each spec Build plan task, and the matching scope milestone, as soon as it is done.
- When the gate passes and the engineer confirms, tick `Build it`. Then append `, code <paths>` to the spec link line.
- Never set `done`; `flow-review` does that once the engineer accepts. The next step is always `/flow-verify <resource>`.

## When the gate fails
Fix the problem and run `make verify` again. After 3 failed attempts on the same error, stop. Report the exact output and your diagnosis, and ask how to proceed.

## Report
- The files changed, grouped by layer
- An AC coverage table: `AC-n -> test name -> pass or fail`
- The `make verify` result, quoted if it failed
- Any assumptions recorded
- The next step: `/flow-verify <resource>`
