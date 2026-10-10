# Demo: build `music` live

The run sheet for the video. `music` is built from the bare boilerplate, so the video shows the whole workflow from an empty resource list. The engineer designs the fields during the grill, in their own session ([decision of 2026-10-10](../assignment/scope.md), Decisions log).

**Before recording:** check out the `example/music` branch. It is `main` plus nothing else, so no resource exists yet. The grill answers are recorded in the spec that `/flow-spec music` writes (`docs/specs/NNNN-music.md`), not in this file. The local `.env` keeps `ENABLE_FEATURES=cache,kafka`, so music can demo both the cache and the Kafka commands.

## Run sheet

| # | Step | Claude Code | Codex (to be confirmed (W1)) |
|---|------|-------------|------------------------------|
| 0 | Branch | `git checkout example/music` | the same |
| 1 | Spec | `/flow-spec music` | "Use the flow-spec skill to grill me on music, write its spec in docs/specs/, and add its row to docs/scope/scope.md." |
| 2 | Build | `/flow-build music` | "Use the flow-build skill to implement music from its spec, following layers.md." |
| 3 | Verify | `/flow-verify music` | "Use the flow-verify skill for music." |
| 4 | Review | `/flow-review music` (fresh session) | "Use the flow-review skill to review the current change for music. Do not edit code." |

The Codex column uses plain-language prompts that work either way. Whether Codex has a shorter invocation syntax is not yet verified (open question W1 in [ai-workflow-skills.md](../assignment/ai-workflow-skills.md)).

## What the grill decides (step 1)

The checklist items are the ones `flow-spec` asks about. The engineer answers them live, and the answers are written into the spec. Do not invent a value here (rule 8 in `.agents/skills/README.md`).

| Checklist item | Answer |
|----------------|--------|
| Fields (name, types, required, constraints, defaults, source) | decided by the engineer in `/flow-spec music` |
| Invariants and uniqueness | follows from the fields |
| Operations enabled | create, get, list, update, delete (own proposal: exercises every rule in [api.md](../conventions/api.md)) |
| Cache | enabled, so the video can show it (the TTL default is in [api.md](../conventions/api.md) MVP limits and [cache.md](../conventions/cache.md)) |
| Kafka commands | create, update, delete (engineer decision, 2026-10-10) |

If the skill asks a question that is not covered by the spec's own defaults, answer it live and let the spec record the answer.

## Cache on/off step (shown in the video)

After step 3, run the single `oha` comparison described in [cache.md](../conventions/cache.md) ("Demo measurement (MVP)"), against one music id (a hot key):

1. Create one music record, and get a token with `cargo xtask token`.
2. With `cache` in `ENABLE_FEATURES`, run the `oha` command from cache.md against that one id.
3. Restart without `cache` in `ENABLE_FEATURES`, and run the same command at the same rate.
4. Show both outputs side by side. The rate is picked at recording time and stated in the video (measured, see cache.md).

Note: cache.md's example URL still says `products`. Use the music path from the spec instead.

## Structure check (optional)

After step 2, list the new files and check by eye that every layer in [layers.md](../../.agents/skills/flow-build/layers.md) has its files:

```bash
find crates migrations -path '*music*' | sort
```
