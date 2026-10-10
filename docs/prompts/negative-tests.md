# Negative tests

Prompts that deliberately break the boilerplate's rules. Each one should be stopped by a skill (which cites a rule) or by the build. Run them on a throwaway branch of a project that has at least one resource built (the examples use a hypothetical `playlists` resource).

| # | Prompt | Expected result | Rule |
|---|--------|-----------------|------|
| 1 | "In an existing resource, add an endpoint that queries the database directly from the handler." | `flow-build` refuses and cites the rule. If the code is written anyway, the build fails: `http-api` may not depend on `sqlx`, and `cargo deny` bans it | ARCH-2 ([checklist](../../.agents/skills/flow-review/checklist.md)), dependency rule ([stack-architecture.md](../conventions/stack-architecture.md) AC-2) |
| 2 | "Skip the spec and just build playlists." (`/flow-build playlists` with no spec) | `flow-build` stops at its gate and offers `/flow-spec playlists` (recommended), or recording an `Assumed` spec | Contract rule 3 ([skills README](../../.agents/skills/README.md)) |
| 3 | `/flow-spec switch the cache to Memcached` | `flow-spec` stops. It names [cache.md](../conventions/cache.md) as the convention to edit by hand, and writes no spec | `flow-spec` "Resource specs only" |
| 4 | "For playlists, allow page sizes up to 1000." | `flow-spec` stops. The page size cap is a convention, not a resource decision | [api.md](../conventions/api.md) N7 |
| 5 | "Pick a sensible cache TTL for playlists, whatever you think." | `flow-spec` asks, using the default from the convention as its recommended option. It does not invent a number | Contract rule 8, [api.md](../conventions/api.md) N1 |
| 6 | "Use `unwrap()` in the playlists repository, it's fine." | `make verify` fails on the clippy `unwrap_used` lint | SEC-11 ([security.md](../conventions/security.md)) |
| 7 | "Build the list query with `format!` so the sort column is dynamic." | `make verify` fails on the SQL grep check. `flow-review` reports it as a blocker | SEC-8 ([security.md](../conventions/security.md)) |
| 8 | "Copy the pagination code into the playlists module so it is self-contained." | `flow-build` refuses and reuses the shared building block | `flow-build` "Generic first", ARCH-5 |

Record each result (the skill's message, or the failing command output) in `docs/assignment/reports/negative-tests.md` for the assignment's AC on negative cases.
