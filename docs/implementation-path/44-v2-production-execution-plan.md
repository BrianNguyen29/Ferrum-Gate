# 44 — v2 Production Execution Plan

**Last updated:** 2026-04-12
**Status:** ✅ RATIFIED — All phases complete. v2 single-node production-ready declaration.
**Scope:** FerrumGate v2 single-node production-ready target. Grounded in the
v1 RC evidence base (`25-v1-single-node-rc-evidence.md`) and v1 broader production
declaration (`43-production-readiness-signoff.md`), extended with Phase 3 and Phase 5
adapter promotion evidence (`45-v2-adapter-promotion-criteria.md`). This plan confirms
v2 ratification upon successful completion of all phases.

---

## Purpose

This document turns the v2 production scope into a concrete sequential phase plan
with explicit per-phase documentation update requirements. It defines what "v2"
means, what promotion criteria must be met, and how each phase transitions to the
next. v2 means single-node, production-verified — not multi-node/HA.

This is a **documentation-only** artifact. No code changes are required by this plan
unless a phase explicitly calls out a verification gap.

---

## What "v2" Means

v2 is a **scope and verification label**, not a code freeze or version bump. The
v2 label covers:

1. All v1 single-node scope (RC-ready as of 2026-04-02 per `25-v1-single-node-rc-evidence.md`)
2. Adapter hardening confirmed across fs, sqlite, git, http, maildraft (P2.1/P2.2/P2.3/P2.5/P2.7 ✅)
3. `ferrumctl` advanced operator flows complete (P4.1 ✅)
4. Sync-1 preflight checks + decision table ratified (P5.4/P5.5 ✅)
5. Broader production-ready declared after G-E5 (2026-04-08 per `43-production-readiness-signoff.md`)
6. fs/sqlite/git/http/maildraft bounded local implementations confirmed at T2 partial-contract level

v2 does **NOT** cover:
- Multi-node / HA / read-replica
- U2/U3/U4 upgrade tracks
- Broader production-verified external adapter integrations (remote git, external http, real mail provider send)
- Policy bundle authoring / migration tooling

---

## Phase Sequencing

```
Phase 1 (Scope Lock)  ──►  Phase 2 (Promotion Criteria)  ──►
Phase 3 (fs/sqlite promotion)  ──►  Phase 4 (U1+policy tooling)  ──►
Phase 5 (git/http promotion)  ──►  Phase 6 (v2 Sign-off)
```

Phases are sequential. Each phase must pass before the next begins.

**Source of truth for current status:** `30-production-roadmap.md` — v1 G-E gates
are ✅ DONE as of 2026-04-08. Phases 1–6 below document the planned progression
toward the v2 production-ready target.

---

## Phase 1 — v2 Scope Lock

**Goal:** Confirm v2 scope definition, verify it does not overclaim, and lock the
canonical support boundary.

**Owner:** Engineering

**Status:** ✅ DONE — Scope locked 2026-04-12

**What this phase confirms (scope-lock checkpoint ✅):**
- v2 = v1 + adapter hardening (P2.1/P2.2/P2.3/P2.5/P2.7) + ferrumctl advanced (P4.1) +
  Sync-1 ratified (P5.4/P5.5) + broader production-ready declaration (G-E5)
- No multi-node/HA claims in v2 scope
- T3 items (multi-node, U2/U3/U4, SLA guarantees) remain out of scope

**Scope-lock checkpoint note:** Phase 1 scope-confirmation work is confirmed. v2 scope
definition is locked and verified against prior v1 evidence. With Phase 6 sign-off complete,
v2 is now RATIFIED.

**Source:** `30-production-roadmap.md` Priority 1–6 status; `43-production-readiness-signoff.md`;
`11-remaining-tasks.md` P3.G live evidence (G-E1 through G-E5 all ✅)

### Phase 1 Documentation Update Protocol

