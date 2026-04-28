# 90 — Documentation Governance: Phase 1 Inventory & Canonical Map

> **Scope**: This artifact captures the Phase 1 docs inventory, canonical hierarchy, overlap/conflict matrix, and next actions for Phase 2 normalization.
> **Constraint**: Documentation-only change. No rewrites, no renumbering.

---

## 1. Doc Family Inventory

### Family A — Current Canonical Core
These are the **primary source-of-truth** docs for current v1 status. Historical roadmap-v1 planning docs remain useful context, but do not override these files.

| File | Role | Status |
|------|------|--------|
| `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` | v1 supported scope and support boundaries | Canonical for v1 support |
| `docs/implementation-path/01-current-state.md` | Current implementation/release state | Canonical current-state summary |
| `docs/implementation-path/31-release-paths-todo.md` | Path 1/2/3 routing and gate ownership | Canonical release/pilot routing |
| `docs/implementation-path/23-production-readiness-assessment.md` | Conservative readiness assessment | Canonical readiness posture |
| `docs/ferrumgate-roadmap-v1/06-constraints-and-invariants.md` | Intent/capability/taint/rollback/provenance/output/system invariants | Canonical invariants |

### Family B — Canonical Index
Entry-point docs that reference Family A but do not replace it.

| File | Role | Distinction from A |
|------|------|--------------------|
| `docs/README.md` | Primary nav index; reading order, source-of-truth priority order | Derivative index — not canonical content |
| `docs/implementation-path/README.md` | Agent handoff entry; reading order for implementation subdir | Derivative index — not canonical content |
| `docs/implementation-path/00-start-here.md` | Agent onboarding checklist | Derivative guidance — defers to current support contract/current-state docs |

### Family C — Implementation-Path Subdir
Detailed, agent-facing execution guides **derived from** Family A. Must not contradict Family A.

| File | Role | Canonical source |
|------|------|-----------------|
| `implementation-path/01-current-state.md` | Current implementation state | Current-state authority |
| `implementation-path/02-execution-order.md` | Ordered task list | `09-implementation-path.md` |
| `implementation-path/03-phase-plan.md` | Phase breakdown | `09-implementation-path.md` |
| `implementation-path/04-crate-by-crate-tasks.md` | Per-crate tasks | `10-crate-by-crate-plan.md` |
| `implementation-path/05-done-criteria.md` | Done criteria per phase | `06-constraints-and-invariants.md` |
| `implementation-path/06-guardrails-and-invariants.md` | Invariant checklist | `06-constraints-and-invariants.md` |
| `implementation-path/07-agent-handoff-prompt.md` | Handoff prompt template | Current support contract + invariants |
| `implementation-path/08-next-issue-backlog.md` | Issue backlog | — (live backlog, derivative) |
| `implementation-path/09-phase-checklists.md` | Phase checklists | `09-implementation-path.md` |
| `implementation-path/10-crate-dependency-map.md` | Crate dependency graph | `10-crate-by-crate-plan.md` |
| `implementation-path/11-remaining-tasks.md` | Remaining tasks tracker | Current-state + release paths |
| `implementation-path/12-sync-3a-probe-api-boundary.md` | API boundary spec | Derived from proto contracts |
| `implementation-path/12a-sync-3a1-read-only-transport-probe.md` | Read-only transport probe | Derived from proto contracts |
| `implementation-path/15-ferrumctl-more-useful-execution-plan.md` | ferrumctl execution plan | Derived from gateway spec |
| `implementation-path/23-production-readiness-assessment.md` | Prod readiness | Evidence/assessment |
| `implementation-path/25-EV-v1-single-node-rc-evidence.md` | RC evidence | Evidence — not canonical spec |
| `implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` | Invariant test evidence matrix | Evidence — not canonical spec |
| `implementation-path/27-production-evaluation-plan.md` | Production eval framework | Evidence/plan |

### Family D — Canonical Reference (v1 Single-Node)
v1 support contract and operational minimums.

| File | Role | Canonical source |
|------|------|-----------------|
| `../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` | v1 support contract | Canonical for v1 support |
| `../ferrumgate-roadmap-v1/20-v1-single-node-operator-checks.md` | Operator check list | Derived from support contract |
| `../ferrumgate-roadmap-v1/21-v1-single-node-observability-minimums.md` | Observability minimums | Derived from support contract |

### Family E — Roadmap Variants
Secondary planning docs; not part of the canonical v1 support chain.

| File | Role | Risk |
|------|------|------|
| `docs/ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/` | Alternate quarterly/release planning | Non-authoritative post-v1 planning reference with cross-links |

### Family F — Production Evidence (Read-Only Reference)
Production/test evidence; useful for historical record but **not canonical spec**.

| File | Role | Risk |
|------|------|------|
| `docs/PRODUCTION_NOTES.md` | Before/after stress test results | Not canonical; may contain stale benchmarks |
| `docs/PERFORMANCE_OPTIMIZATION_PLAN.md` | Three-phase perf plan (incl. SQLite write queue) | Not canonical; some phases reverted |
| `docs/91-phase-success-criteria-and-kpis.md` | Phase success KPIs | Derivative tracking |

### Family G — Out-of-Tree Candidate ⚠️

| File | Status | Risk |
|------|--------|------|
| `implementation-path/40-out-of-tree-sqlite-performance-candidate.md` | **Non-canonical / unmerged** | Explicitly marked draft. Phase 1 write-queue findings are production-tested; Phase 2 batching was reverted. Do not treat as merged canonical docs. |

