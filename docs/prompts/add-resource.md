# Add a resource

Replace `<resource>` with the plural resource name, for example `playlists`. Run the four steps in order. Start a fresh session between steps if the context gets long; all state is in files.

## 1. Write the spec
- **Claude Code:** `/flow-spec <resource>`
- **Codex:** "Use the flow-spec skill to grill me on `<resource>`, write its spec in docs/specs/, and add its row to docs/scope/scope.md."
- **You answer:** rounds of up to 4 questions each, on fields, invariants, operations, relations, delete semantics, authorization, cache, Kafka commands, and the scope row. Every question has one recommended option.
- **Produces:**
  - `docs/specs/NNNN-<resource>.md` with status `Proposed`, IDed acceptance criteria, and a build plan grouped by layer
  - a row for the resource in `docs/scope/scope.md`, with `Spec it` ticked, the `Build it` milestones, `Verify it`, and `Review it`

## 2. Build it
- **Claude Code:** `/flow-build <resource>`
- **Codex:** "Use the flow-build skill to implement `<resource>` from its spec, following layers.md and mirroring the project's first resource if one exists."
- **Produces:**
  - `migrations/postgres/<timestamp>_create_<resource>.{up,down}.sql`
  - `crates/domain/src/<res>/`
  - `crates/application/src/<res>/` (including `contract.rs`, the repository contract suite)
  - `crates/infrastructure/src/{postgres,redis}/<res>_*.rs`
  - `crates/http-api/src/<res>/`
  - `crates/kafka-api/src/<res>/` (only if the spec declares commands)
  - wiring in `crates/bootstrap/src/`, and tests `crates/bootstrap/tests/<res>_*.rs` named `ac_<n>_*`
- **Gate:** `make verify` passes. If the spec leaves a decision open, the skill stops and asks instead of guessing.

## 3. Verify the acceptance criteria
- **Claude Code:** `/flow-verify <resource>`
- **Codex:** "Use the flow-verify skill for `<resource>`."
- **Produces:** `docs/reports/NNNN-<resource>-verify.md`, with PASS, FAIL, or N/A and evidence for every AC.

## 4. Review and close
- **Claude Code:** `/flow-review <resource>`, in a fresh session or with a different model
- **Codex:** "Use the flow-review skill to review the current change for `<resource>`. Do not edit code."
- **Produces:** `docs/reports/NNNN-<resource>-review.md`, with findings that cite rule IDs and a verdict. When you accept a "ready to merge" verdict, it ticks `Review it`, sets the resource to `done`, and moves the spec to `Accepted`.

## Where am I?
Open `docs/scope/scope.md`. The next step for a resource is its first unticked box.
