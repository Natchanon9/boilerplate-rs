# Conventions

Permanent rules of the boilerplate: stack and architecture, API rules, cache, Kafka, and security.

Each file describes the **MVP** that ships on 14 Oct. Anything designed but cut is kept under a `## Deferred` heading in the same file, with the reason and what the MVP uses instead.

- To change a convention, edit the file by hand and commit it with the reason in the commit message.
- Flow skills only read these files. They never edit them. `flow-spec` stops when a request would change a convention.
- Every value carries an inline source link, or is labeled `measured` or `own proposal` (see `.agents/skills/README.md`, rule 8).

| File | Covers |
|------|--------|
| [stack-architecture.md](stack-architecture.md) | Crates, dependency rule, stack, error flow |
| [api.md](api.md) | HTTP rules and MVP limits. The perf method and the N1 to N8 limits are under Deferred |
| [database.md](database.md) | `DB_PLATFORM`, adapters, reversible migrations, seeds, contract tests, and the recipe for a new engine |
| [runtime.md](runtime.md) | CLI, config precedence, `ENABLE_FEATURES`, and the public, protected, and hidden routers |
| [cache.md](cache.md) | Toggleable cache-aside |
| [kafka.md](kafka.md) | Kafka as an inbound transport |
| [security.md](security.md) | Auth design and SEC rule IDs |

The `**Built in**` line in each file points to the assignment history in `docs/assignment/`. If that folder is deleted, drop those lines.

## Credits

The runtime shape in [runtime.md](runtime.md) and [database.md](database.md) (the CLI subcommands, `ENABLE_FEATURES`, the selectable `DB_PLATFORM`, and separate public, protected, and hidden routers) is inspired by [codoworks/go-boilerplate](https://github.com/codoworks/go-boilerplate) (MIT). No code is taken from it; this boilerplate is written for Rust from scratch.