> **Note**: `docs/09-implementation-path.md` does NOT reference this candidate. Any canonical treatment must go through a proper merge process.

---

## 2. Canonical Hierarchy Map

```
Source-of-truth priority order (from docs/README.md):
1.  docs/00-project-canon.md          ← project definition, hard rules
2.  docs/06-constraints-and-invariants.md ← invariants
3.  docs/09-implementation-path.md    ← phase/crate roadmap
4.  docs/10-crate-by-crate-plan.md    ← detailed tasks
5.  [all other docs/]

Index entry points:
  docs/README.md                        → reading order pointing into above
  docs/implementation-path/README.md    → reading order for subdir

Implementation subdir defers to:
  1. docs/00-project-canon.md
  2. docs/06-constraints-and-invariants.md
  3. [files in implementation-path/]
```

**Conflict resolution**: When Family C or lower contradicts Family A, Family A wins. File a Phase 2 ticket to resolve the conflict.

---

## 3. Overlap / Conflict Matrix

| Overlap hotspot | Docs involved | Nature | Severity |
|----------------|---------------|--------|----------|
| Roadmap priority | `docs/README.md` §Source-of-truth, `implementation-path/README.md` §Luật ưu tiên | Both define priority order; subdir README prepends canon/invariants before subdir files | Low — intentionally layered |
| Performance findings | `PRODUCTION_NOTES.md`, `PERFORMANCE_OPTIMIZATION_PLAN.md`, `implementation-path/40-*.md` | Three docs cover SQLite write-queue perf; Phase 2 batching reverted in `40-*.md` but not reflected in `PERFORMANCE_OPTIMIZATION_PLAN.md` | **Medium** — stale plan may mislead |
| Evidence vs spec | `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`, `25-EV-v1-single-node-rc-evidence.md` | These are evidence docs that may be read as spec by agents | Low — intent is clear, but naming is similar to canonical docs |
| Roadmap v2 vs v1 | `ferrumgate-roadmap-v2/` vs `09-implementation-path.md` | Alternate planning pack risks drift from canonical roadmap | **Medium** — unclear which is authoritative |
| Out-of-tree SQLite | `implementation-path/40-out-of-tree-sqlite-performance-candidate.md` vs canonical chain | Explicitly marked non-canonical; no link from canonical index | **Low** (risk is contained by marking) — but not merged means findings could be lost |

---

## 4. Next Actions for Phase 2 Normalization

| # | Action | Owner | Priority | Status |
|---|--------|-------|----------|--------|
| 1 | **Reconcile** `PERFORMANCE_OPTIMIZATION_PLAN.md` with reverted Phase 2 batching — mark Phase 2 as deferred/reverted | docs owner | **High** | ✅ Done — header now states Phase 2 deferred/regressed and Phase 3 unimplemented |
| 2 | **Decide authority** of `ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/` — either fold into canonical or mark deprecated | docs owner | **Medium** | ✅ Done — retained as non-authoritative post-v1/quarterly planning reference with v1 cross-links |
| 3 | **Audit Family C** files for silent drift from Family A — especially `06-guardrails-and-invariants.md` vs `06-constraints-and-invariants.md` | docs owner | **Medium** | ✅ Done — no contradiction found; implementation-path guardrails now link to full canonical invariant set |
| 4 | **Merge or retire** `40-out-of-tree-sqlite-performance-candidate.md` — if Phase 1 write-queue results are canonical, merge key findings into `PRODUCTION_NOTES.md` or a new canonical perf doc; otherwise retire | docs owner | **Medium** | ✅ Done — retired as historical reference; canonical Phase 1 findings remain in `PRODUCTION_NOTES.md` |
| 5 | **Standardize evidence-doc naming** — rename `26-*.md` and `25-*.md` to signal evidence role more clearly (e.g., `EV-25-rc-evidence.md`) | docs owner | Low | ✅ Done — evidence docs now use `25-EV-*` and `26-EV-*` names with evidence-role headers |
| 6 | **Add discoverability link** — if `40-out-of-tree-sqlite-performance-candidate.md` is eventually canonicalized, add link from `09-implementation-path.md` | docs owner | Low | ✅ Closed as not applicable — SQLite candidate retired instead of canonicalized |
| 7 | **Update current docs entrypoints** — ensure docs indexes prioritize current implementation-path/support-contract docs over superseded `00-project-canon.md` | docs owner | **High** | ✅ Done — `docs/README.md`, `implementation-path/README.md`, and `00-start-here.md` now route to current docs first |

---

## 5. Out-of-Tree SQLite Candidate — Explicit Risk Callout

**File**: `docs/implementation-path/40-out-of-tree-sqlite-performance-candidate.md`

**Risk classification**: `NON-CANONICAL / UNMERGED`

**What it contains**:
- Phase 1 write-queue + PRAGMA tuning results (production-tested, high confidence)
- Phase 2 batching experiment (reverted, benchmark regression)

**Why it is non-canonical**:
- Not linked from `docs/09-implementation-path.md` or any canonical index
- Explicitly marked as draft/out-of-tree in its own header
- Has no entry in `docs/README.md` reading order

**Risk if treated as canonical**: Phase 1 findings may be duplicated or lost when the doc is eventually merged or retired. Phase 2 reverted content may mislead future agents.

**Required action**: Phase 2 merge or retirement decision (see Next Actions #4).
