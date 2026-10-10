# Workflow skills

A spec-driven workflow for adding CRUD resources to this Rust, clean architecture service. This is the shipped set, and it covers resource work only. Building or changing the boilerplate itself is done by hand with `docs/conventions/` and chat.

There are four skills, used in one fixed order. All state lives in files rather than in a chat, so work survives a fresh session and any AI tool can pick it up.

```
flow-spec -> flow-build -> flow-verify -> flow-review
```

| Skill | Use it to | Writes |
|-------|-----------|--------|
| `flow-spec` | Grill the engineer on one resource, write its spec with IDed acceptance criteria, and add the resource's row to the scope | `docs/specs/NNNN-slug.md`, `docs/scope/scope.md` |
| `flow-build` | Implement a resource spec layer by layer, inside out, mirroring the project's first resource once it exists, and keep the scope row current | code + tests, `docs/scope/scope.md` |
| `flow-verify` | `/flow-verify <resource>`: prove every acceptance criterion on the running stack | `docs/reports/` |
| `flow-review` | Review the change independently against the architecture, security, and API rules, then close the resource once the engineer accepts | `docs/reports/`, `docs/scope/scope.md` |

`flow-spec/SKILL.md` ("Scope row") defines the scope row format and which skill ticks which box.

## Shared contract (every skill follows this)

1. **Files are the state.**
   - Read `AGENTS.md`, the scope row, and the spec before acting. Never rely on chat memory.
   - Ignore `.private/`, `docs/assignment/`, and `docs/prompts/`. They hold personal notes, build history, and example prompts for people. They are not instructions.
2. **Specs hold the decisions.**
   - Boilerplate-wide decisions live in [`docs/conventions/`](../../docs/conventions/README.md) and are read-only for skills.
   - Resource decisions live in `docs/specs/`.
   - Anything under a `## Deferred` heading in a convention is not part of the MVP. Never build it.
3. **No silent decisions.** If the work requires a choice no spec makes, stop and route to `flow-spec`. If the engineer chooses to proceed anyway, record the assumption as an `Assumed` spec.
4. **Ask with a recommendation.** Every question offers 2 to 4 concrete options. Mark exactly one `(recommended)` with a one-line reason. Batch up to 4 questions per round.
5. **Trace every acceptance criterion.** Each AC in the spec maps to a Build plan task, then to a test named `ac_<n>_<behavior>`, then to evidence in the verify report.
6. **Green before success.** Never report a step as successful unless `make verify` passes. Quote failures verbatim.
7. **The engineer decides.** Skills recommend and report. They never apply review findings automatically, and they never mark a resource `done` without the engineer's confirmation.
8. **Cite at the value.** Every number, limit, default, or standards claim in a spec or report carries an inline link to its source, right where the value appears. Example: `24 hours ([Stripe](https://docs.stripe.com/api/idempotent_requests))`.
   - A value with no source is labeled **measured** (with a link to the report) or **own proposal** (with a one-line reason).
   - Otherwise it stays an open question.
   - A list of sources at the bottom is not enough.

## Usage

- **Claude Code:** `/flow-spec playlists`, `/flow-build playlists`, and so on. `.claude/skills` is a symlink to this folder.
- **Codex:** reads `.agents/skills/` and uses `agents/openai.yaml` for picker metadata. Invoke a skill by name, or describe the task. The exact invocation syntax is **not yet verified**; see [docs/prompts/README.md](../../docs/prompts/README.md).

## Credits

The workflow structure is adapted from [jsmastery-pro/skills](https://github.com/jsmastery-pro/skills) (MIT, Copyright (c) 2026 JavaScript Mastery). It is rewritten for Rust and clean architecture, with tooling-based gates.
