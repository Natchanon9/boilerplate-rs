# Demo: build `tracks` live

The run sheet for building `tracks` live. `tracks` is built from the bare boilerplate, so it shows the whole workflow from an empty resource list. The engineer designs the fields during the grill, in their own session ([decision of 2026-10-12](../assignment/scope.md), Decisions log).

**Before you start:** start from `main`, the bare boilerplate, so no resource exists yet. The grill answers are recorded in the spec that `/flow-spec tracks` writes (`docs/specs/NNNN-tracks.md`), not in this file. The local `.env` needs `ENABLE_FEATURES=cache`, so tracks can demo the cache. Kafka is not part of the demo (decision of 2026-10-12).

## Run sheet

| # | Step | Claude Code | Codex (to be confirmed (W1)) |
|---|------|-------------|------------------------------|
| 0 | Branch | `git checkout -b demo/tracks main` | the same |
| 1 | Spec | `/flow-spec tracks` | "Use the flow-spec skill to grill me on tracks, write its spec in docs/specs/, and add its row to docs/scope/scope.md." |
| 2 | Build | `/flow-build tracks` | "Use the flow-build skill to implement tracks from its spec, following layers.md." |
| 3 | Verify | `/flow-verify tracks` | "Use the flow-verify skill for tracks." |
| 4 | Review | `/flow-review tracks` (fresh session) | "Use the flow-review skill to review the current change for tracks. Do not edit code." |

The Codex column uses plain-language prompts that work either way. Whether Codex has a shorter invocation syntax is not yet verified (open question W1 in [ai-workflow-skills.md](../assignment/ai-workflow-skills.md)).

The finished result of this run sheet is on the `example/track` branch: one commit per step (spec, build, verify, review) plus the review fixes.

## What the grill decides (step 1)

The checklist items are the ones `flow-spec` asks about. The engineer answers them live, and the answers are written into the spec. Do not invent a value here (rule 8 in `.agents/skills/README.md`).

| Checklist item | Answer |
|----------------|--------|
| Fields (name, types, required, constraints, defaults, source) | decided by the engineer in `/flow-spec tracks` |
| Invariants and uniqueness | follows from the fields |
| Operations enabled | create, get, list, update, delete (own proposal: exercises every rule in [api.md](../conventions/api.md)) |
| Cache | enabled, so the demo can show it (the TTL default is in [api.md](../conventions/api.md) MVP limits and [cache.md](../conventions/cache.md)) |
| Kafka commands | none (engineer decision, 2026-10-12: Kafka is not part of the demo) |

If the skill asks a question that is not covered by the spec's own defaults, answer it live and let the spec record the answer.

## Showing the cache

Step 3 shows the cache with the `X-Cache` header (`CACHE_DEBUG_HEADER=true`): the first get of a track is `MISS`, the next is `HIT`, and a `PUT` or `DELETE` makes the next get a `MISS` again. There is no load comparison of cache on and cache off (decision of 2026-10-12).

## Structure check (optional)

After step 2, list the new files and check by eye that every layer in [layers.md](../../.agents/skills/flow-build/layers.md) has its files:

```bash
find crates migrations seeds -path '*track*' | sort
```