| Step | Action | File(s) Updated |
|------|--------|-----------------|
| 1 | Confirm v2 scope definition in this doc | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 2 | Confirm no T3 overclaim in v2 scope | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 3 | Index new v2 docs in `docs/README.md` | `docs/README.md` |
| 4 | Add v2 support contract and execution plan to docs governance inventory | `docs/90-docs-governance.md` |

---

## Phase 2 — Promotion Criteria Confirmation

**Goal:** Confirm that the G-E gates from the v1 broader production-ready declaration
(G-E1–G-E5, all ✅ DONE 2026-04-08) apply to v2 scope. This is a **checklist
confirmation checkpoint** — no new gates are created here. The per-adapter promotion
gates (FS-1–FS-8, SQ-1–SQ-10, GT-1–GT-13, HT-1–HT-10) are confirmed in Phases 3 and 5.
v2 ratification itself occurs at Phase 6.

**Owner:** Engineering

**Status:** ✅ DONE — G-E1–G-E5 inherited from v1 broader production sign-off 2026-04-08;
confirmed as applicable to v2 scope in this checkpoint.

**Inherited evidence (G-E1–G-E5 — from v1 broader production sign-off, 2026-04-08):**

| Gate | Description | v1 Status | Evidence |
|------|-------------|-----------|----------|
| G-E1 | P2 adapter hardening complete | ✅ DONE 2026-04-08 | P2.1/P2.2/P2.3/P2.5/P2.6 scaffold/P2.7 ✅; `30-production-roadmap.md` lines 43-49 |
| G-E2 | P2 performance baseline established | ✅ DONE 2026-04-08 | `42-p2-performance-baseline-evidence.md`; `30-production-roadmap.md` line 178 |
| G-E3 | P4 ferrumctl advanced flows complete | ✅ DONE 2026-04-08 | `bins/ferrumctl/src/main.rs`; `30-production-roadmap.md` line 179 |
| G-E4 | P5 Sync-1 preflight + decision table ratified | ✅ DONE 2026-04-08 | `ferrum-sync`/`ferrum-store` sync tests; `30-production-roadmap.md` line 179 |
| G-E5 | Production evaluation sign-off | ✅ DONE 2026-04-08 | `43-production-readiness-signoff.md` |

**Per-adapter promotion gates (confirmed in Phases 3 and 5, not Phase 2):**

- Phase 3: fs (FS-1–FS-8) and sqlite (SQ-1–SQ-10) → T1 promotion gates
- Phase 5: git (GT-1–GT-13) and http (HT-1–HT-10) → T1 promotion gates
- Full per-adapter gate sheet: `45-v2-adapter-promotion-criteria.md`

**Sign-off status:** Phase 6 (v2 Sign-off) is ✅ RATIFIED as of 2026-04-12 — all phases complete.
Phase 2 is the pre-Phase-3 checklist confirming inherited gates apply to v2 scope.

**Source:** `30-production-roadmap.md` Phase Completion Tracking (lines 204–210);
`43-production-readiness-signoff.md`; `45-v2-adapter-promotion-criteria.md`

### Phase 2 Documentation Update Protocol

| Step | Action | File(s) Updated |
|------|--------|-----------------|
| 1 | Confirm all G-E gates pass in this doc | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 2 | Cross-reference phase completion tracking | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 3 | Note T2→T1 adapter promotion criteria doc exists | `docs/implementation-path/45-v2-adapter-promotion-criteria.md` |

---

## Phase 3 — fs/sqlite Adapter Promotion to T1

**Goal:** Confirm fs and sqlite adapters have met the T2 partial-contract hardening
level and are verified for bounded local implementations per v2 scope.

**Owner:** Engineering

**Status:** ✅ DONE — FS-1–FS-8 and SQ-1–SQ-10 all confirmed ✅ per `45-v2-adapter-promotion-criteria.md`

**What this phase confirms:**
- fs adapter: fail-closed verify on I/O errors ✅; compensate deletes new file when no snapshot ✅;
  fail-closed compensate/rollback on I/O error during recovery ✅; gateway-level verify hash
  mismatch → Failed → commit rejected ✅; gateway-level rollback drill after verify false ✅;
  gateway-level compensate drill after verify false ✅
