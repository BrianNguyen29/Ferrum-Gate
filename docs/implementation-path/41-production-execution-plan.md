# 41 — Broader Production Execution Plan

**Last updated:** 2026-04-07
**Scope:** Broader production readiness (post single-node RC). Single-node v1 is RC-ready as of 2026-04-02 per `25-v1-single-node-rc-evidence.md`. This plan does not modify RC truth.

---

## Purpose

This document turns the production evaluation gates in `30-production-roadmap.md` (G-E1 through G-E5) into a concrete sequential phase plan with explicit per-phase documentation update requirements and commit/PR merge cadence.

This is a **documentation-only** artifact. No code changes are required by this plan.

---

## Phase Sequencing

Each phase (G-E1 → G-E5) is sequenced and gated. Phases are sequential; each phase must pass before the next begins.

```
G-E1 (Adapter Hardening)  ──►  G-E2 (Performance Baseline)  ──►
G-E3 (ferrumctl Advanced)  ──►  G-E4 (P5 Sync-1 Ratified)  ──►
G-E5 (Production Sign-off)
```

**Source of truth for current status:** `30-production-roadmap.md` — Priority 5 / Production Evaluation Gates.

---

## Gate G-E1 — P2 Adapter Hardening Complete

**Goal:** All P2 adapter slices (P2.1, P2.2, P2.3, P2.6, P2.7) pass their slice criteria.

**Owner:** Engineering

**Status:** 🔄 IN PROGRESS (P2.5 ✅ DONE; P2.6 scaffold ✅ DONE 2026-04-04 (real provider send integration TBD post-v1/non-blocking); P2.7 ✅ DONE; P2.1, P2.2, P2.3 🔄 IN PROGRESS)

**Note:** P2.6 scaffold completion (slices 1–5, all ✅) satisfies the current G-E1 boundary. Real provider send integration is explicitly post-v1/non-blocking and does not block G-E1.

**Source:** `30-production-roadmap.md` lines 43–49; `11-remaining-tasks.md` lines 98–119

### G-E1 Documentation Update Protocol

When G-E1 is complete:

| Step | Action | File(s) Updated | Commit Scope |
|------|--------|-----------------|--------------|
| 1 | Update `30-production-roadmap.md` G-E1 row: `🔄 IN PROGRESS` → `✅ DONE`, add date | `docs/implementation-path/30-production-roadmap.md` | Single commit |
| 2 | Update `11-remaining-tasks.md` execution sequence table: P2 adapter items → `✅ DONE`, add date | `docs/implementation-path/11-remaining-tasks.md` | Same commit |
| 3 | Record final evidence in this doc's G-E1 completion row | `docs/implementation-path/41-production-execution-plan.md` | Same commit |
| 4 | Update `docs/README.md` index if any new doc was added | `docs/README.md` | Separate commit |

**PR cadence for G-E1:**
- One PR per adapter slice (P2.1, P2.2, P2.3, P2.6) with their test evidence
- A final consolidation PR updating roadmap and this doc
- All PRs reviewed before merge; merge before starting G-E2

---

## Gate G-E2 — P2 Performance Baseline Established

**Goal:** Benchmark suite covers key SQLite and adapter paths under concurrent load.

**Owner:** Engineering

**Status:** ✅ DONE 2026-04-08

**Source:** `30-production-roadmap.md` lines 177–178; `40-out-of-tree-sqlite-performance-candidate.md` (out-of-tree — not merged)

### G-E2 Documentation Update Protocol

> **Note:** If the out-of-tree SQLite performance candidate (`40-out-of-tree-sqlite-performance-candidate.md`) is validated and merged, P2.2 Slice 3 benchmarks use that result. Otherwise, a new benchmark suite is built from scratch.

| Step | Action | File(s) Updated | Commit Scope |
|------|--------|-----------------|--------------|
| 1 | Update `30-production-roadmap.md` G-E2 row: `⬜ TODO` → `🔄 IN PROGRESS` → `✅ DONE`, add date | `docs/implementation-path/30-production-roadmap.md` | Single commit |
| 2 | Document benchmark results in a new evidence doc (e.g., `42-p2-performance-baseline-evidence.md`) | `docs/implementation-path/42-p2-performance-baseline-evidence.md` | Same commit |
| 3 | Update `11-remaining-tasks.md` if new items are discovered (minimal, grounded) | `docs/implementation-path/11-remaining-tasks.md` | Same commit |
| 4 | Record final evidence in this doc's G-E2 completion row | `docs/implementation-path/41-production-execution-plan.md` | Same commit |
| 5 | Update `docs/README.md` index if a new evidence doc was added | `docs/README.md` | Separate commit |

