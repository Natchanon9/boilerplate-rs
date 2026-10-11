# Prompts

Example prompts for people who use this boilerplate. They are written for humans to copy. Flow skills do not read this folder, and nothing here is an instruction to a skill.

## Order of work for one resource

```
/flow-spec <resource>     grill, write docs/specs/NNNN-<resource>.md, and add the row to docs/scope/scope.md
/flow-build <resource>    implement it layer by layer, then make verify
/flow-verify <resource>   prove every acceptance criterion on the running stack
/flow-review <resource>   independent review (fresh session or another model), then close it
```

Every resource goes through all four steps, in this order.

## How to invoke

- **Claude Code:** type the slash command, for example `/flow-spec playlists`. Claude Code finds the skills through `.claude/skills`, which links to `.agents/skills`.
- **Codex:** reads `.agents/skills/`. Ask for the skill by name in plain language, for example "Use the flow-spec skill for playlists". Whether Codex also has a shorter invocation syntax is **not yet verified** (open question W1 in [docs/assignment/ai-workflow-skills.md](../assignment/ai-workflow-skills.md)). Each step in [add-resource.md](add-resource.md) has a plain-language prompt that works either way.

## Files

| File | Use it for |
|------|-----------|
| [add-resource.md](add-resource.md) | Adding any resource, step by step, with the files each step produces |
| [demo-tracks.md](demo-tracks.md) | A run sheet: `tracks` built from the bare boilerplate, with the cache shown through `X-Cache` |
| [negative-tests.md](negative-tests.md) | Prompts that deliberately break the rules, and what should happen |
