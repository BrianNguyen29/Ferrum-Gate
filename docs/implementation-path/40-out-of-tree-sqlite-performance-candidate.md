# 40 — Out-of-Tree SQLite Performance Candidate

> **⚠️ Out-of-tree / unmerged**: This document captures performance findings that are not yet merged into the canonical documentation chain. Treat as draft candidate.
> **Status**: Phase 1 completed and production-tested. Phase 2 reverted.

---

## SQLite Single-Writer Performance — Local Findings

### Problem

SQLite's single-writer architecture creates a hard throughput ceiling of ~8–10 writes/s.
Combined with synchronous handler writes and a cascading FK chain
(`intents → proposals → capabilities → executions → rollback_contracts`), this produces:

| Scenario | Workers | Throughput | p50 Latency | Error Rate | Root Cause |
|----------|---------|------------|-------------|------------|------------|
| S4 intent-compile | 5 | ~6.5 req/s | ~140ms | ~10% | Single INSERT contention |
| S5 execution-pipeline | 5 | ~1.1 req/s | ~958ms | ~73% | 6 sequential writes × contention |
| S6 capability cycle | 5 | ~1.5 req/s | varies | ~33% | 3 writes + fire-and-forget |
| S7 sqlite-contention | 50 | ~8.1 req/s | ~3.26s | 0%* | Pure write saturation |

*S7 showed 0% errors because requests queued behind SQLite's writer lock, but latency was extremely high due to lock serialization.

---

## Phase 1 — Write Queue + Retry Cleanup + PRAGMA Tuning

**What was done**:
1. Replaced N connections competing for SQLite write lock with 1 dedicated writer task via `tokio::sync::mpsc` channel
2. Removed stale retry loops from handlers (mint_capability, authorize_execution)
3. Tuned SQLite PRAGMAs: `synchronous=NORMAL`, `wal_autocheckpoint=1000`, `cache_size=-64000`, `busy_timeout=5000`

**Measured results** (release build, `ferrum-stress` suite):

| Scenario | Before | After | Improvement |
|----------|--------|-------|-------------|
| S4 intent-compile | 6.5 req/s, 140ms, ~10% err | 305.5 req/s, 2.25ms, 0% err | ~47x throughput |
| S5 execution-pipeline | 1.1 req/s, 958ms, ~73% err | 57.6 req/s, 16.0ms, 0% err | ~52x throughput |
| S6 capability cycle | 1.5 req/s, ~33% err | 42.0 req/s, 0.30ms, 0% err | ~28x throughput |
| S7 sqlite-contention | 8.1 req/s, 3.26s, 0% err | 289.4 req/s, 29.9ms, 0% err | ~36x throughput |

Phase 1 materially outperformed the original target. The queue removed lock-thrashing rather than merely smoothing it.

**Phase 1 did NOT implement true cross-operation transaction batching.** Writes are serialized through a single writer task, but each write is its own transaction. The `Pipeline` variant (batching multiple writes into a single `BEGIN IMMEDIATE ... COMMIT`) belongs to Phase 2 and was reverted.

---

## Phase 2 — Broader Batching Experiment (Deferred)

**What was attempted**: A `Pipeline` variant in the WriteOp enum that would batch multiple heterogeneous operations (INSERT intent + INSERT proposal + INSERT capability, etc.) into a single explicit transaction.

**Result**: Benchmark testing revealed performance regression — not improvement. The implementation was reverted.

**Reason**: The write queue already eliminates lock thrashing via serialization. Batching heterogeneous operations into a single transaction added complexity overhead without meaningful lock contention reduction, because the single-writer model means only one write proceeds at a time regardless.

**Current status**: Phase 2 deferred. Phase 1 write-queue architecture is production-ready for single-node workloads up to ~300 writes/s sustained.

---

## Phase 3 — PostgreSQL Migration

For higher sustained write throughput or multi-node deployment, migrate to PostgreSQL:
- `store_dsn` scheme changes from `sqlite:` to `postgres:`
- `ferrum-store` supports PostgreSQL via the same `StoreFacade` trait
- PostgreSQL removes the single-writer bottleneck entirely
- Target: 1000+ writes/s, 200+ pipelines/s

---

## References

- `docs/PRODUCTION_NOTES.md` — complete before/after stress test table with PRAGMA details
- `docs/PERFORMANCE_OPTIMIZATION_PLAN.md` — full three-phase plan with architecture diagrams
- `docs/implementation-path/27-production-evaluation-plan.md` — production evaluation framework