**PR cadence for G-E2:**
- One PR for benchmark suite implementation + results
- One PR updating roadmap and this doc (merged after benchmark PR)
- Merge before starting G-E3

---

## Gate G-E3 — P4 `ferrumctl` Advanced Flows Complete

**Goal:** Remaining operator-facing REST surface accessible via `ferrumctl` CLI. Policy bundle lifecycle tooling may land as a separate post-G-E3 scope if not required for REST-surface closure.

**Owner:** Engineering

**Status:** ✅ DONE 2026-04-08

**Source:** `30-production-roadmap.md` lines 178, 108–118; `11-remaining-tasks.md` line 112–113

### G-E3 Documentation Update Protocol

| Step | Action | File(s) Updated | Commit Scope |
|------|--------|-----------------|--------------|
| 1 | Update `30-production-roadmap.md` G-E3 row: `⬜ PLANNED` → `🔄 IN PROGRESS` → `✅ DONE`, add date | `docs/implementation-path/30-production-roadmap.md` | Single commit |
| 2 | Update P4.1 in `30-production-roadmap.md` to `✅ DONE`; explicitly defer P4.2 if kept post-G-E3 | `docs/implementation-path/30-production-roadmap.md` | Same commit |
| 3 | Update `11-remaining-tasks.md` execution sequence: P4.1 → `✅ DONE`, P4.2 defer note if applicable | `docs/implementation-path/11-remaining-tasks.md` | Same commit |
| 4 | Record final evidence in this doc's G-E3 completion row | `docs/implementation-path/41-production-execution-plan.md` | Same commit |
| 5 | Update `docs/README.md` index if a new doc was added | `docs/README.md` | Separate commit |

**PR cadence for G-E3:**
- One PR for `ferrumctl` advanced flows implementation + tests
- One PR for policy bundle tooling (if applicable)
- One consolidation PR for doc updates
- Merge before starting G-E4

---

## Gate G-E4 — P5 Sync-1 Preflight Checks + Decision Table Ratified

**Goal:** Sync-1 preflight checks (PF1–PF8) and decision table implemented and reviewed.

**Owner:** Engineering

**Status:** ⬜ PLANNED

**Source:** `30-production-roadmap.md` lines 179, 130–133; `11-remaining-tasks.md` line 114; `24-p1-p2-p3-execution-plan.md` lines 204–214

### G-E4 Documentation Update Protocol

| Step | Action | File(s) Updated | Commit Scope |
|------|--------|-----------------|--------------|
| 1 | Update `30-production-roadmap.md` G-E4 row: `⬜ PLANNED` → `🔄 IN PROGRESS` → `✅ DONE`, add date | `docs/implementation-path/30-production-roadmap.md` | Single commit |
| 2 | Update P5.4 and P5.5 rows in `30-production-roadmap.md`: `⬜ TODO` → `✅ DONE` | `docs/implementation-path/30-production-roadmap.md` | Same commit |
| 3 | Update `11-remaining-tasks.md` execution sequence: P5.4–P5.5 → `✅ DONE` | `docs/implementation-path/11-remaining-tasks.md` | Same commit |
| 4 | Update `24-p1-p2-p3-execution-plan.md` P3.1 and P3.2 rows if applicable | `docs/implementation-path/24-p1-p2-p3-execution-plan.md` | Same commit |
| 5 | Record final evidence in this doc's G-E4 completion row | `docs/implementation-path/41-production-execution-plan.md` | Same commit |
| 6 | Update `docs/README.md` index if a new doc was added | `docs/README.md` | Separate commit |

**PR cadence for G-E4:**
- One PR for Sync-1 implementation + integration tests
- One consolidation PR for doc updates
- Merge before starting G-E5

---

## Gate G-E5 — Production Evaluation Sign-off

**Goal:** Documented assessment confirming all T1/T2 surface is production-hardened per support contract.

**Owner:** Team

**Status:** ⬜ PLANNED

**Source:** `30-production-roadmap.md` lines 180, 204–205; `16-release-checklist.md`

### G-E5 Documentation Update Protocol