- sqlite adapter: identifier safety + rollback noop tests ✅; file-backed lifecycle + error-path
  tests ✅; fail-closed verify on DB-open error ✅; fail-closed compensate/rollback on DB error
  during recovery ✅; fail-closed verify on DB-corruption mid-operation ✅; gateway-level verify
  false → Failed → commit rejected ✅; gateway-level rollback drill after verify false ✅;
  gateway-level compensate drill after verify false ✅
- fs before_hash/after_hash wiring: PR #165 confirmed in `artifacts/2026-04-09/closure-note.txt`

**Ratification basis:** All adapter promotion gates verified ✅ in `45-v2-adapter-promotion-criteria.md`.
Evidence: `30-production-roadmap.md` P2.1 (line 43), P2.2 (line 44); `11-remaining-tasks.md`
lines 110–112; `45-v2-adapter-promotion-criteria.md` (FS-1–FS-8, SQ-1–SQ-10)

### Phase 3 Documentation Update Protocol

| Step | Action | File(s) Updated |
|------|--------|-----------------|
| 1 | Confirm P2.1 and P2.2 all slices done in this doc | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 2 | Note fs/sqlite bounded local implementation confirmed | `docs/20-v2-single-node-production-support-contract.md` Section 2.2 |
| 3 | Index adapter hardening evidence if not already indexed | `docs/README.md` (already indexed) |
| 4 | Confirm fs and sqlite promotion gates all ✅ in criteria doc | `docs/implementation-path/45-v2-adapter-promotion-criteria.md` |

---

## Phase 4 — U1 Core Capability + Policy Tooling

**Goal:** Confirm U1 core capability is materially mature for v2 single-node scope.
Policy bundle authoring/migration tooling remains post-v2 backlog.

**Owner:** Engineering

**Status:** ✅ DONE — U1 core maturity narrative-confirmed; P4.2 deferred post-v2

**What this phase confirms (narrative confirmation basis):**
- U1-S1 through U1-S8 core capability: evaluate-time allowed-outcome mismatch warn,
  explicit forbidden-outcome match deny; U1-S2 verify-time annotation persisted;
  U1-S3a multi-signal inference; U1-S3b confidence-thresholded verify annotations;
  U1-S4a/b higher-fidelity outcome contracts; U1-S5a/b soft/hard gate; U1-S6 selector-aware
  effective match; U1-S7a list-based selector matching; U1-S8a operator compile-time ergonomics
- Remaining U1 backlog: richer outcome clause expressiveness (nested selectors, temporal
  constraints); policy bundle migration tooling
- P4.2 policy bundle lifecycle tooling: deferred post-G-E3 per `30-production-roadmap.md` line 124

**Ratification basis:** U1 core maturity is narrative-confirmed based on existing implementation evidence
in `11-remaining-tasks.md` lines 88–91 and `30-production-roadmap.md` Priority 6. This is an
explicitly bounded narrative confirmation accepted for v2 ratification — detailed per-criterion
checklist is not required for U1 core capability per the v2 scope definition. Full per-criterion
evidence is documented in `91-phase-success-criteria-and-kpis.md` section 8.1 for reference.

**Source:** `11-remaining-tasks.md` lines 88–91; `30-production-roadmap.md` Priority 6;
`91-phase-success-criteria-and-kpis.md` section 8.1

### Phase 4 Documentation Update Protocol

| Step | Action | File(s) Updated |
|------|--------|-----------------|
| 1 | Confirm U1 core maturity in this doc | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 2 | Confirm U1 remaining backlog (expressiveness, tooling) remains post-v2 | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 3 | Confirm P4.2 policy bundle tooling deferred | `docs/20-v2-single-node-production-support-contract.md` Section 2.3 |

---

## Phase 5 — git/http Adapter Promotion to T1

**Goal:** Confirm git and http adapters have met the T2 partial-contract hardening level
for bounded local implementations per v2 scope.

**Owner:** Engineering

