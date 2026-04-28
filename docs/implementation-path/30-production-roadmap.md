# 30 — Production Roadmap

> **Status**: In-tree documentation. Captures the production roadmap decisions for FerrumGate v1 single-node SQLite scope.

---

## Production Architecture Roadmap

Three-phase plan to resolve SQLite single-writer bottleneck and reach full production scale.

| Phase | Solution | Status | Key Results |
|-------|----------|--------|-------------|
| **Phase 1** | Write-Queue (mpsc) + retry cleanup + PRAGMA tuning | ✅ Production-ready | S4: 47x, S5: 52x, S6: 28x, S7: 36x throughput improvement; all errors → 0% |
| **Phase 2** | Transaction batching for pipelines + direct UPDATE | ✅ Done (reverted) | Reverted; Phase 1 remains production target |
| **Phase 3** | PostgreSQL migration | Planned | Target: 1000+ writes/s, 200+ pipelines/s |

> **Note**: Phase 2 was partially implemented (Transaction/Pipeline batching in WriteOp enum) but benchmark testing revealed performance regression rather than improvement. The implementation was reverted. Phase 1 write-queue architecture is production-ready for single-node workloads up to ~300 writes/s sustained.

---

## Phase 1 — Production-Ready Scope

**Appropriate for**:
- Low-to-medium write throughput (≤300 writes/s sustained)
- Single-node, file-backed SQLite
- Bounded execution history

**Not appropriate for**:
- High sustained write throughput (>500 writes/s)
- Multi-node or HA topology
- Read-replica queries
- Large execution history with complex lineage traversal

---

## Phase 2 — Deferred

Phase 2 introduced a broader batching experiment (cross-operation transaction batching via `Pipeline` variant in WriteOp). Benchmark testing showed performance regression. Implementation reverted.

**Reason**: The write queue already serializes writes effectively; batching multiple heterogeneous operations into a single transaction did not reduce lock contention and added complexity overhead.

---

## Phase 3 — PostgreSQL Migration Path

For sustained production workloads requiring materially higher write throughput or multi-node deployment, implement and migrate to PostgreSQL:
- PostgreSQL support is planned but **not implemented**; `postgres://` and `postgresql://` DSNs are rejected with an explicit ADR-50 error.
- The intended path is to add a PostgreSQL `StoreFacade` implementation behind the existing store abstraction.
- PostgreSQL is expected to remove the SQLite single-writer bottleneck once implemented and verified.
- Target throughput goals (for the future implementation): 1000+ writes/s with connection pooling.

See `docs/implementation-path/50-p4-postgres-store-facade-adr.md` for the phased implementation plan.

---

## See Also

- `docs/PRODUCTION_NOTES.md` — complete before/after stress test table
- `docs/PERFORMANCE_OPTIMIZATION_PLAN.md` — full three-phase plan
- `docs/implementation-path/27-production-evaluation-plan.md` — production evaluation framework
- `docs/implementation-path/23-production-readiness-assessment.md` — production readiness summary
