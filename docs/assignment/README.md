# Assignment history

How this boilerplate was built for the take-home assignment: the build plan, the decisions log, and the design record of the workflow skills.

- [scope.md](scope.md): the build plan and the decisions log (features 1 to 9)
- [ai-workflow-skills.md](ai-workflow-skills.md): why the flow skills look the way they do
- `reports/`: verify, perf, and review reports for build-time work

Flow skills do not read this folder. The permanent rules that came out of this work live in [docs/conventions/](../conventions/README.md).

**Starting a new service from this boilerplate?** You can delete this whole folder, but first remove or repoint the links into it. These are all of them (checked with `git grep assignment`):

| File | What links in |
|------|---------------|
| `docs/conventions/{api,cache,database,kafka,runtime,security,stack-architecture}.md` | the `**Built in**` line at the top (drop it); `docs/conventions/README.md` explains it |
| `docs/conventions/api.md` (Deferred, perf method) | a link to `deferred-skills/flow-verify/perf.md` (drop the sentence or keep the perf method elsewhere) |
| `docs/prompts/README.md`, `docs/prompts/demo-tracks.md` | links to `ai-workflow-skills.md` (open question W1) and `scope.md` (decisions log) |
| `docs/prompts/negative-tests.md` | tells the engineer to record results in `reports/negative-tests.md` |
| `AGENTS.md`, `.agents/skills/README.md` | only name the folder as ignorable, no link |

`docs/prompts/` can be deleted with it, since it holds the run sheets for the assignment demo.