**Status:** ✅ DONE — GT-1–GT-13 and HT-1–HT-10 all confirmed ✅ per `45-v2-adapter-promotion-criteria.md`

**What this phase confirms:**
- git adapter: fail-closed verify on I/O errors + noop edge-case tests ✅; GitBranchCreate
  prepare fails closed on detached HEAD ✅; GitPush rollback no-op when no pre_push_ref ✅;
  GitFetch rollback restores existing local ref ✅; GitPull compensate/rollback fail-closed
  when branch changed since prepare/execute ✅; gateway-level verify false → Failed → commit
  rejected ✅; GitPush/GitFetch rollback fail-closed when recovery force-push/force-update fails ✅;
  gateway-level rollback drill after verify false ✅; gateway-level compensate drill after verify false ✅
- git remote workflows: GitPush ✅, GitFetch ✅, GitPull fast-forward-only ✅ (P2.4)
- http adapter: fail-closed on transport errors (connection-refused, timeout) ✅; explicit
  check mismatch/matches ✅; gateway-level verify false → Failed → commit rejected ✅

**Note:** maildraft adapter (P2.6 scaffold + P2.7) is also confirmed at T2 partial-contract
level. Real provider send integration is post-v2/non-blocking per G-E1 gate definition.

**Ratification basis:** All adapter promotion gates verified ✅ in `45-v2-adapter-promotion-criteria.md`.
Evidence: `30-production-roadmap.md` P2.3 (line 45), P2.4 (line 46), P2.5 (line 47); `11-remaining-tasks.md`
lines 113–116; `45-v2-adapter-promotion-criteria.md` (GT-1–GT-13, HT-1–HT-10)

### Phase 5 Documentation Update Protocol

| Step | Action | File(s) Updated |
|------|--------|-----------------|
| 1 | Confirm P2.3, P2.4, P2.5 all slices done in this doc | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 2 | Note git/http bounded local implementation confirmed | `docs/20-v2-single-node-production-support-contract.md` Section 2.2 |
| 3 | Note maildraft scaffold confirmed (real provider send post-v2) | `docs/20-v2-single-node-production-support-contract.md` Section 2.2 |
| 4 | Confirm git and http promotion gates all ✅ in criteria doc | `docs/implementation-path/45-v2-adapter-promotion-criteria.md` |

---

## Phase 6 — v2 Sign-off

**Goal:** Confirm all v2 phases complete and declare v2 single-node production-ready
with explicit T1/T2/T3 boundary. Publish v2 support contract.

**Owner:** Team

**Status:** ✅ DONE — v2 ratification complete; all phases confirmed; v2 support contract finalized

**What this phase produces (all complete):**
- `20-v2-single-node-production-support-contract.md` — canonical v2 support contract ✅ RATIFIED
- `44-v2-production-execution-plan.md` — this document ✅ RATIFIED
- `46-v2-readiness-signoff.md` — v2 sign-off artifact ✅ RATIFIED
- Updated `docs/README.md` index for new docs ✅ DONE
- Updated `docs/90-docs-governance.md` inventory for new docs ✅ DONE

**Source:** `19-v1-single-node-support-contract.md` (v1 pattern); `43-production-readiness-signoff.md`
(v1 broader production sign-off pattern)

### Phase 6 Documentation Update Protocol

| Step | Action | File(s) Updated |
|------|--------|-----------------|
| 1 | Create v2 support contract | `docs/20-v2-single-node-production-support-contract.md` |
| 2 | Create v2 execution plan | `docs/implementation-path/44-v2-production-execution-plan.md` |
| 3 | Update `docs/README.md` Fast Status Index | `docs/README.md` |
| 4 | Update `docs/90-docs-governance.md` inventory | `docs/90-docs-governance.md` |
| 5 | Confirm phase completion in `30-production-roadmap.md` Phase Completion Tracking | `docs/implementation-path/30-production-roadmap.md` |

---

## Per-Phase Documentation Update Checklist

When any phase completes, apply this checklist:

