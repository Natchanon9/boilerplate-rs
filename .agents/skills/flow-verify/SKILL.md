---
name: flow-verify
description: Prove a built CRUD resource works on the real stack by exercising every acceptance criterion in its spec, with recorded evidence (HTTP responses, database rows, Redis keys, Kafka messages). Use after flow-build. Reports only, never edits code.
---

# flow-verify

Invoke it as `/flow-verify <resource>`.

It writes `docs/reports/NNNN-<slug>-verify.md`, appending each run under a dated heading. It never edits source code.

## Preconditions
- `make verify` passes. If it does not, stop and report the failure.
- `docker compose up -d` is running, and `curl http://127.0.0.1:8079/readyz` (the hidden router, [runtime.md](../../../docs/conventions/runtime.md)) returns 200.
- Optional demo data is loaded with `app db seed`.

## Steps
For every AC in the spec, in order:
1. Perform the action for real:
   - HTTP: `curl -sS -i` with a dev token from `cargo xtask token`
   - Kafka: `cargo xtask kafka-produce`
2. Capture the evidence the AC needs:
   - the response status, headers, and body
   - the DB row (`psql`)
   - the Redis key and TTL (`redis-cli`)
   - the Kafka message, or the DLQ entry (Kafka UI or `kcat`)
3. Mark the AC `PASS`, `FAIL`, or `NOT RUN`, with the reason. **There is no PASS without evidence.**

Then cover the cross-cutting checks that apply:
- 401/403 ([docs/conventions/security.md](../../../docs/conventions/security.md))
- 404 and 422 with field codes ([docs/conventions/api.md](../../../docs/conventions/api.md))
- fail-open with Redis stopped ([docs/conventions/cache.md](../../../docs/conventions/cache.md))

If the spec disables a capability (for example no cache, or no Kafka commands), mark its checks `N/A` and cite the spec line as the reason.

## After the run
- **Every AC passes:** tick `Verify it` in the scope row once the engineer confirms. The next step is `/flow-review <resource>`.
- **Something fails:** list the failures with evidence, and suggest `/flow-build <resource>` with the specific fix. Never fix it yourself.

## Report format
```markdown
## YYYY-MM-DD HH:MM, commit <sha>
| AC | Result | Evidence |
|----|--------|----------|
| AC-1 | PASS | `HTTP/1.1 201`, Location: /api/v1/products/0192... |
Summary: <n> pass, <n> fail, <n> not run. Next: <step>
```
