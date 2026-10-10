# Deferred skills

These skills were part of the workflow before the MVP cut on 2026-10-10. They are kept for reference and for the interview. They are not installed: Claude Code and Codex only load skills from `.agents/skills/`.

| Skill | What it did | What the MVP uses instead |
|-------|-------------|---------------------------|
| [flow-scope](flow-scope/SKILL.md) | Planned the service scope, added resources, reported status and drift | `flow-spec` adds the resource's row to `docs/scope/scope.md` |
| [flow-sync](flow-sync/SKILL.md) | Closed the loop: statuses, `AGENTS.md`, CHANGELOG, PR body | `flow-build` and `flow-review` keep the scope row current |
| [flow-verify/perf.md](flow-verify/perf.md) | The perf mode of `flow-verify`: capacity search, knee, baseline, gate runs | One `oha` cache on/off comparison for the video (`docs/conventions/cache.md`) |

The tiers (`spike`, `standard`, `production`) were removed with them. The MVP has a single flow: spec, build, verify, review.