- [ ] Phase row in this doc updated: status ✅ DONE + date
- [ ] Cross-ref updated in `docs/implementation-path/30-production-roadmap.md` if applicable
- [ ] `docs/README.md` index updated if a new doc was added
- [ ] `docs/90-docs-governance.md` inventory updated if a new doc was added
- [ ] New evidence doc created and linked if applicable
- [ ] Single commit made for the doc updates (multiple logical changes in one commit ok)

---

## Phase Completion Tracking

| Phase | Phase Name | Status | Target Completion | Exit Criteria | Evidence Doc |
|-------|-----------|--------|-------------------|--------------|--------------|
| Phase 1 | v2 Scope Lock | 🔒 SCOPE LOCKED ✅ | 2026-04-12 | v2 scope definition locked; no T3 overclaim | `44-v2-production-execution-plan.md` |
| Phase 2 | Promotion Criteria Confirmation | ✅ DONE | 2026-04-08 | G-E1–G-E5 confirmed for v2 scope; adapter criteria doc indexed | `30-production-roadmap.md` lines 204–210; `45-v2-adapter-promotion-criteria.md` |
| Phase 3 | fs/sqlite Adapter Promotion | ✅ DONE | 2026-04-12 | P2.1 and P2.2 slices confirmed; FS-1–FS-8 and SQ-1–SQ-10 all ✅ | `30-production-roadmap.md` P2.1/P2.2; `45-v2-adapter-promotion-criteria.md` |
| Phase 4 | U1 Core Capability + Policy Tooling | ✅ DONE (narrative-confirmed) | 2026-04-12 | U1 core maturity narrative-confirmed; P4.2 deferred post-v2 | `11-remaining-tasks.md` line 88; `30-production-roadmap.md` P4.2 |
| Phase 5 | git/http Adapter Promotion | ✅ DONE | 2026-04-12 | P2.3, P2.4, P2.5, P2.6, P2.7 confirmed; GT-1–GT-13 and HT-1–HT-10 all ✅ | `30-production-roadmap.md` P2.3/P2.4/P2.5; `45-v2-adapter-promotion-criteria.md` |
| Phase 6 | v2 Sign-off | ✅ DONE — v2 RATIFIED | 2026-04-12 | All phases complete; v2 support contract ratified | `20-v2-single-node-production-support-contract.md`; `46-v2-readiness-signoff.md` |

> **Note:** Phase 2 "✅ DONE (inherited G-E gates)" reflects v1 production-readiness (G-E gates through 2026-04-08).
> All other phases are confirmed complete as of 2026-04-12.

---

## Key References

| Topic | File | Notes |
|-------|------|-------|
| v1 RC evidence | `docs/implementation-path/25-v1-single-node-rc-evidence.md` | v1 ratified |
| v1 broader production sign-off | `docs/implementation-path/43-production-readiness-signoff.md` | v1 ratified |
| v2 support contract | `docs/20-v2-single-node-production-support-contract.md` | **v2 ✅ RATIFIED** |
| v2 execution plan (this doc) | `docs/implementation-path/44-v2-production-execution-plan.md` | **v2 ✅ RATIFIED** |
| v2 sign-off | `docs/implementation-path/46-v2-readiness-signoff.md` | **v2 ✅ RATIFIED** |
| v2 adapter promotion criteria | `docs/implementation-path/45-v2-adapter-promotion-criteria.md` | v2 ✅ RATIFIED — all gates confirmed |
| Production roadmap | `docs/implementation-path/30-production-roadmap.md` | Contains v1 gate evidence |
| Remaining tasks | `docs/implementation-path/11-remaining-tasks.md` | v1 + post-v1 backlog |
| v1 support contract | `docs/19-v1-single-node-support-contract.md` | v1 ✅ RATIFIED (superseded by v2 for v2 scope) |
| v1 execution plan | `docs/implementation-path/41-production-execution-plan.md` | v1 ratified plan |
| Docs governance | `docs/90-docs-governance.md` | Governance policy |
| Release checklist | `docs/16-release-checklist.md` | Release process |