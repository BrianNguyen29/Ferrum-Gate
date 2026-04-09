# 42 — P2 Performance Baseline Evidence

**Last updated:** 2026-04-08  
**Gate:** G-E2 — P2 performance baseline established  
**Status:** ✅ DONE 2026-04-08 — evidence captured and ratified via cross-doc sign-off (`30-production-roadmap.md`, `41-production-execution-plan.md`, `43-production-readiness-signoff.md`)

---

## Scope

This doc records the in-repo benchmark suite added for G-E2.

Per `41-production-execution-plan.md`, G-E2 requires a benchmark suite that covers
key SQLite and adapter paths under concurrent load. This gate establishes a
measured baseline; it does **not** require optimization or a throughput target.

The benchmark harness is implemented as a standalone workspace binary crate:

- `benches/Cargo.toml`
- `benches/src/main.rs`

It intentionally does **not** import or merge the out-of-tree write-queue
candidate documented in `40-out-of-tree-sqlite-performance-candidate.md`.

---

## Benchmark Surface

The harness covers four concurrent scenarios:

| Stage | Scenario | Surface | Notes |
|-------|----------|---------|-------|
| S4 | `intent-compile` | `ferrum-store` pooled SQLite path | intent + proposal persistence + status update |
| S5 | `execution-pipeline` | `ferrum-store` pooled SQLite path | execution + rollback persistence |
| S6 | `capability-cycle` | `ferrum-store` pooled SQLite path | capability insert + single-use transition |
| S7 | `sqlite-contention` | `ferrum-adapter-sqlite` non-pooled path | adapter prepare → execute → verify → rollback |

---

## Commands Run

Build and smoke verification:

```bash
cargo build -p ferrum-perf-baseline
cargo test -p ferrum-perf-baseline
cargo run -p ferrum-perf-baseline -- --concurrency 2 --iterations 2
```

Baseline evidence run:

```bash
cargo run --release -p ferrum-perf-baseline -- --concurrency 5 --iterations 5
```

---

## Baseline Results (`--release --concurrency 5 --iterations 5`)

- total requested operations per scenario: `25`
- successful operations and error counts are reported separately below

| Scenario | Surface | Successful Ops | Total Seconds | Throughput (ops/sec) | Avg Latency (ms) | Min Latency (ms) | Max Latency (ms) | Errors |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| `intent-compile` | `ferrum-store (pooled)` | 22 | 20.296 | 1.08 | 1791.54 | 372.80 | 8050.86 | 3 |
| `execution-pipeline` | `ferrum-store (pooled)` | 11 | 26.336 | 0.42 | 3886.62 | 1060.23 | 8847.33 | 14 |
| `capability-cycle` | `ferrum-store (pooled)` | 22 | 20.629 | 1.07 | 1748.18 | 585.55 | 9253.77 | 3 |
| `sqlite-contention` | `ferrum-adapter-sqlite (non-pooled)` | 19 | 21.961 | 0.87 | 1958.57 | 731.79 | 6150.11 | 6 |

---

## Interpretation

1. **G-E2 baseline exists in repo truth now.** We have a runnable benchmark suite
   covering the required SQLite/store and adapter surfaces under concurrent load.
2. **This is baseline evidence, not optimized evidence.** The current numbers
   reflect the in-repo implementation as of 2026-04-08.
3. **Contention/error counts are part of the measured baseline.** Under the chosen
   concurrent workload, some scenarios exhibit lock/contention behavior rather than
   0%-error throughput. That is acceptable for baseline establishment; it does not
   by itself justify importing the out-of-tree write-queue candidate.
4. **The out-of-tree candidate remains separate.** Any future optimization must be
   proposed and validated through normal in-repo review/benchmark slices.

---

## Gate Conclusion

The benchmark harness required for G-E2 is implemented and benchmark results are
captured in repo docs. G-E2 was subsequently ratified on 2026-04-08 via the
roadmap/execution-plan/sign-off doc set:

- `30-production-roadmap.md`
- `41-production-execution-plan.md`
- `43-production-readiness-signoff.md`