| Step | Action | File(s) Updated | Commit Scope |
|------|--------|-----------------|--------------|
| 1 | Create production readiness assessment doc (e.g., `43-production-readiness-signoff.md`) citing all prior evidence | `docs/implementation-path/43-production-readiness-signoff.md` | Single commit |
| 2 | Update `30-production-roadmap.md` G-E5 row: `⬜ PLANNED` → `✅ DONE`, add date | `docs/implementation-path/30-production-roadmap.md` | Same commit |
| 3 | Update `16-release-checklist.md` broader production-ready section to reflect completion | `docs/16-release-checklist.md` | Same commit |
| 4 | Record final evidence in this doc's G-E5 completion row | `docs/implementation-path/41-production-execution-plan.md` | Same commit |
| 5 | Update `docs/README.md` index if a new doc was added | `docs/README.md` | Separate commit |
| 6 | Declare broader production-ready status in `00-project-canon.md` if applicable | `docs/00-project-canon.md` | Same commit |

**PR cadence for G-E5:**
- One PR for production readiness assessment doc + all cross-doc updates
- Reviewed and merged to declare broader production-ready

---

## Per-Phase Documentation Update Checklist

When any gate phase completes, apply this checklist:

- [ ] Gate row in `30-production-roadmap.md` updated: status emoji + date + verification
- [ ] Affected P-item rows in `30-production-roadmap.md` updated to `✅ DONE`
- [ ] Execution sequence table in `11-remaining-tasks.md` updated
- [ ] This doc (`41-production-execution-plan.md`) completion row filled in
- [ ] `docs/README.md` index updated if a new doc was added
- [ ] Any new evidence doc created and linked
- [ ] Single commit made for the doc updates (multiple logical changes in one commit ok)
- [ ] One PR opened for the doc updates
- [ ] PR reviewed and merged before next phase begins

---

## Commit/PR Merge Cadence

| Phase | Commits | PRs | Merge Constraint |
|-------|---------|-----|------------------|
| G-E1 | 1 per adapter slice + 1 consolidation | 5 | All PRs merged before G-E2 starts |
| G-E2 | 1 for benchmarks + 1 consolidation | 2 | Merged before G-E3 starts |
| G-E3 | 1–2 for ferrumctl + 1 consolidation | 2–3 | Merged before G-E4 starts |
| G-E4 | 1 for Sync-1 + 1 consolidation | 2 | Merged before G-E5 starts |
| G-E5 | 1 for sign-off doc + cross-doc updates | 1 | Final — declares broader production-ready |

**General rule:** One logical scope per commit. Doc updates for a single phase may be combined into one commit. Each phase concludes with one PR merging all doc changes for that phase before the next phase begins.

---

## Phase Completion Tracking

| Gate | Phase Name | Status | Completion Date | Evidence Doc | Notes |
|------|-----------|--------|-----------------|--------------|-------|
| G-E1 | P2 Adapter Hardening Complete | ✅ DONE | 2026-04-08 | `30-production-roadmap.md` | P2.1/P2.2/P2.3/P2.5/P2.6 scaffold/P2.7 ratified; real provider send remains post-v1/non-blocking by gate definition |
| G-E2 | P2 Performance Baseline Established | ✅ DONE | 2026-04-08 | `42-p2-performance-baseline-evidence.md` | Standalone `benches/` harness merged; release baseline captured for S4-S7 concurrent workloads |
| G-E3 | P4 ferrumctl Advanced Flows Complete | ✅ DONE | 2026-04-08 | `bins/ferrumctl/src/main.rs` | Added CLI coverage for compile/evaluate/mint/authorize/verify/commit; P4.2 deferred as separate post-G-E3 scope |
| G-E4 | P5 Sync-1 Preflight + Decision Ratified | ⬜ PLANNED | — | — | |
| G-E5 | Production Evaluation Sign-off | ⬜ PLANNED | — | — | Team sign-off required |

---

## Key References

| Topic | File |
|-------|------|
| Production roadmap | `docs/implementation-path/30-production-roadmap.md` |
| Remaining tasks | `docs/implementation-path/11-remaining-tasks.md` |
| v1 RC evidence | `docs/implementation-path/25-v1-single-node-rc-evidence.md` |
| Docs governance | `docs/90-docs-governance.md` |
| Release checklist | `docs/16-release-checklist.md` |
| P1/P2/P3 execution plan | `docs/implementation-path/24-p1-p2-p3-execution-plan.md` |
