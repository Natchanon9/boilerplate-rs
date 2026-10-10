# Performance verification

This file implements the method in [docs/conventions/api.md](../../../conventions/api.md) ("Performance budgets"). Read that section first. Do not invent thresholds, rates, or volumes. Each one comes from the spec, the baseline report, or a measurement taken in this run. Defaults marked **(own proposal)** are adjustable, and the report must state which values were used.

## Ground rules
1. **Release build:** `cargo build --release`. Never measure a debug build.
2. **Record the environment:** CPU, RAM, OS, docker resources, and `redis-cli --intrinsic-latency` output (run inside the redis container). Only compare runs from the same environment.
3. **Seed to the spec's perf volume** (the resource spec's "Data volume" line): `cargo xtask seed <res> --rows <volume>`. If the spec does not state a volume, stop and ask. Never pick one yourself.
4. **Use an open model only:** `oha -q <rate> --latency-correction -z <duration> --no-tui` with JSON output (check `oha --help` for the output flag on the installed version). Keep-alive is on ([docs/conventions/api.md](../../../conventions/api.md) rule 7).
5. **Warm up, then measure** (own proposal): a short warm-up run at the test rate, discarded. Use a fixed duration, and repeat each run 3 times. Report the median run and the spread.
6. **Correctness under load:** zero non-intentional errors. A run with errors fails, however fast it is.

## Step 1: Capacity search (once per environment, and again after major changes)
1. Start at a low rate. Increase it step by step: double it until the first failure, then bisect between the last pass and the first fail.
2. A step **passes** when `99% < 100 ms` (the ceiling) and there are 0 errors.
3. Record T99 at every step. This is the rate-against-T99 curve. Attach it to the report as a table, and as a chart if one is available.
4. **Capacity** is the highest passing rate. Report it for each scenario.
5. **Operating rate** is the knee of the curve, located with the Kneedle method ([docs/conventions/api.md](../../../conventions/api.md) rule 8). Report the knee and the data behind it.

## Step 2: Baseline (when there are no budgets yet)
Run a baseline when `docs/reports/perf-baseline.md` does not exist (the first perf run, normally the walking-skeleton resource), or when the resource spec overrides budgets or adds an operation type that has no baseline.
1. Run each scenario at the operating rate (the knee from step 1).
2. Record T90 and T99 for each operation, then round them to manageable numbers (SRE workbook practice).
3. Write the results to `docs/reports/perf-baseline.md`, and propose them as the Accepted budgets. The engineer confirms.

## Step 3: Gate runs (every later perf verify)
Run the applicable scenarios at the operating rate, and compare each against the baseline budgets and the ceiling.

## Scenarios
| ID | Scenario | Setup | Pass condition |
|----|----------|-------|----------------|
| P1 | GET by id, hot key | Cache on, then cache off, at the same rate | Budgets met. Cache gate: cache-on T99 is below cache-off T99. All requests are HITs after the first, until the TTL expires |
| P2 | GET by id, Redis down | Stop the redis container | 0 errors (fail-open). Latency stays near cache-off plus at most `CACHE_TIMEOUT_MS` |
| P3 | GET by id, many keys | URL list of random seeded ids (`cargo xtask perf-urls`) | Budgets met. Hit ratio is reported (a result, not a budget) |
| P4 | List, first page vs a deep cursor | A cursor from deep in the seeded data | Both meet budget. A large gap means the keyset index is missing |
| P5 | Writes | Create and update | Budgets met. The next GET after an update is a MISS (invalidation works) |
| P6 | Kafka throughput | Produce the seeded volume as commands | Time to drain, peak consumer lag, 0 DLQ messages. These are reported as results |

## Database checks
- Run `EXPLAIN (ANALYZE, BUFFERS)` for every repository query at the spec's perf volume, with realistic parameters.
- Flag any `Seq Scan` on a query that the spec expects to be index-backed, any sort that spills to disk, and any nested loop over an unindexed join.
- Check the connection pool: if `/metrics` shows pool wait time, report the pool size and the concurrency observed.

## Report table
| Scenario | Rate (rps) | % < T90 | % < T99 | p50 | p95 | p99 | Errors | Hit ratio | Verdict |
|----------|-----------|---------|---------|-----|-----|-----|--------|-----------|---------|

For each failing row, add one paragraph: what was measured, the likely cause (with evidence from the query plan or metrics), and a suggested fix. The suggestion goes to the engineer. Never apply it.
