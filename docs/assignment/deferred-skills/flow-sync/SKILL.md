---
name: flow-sync
description: Close the loop after a resource is built and checked. Reconcile its scope and spec statuses with the real diff, propose AGENTS.md additions, report drift between layers.md and the conventions, and draft the CHANGELOG entry and PR description. Use as the last step of a resource.
---

# flow-sync

Keeps the files that hold state truthful for **one feature**, after its change. Use `/flow-scope` with no argument for the whole-service view. It writes only what the diff and the reports show, and never invents anything.

## Steps
1. **Read the change:** the diff against the base branch, the feature spec, and the latest verify, perf, and review reports.
2. **Scope:**
   - Check that every box is ticked by its owning skill (see the table in `../flow-scope/SKILL.md`). If a box is missing, tick it only when a report proves it. Mark skipped steps `(skipped)`, never delete them.
   - Check the pointer line, and tick `Sync it`.
   - Ask whether to mark the feature `done`, and set it only after the engineer says yes.
3. **Spec status:**
   - When the feature is `done`, move `Proposed` to `Accepted`.
   - Never clear `Assumed`. Only `/flow-spec` ratifies an assumption. List outstanding assumptions in the report.
4. **Context files:**
   - If the change introduced a new shared building block, command, or pattern, propose the exact lines to add to `AGENTS.md`.
   - Compare `../flow-build/layers.md` with [docs/conventions/stack-architecture.md](../../../conventions/stack-architecture.md) (crate map, dependency rule, paths). If they differ, report the drift. Conventions are edited by hand, so never edit them: propose the convention change for the engineer to make, and update `layers.md` only after the engineer has changed the convention.
   - Apply the `AGENTS.md` and `layers.md` edits after confirmation.
5. **Release notes:**
   - Add a `CHANGELOG.md` entry under `## [Unreleased]` (create the file with the standard header if it does not exist) ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format).
   - Draft a PR body: summary, ACs covered, verify and perf results (with numbers), review verdict, and known trade-offs.

## Never
- Mark a feature `done` without confirmation.
- Describe behavior that is not in the diff.
- Rewrite curated `AGENTS.md` content. Only append to it, or propose edits.

## Report
List the files updated, the statuses changed, any outstanding assumptions, and the PR body, ready to paste.
