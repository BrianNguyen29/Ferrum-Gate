# 40 — Out-of-Tree SQLite Performance Candidate Evidence

> ⚠️ **OUT-OF-TREE / UNMERGED CANDIDATE** — This document captures performance
> investigation performed outside the Ferrum-Gate-git repo. These findings are
> **NOT merged repo truth** and should not be treated as implemented or validated
> within the main branch. They are documented here as candidate evidence only.

**Last updated:** 2026-04-07
**Scope:** Single-node v1 SQLite performance investigation
**Source workspace:** Local non-git workspace (outside Ferrum-Gate-git)

---

## Origin

During local workspace investigation (outside Ferrum-Gate-git repo), a Phase 1
SQLite write-queue optimization was prototyped and evaluated. Findings were
captured as candidate evidence for potential future merging into the main
branch's P2 adapter hardening track (specifically P2.2 — sqlite adapter hardening).

---

## Phase 1 — Write-Queue Optimization

### Description

Phase 1 consisted of: SQLite write queue + retry cleanup + PRAGMA tuning.

Phase 1 did **NOT** implement true cross-operation transactional batching.

A write-queue was introduced between the gateway orchestration layer and the
SQLite store (`ferrum-store`) to serialize write operations through a single
queue-backed writer path, reducing lock contention and retry churn.

### Pre-Phase-1 Baseline

| Stage | Throughput | Latency | Error Rate |
|-------|------------|---------|------------|
| S4 intent-compile | ~6.5 req/s | ~140ms | ~10% err |
| S5 execution-pipeline | ~1.1 req/s | ~958ms | ~73% err |
| S6 capability cycle | ~1.5 req/s | — | ~33% err |
| S7 sqlite-contention | ~8.1 req/s | ~3.26s | 0% err |

### Observed Results (Phase 1)

| Stage | Throughput | Latency | Error Rate |
|-------|------------|---------|------------|
| S4 intent-compile | 305.5 req/s | 2.25ms | 0% err |
| S5 execution-pipeline | 57.6 req/s | 16.0ms | 0% err |
| S6 capability cycle | 42.0 req/s | 0.30ms | 0% err |
| S7 sqlite-contention | 289.4 req/s | 29.9ms | 0% err |

**Outcome:** Strong gains observed across S4–S7 under concurrent write workloads.

### Caveats

- Results obtained in local non-git workspace; no CI validation
- Not merged into Ferrum-Gate-git main branch
- Implementation boundary and error-handling path not fully exercised
- Not reviewed by full team

---

## Phase 2 — Batching Experiment (Deferred)

### Description

Phase 2 was a broader batching/transaction experiment planned to extend
batching across broader write surfaces and evaluate scalability at higher
concurrency levels.

### Outcome

**Deferred** — Performance regression observed during early Phase 2 experiments.
The regression manifested as increased tail latency under certain write
patterns, attributed to write-queue contention under backpressure conditions.

### Decision

Phase 2 deferred pending:
1. Root cause analysis of the backpressure regression
2. Redesign of queue sizing/backpressure strategy
3. Re-validation of Phase 1 gains under more representative workloads

---

## Relationship to Repo Roadmap

In the Ferrum-Gate-git roadmap (`30-production-roadmap.md`):

- **P2.2 — sqlite adapter hardening** is listed as 🔄 IN PROGRESS
  - Slice 1: identifier safety + rollback noop tests ✅ (2026-04-04)
  - Slice 2: file-backed lifecycle + error-path tests ✅ (2026-04-04)

The write-queue optimization described above is **NOT** part of the current
P2.2 slices in the main branch. It is a separate out-of-tree investigation
that could inform a future Phase 3 of P2.2 if the regression is resolved
and the approach is validated.

### What This Means for the Roadmap

| Item | Status in Repo | Status Out-of-Tree |
|------|---------------|-------------------|
| P2.2 Slice 1 (identifier safety) | ✅ DONE (merged) | N/A |
| P2.2 Slice 2 (file-backed lifecycle) | ✅ DONE (merged) | N/A |
| P2.2 write-queue optimization | ❌ NOT IN REPO | 🔬 Phase 1 ✅ (candidate), Phase 2 deferred |

---

## Recommended Next Steps (If Merging Pursued)

1. **Root cause Phase 2 regression** — analyze backpressure contention under
   simulated high-concurrency workloads
2. **Validate Phase 1 gains independently** — reproduce S4–S7 observations
   in a controlled test environment
3. **Propose as P2.2 Slice 3** — if Phase 1 holds and Phase 2 is redesigned,
   propose as an additional slice in the roadmap with full review
4. **Do NOT merge unvalidated candidate** — Phase 1/Phase 2 findings remain
   out-of-tree until properly reviewed, tested, and merged via normal PR process

---

## Labeling Convention Used in This Doc

To ensure clarity, this document uses the following labels:

| Label | Meaning |
|-------|---------|
| **OUT-OF-TREE** | Not in Ferrum-Gate-git repo; local workspace finding |
| **UNMERGED** | Not reviewed or merged into main branch |
| **CANDIDATE** | Potential future work; requires validation before merging |
| **✅ DONE (merged)** | Verified in-repo truth on main branch |
| **🔬 Phase N** | Experimental phase in out-of-tree investigation |

---

## References

- Repo support contract: `docs/19-v1-single-node-support-contract.md`
- Repo roadmap: `docs/implementation-path/30-production-roadmap.md`
- Repo P2.2 status: `docs/implementation-path/30-production-roadmap.md` Priority 2, P2.2
- Repo sqlite adapter: `crates/ferrum-adapter-sqlite/`
- Repo store: `crates/ferrum-store/src/sqlite/`
