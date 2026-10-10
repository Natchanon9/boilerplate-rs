---
name: flow-review
description: Independent pre-merge review of a CRUD resource change against this repo's rules (clean architecture dependency rule, security baseline, API conventions, query efficiency, AC test coverage). Best run in a fresh session or on a different model than the one that wrote the code. Writes findings with rule IDs to docs/reports, never edits code, and closes the resource once accepted.
---

# flow-review

Writes `docs/reports/NNNN-<slug>-review.md`. This skill never edits source code.

## Independence
Recommend running this review in a fresh session, or on a different model than the one that wrote the code. A reviewer that shares the author's context shares the author's blind spots. If neither option is available, say so in the report.

## Inputs
- The diff against the base branch: `git diff <base>...HEAD`, plus uncommitted changes.
- The feature spec, and [docs/conventions/](../../../docs/conventions/README.md).
- `AGENTS.md` and `../flow-build/layers.md`.
- The latest verify report for this resource, if any.

## Process
1. Read `checklist.md` in full.
2. Walk the diff file by file. For each possible issue, build a concrete failure scenario: what input or state, and what goes wrong. If you cannot build one, drop the issue.
3. Check AC coverage: every AC in the spec has a passing test `ac_<n>_*`.
4. Write each finding:
   - severity: `blocker`, `major`, `minor`, or `nit`
   - rule ID (e.g. `ARCH-2`, `SEC-4`, `PERF-1`)
   - `file:line`
   - the failure scenario
   - a suggested fix
5. Order the findings by severity. An empty list is a valid result, so say so plainly.

## After the review
- Present the findings, and let the engineer choose which to fix, using `/flow-build`.
- Never apply fixes. Never downgrade a finding to make the review pass.
- When the verdict is "ready to merge" and the engineer accepts it, close the resource:
  - tick `Review it` in its scope row
  - set its status to `done` in the `At a glance` table
  - move its spec from `Proposed` to `Accepted`
  - list any `Assumed` decisions that still need ratifying with `/flow-spec`

## Report format
```markdown
## YYYY-MM-DD, commit <sha>, reviewer <model/session>
| # | Severity | Rule | Location | Finding | Failure scenario | Suggested fix |
AC coverage: <n>/<m> ACs have passing tests. Missing: AC-x
Verdict: ready to merge | fix blockers first
```
