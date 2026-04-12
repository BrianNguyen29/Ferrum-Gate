# 46 — v2 Production Readiness Sign-off (Stage A — Pre-Ratification)

**Last updated:** 2026-04-12
**Gate:** Phase 6 — v2 Sign-off
**Status:** ⏸ PENDING — Stage A input artifact; v2 is NOT yet ratified.
v1 (`19-v1-single-node-support-contract.md`) remains the authoritative support contract.

---

## Purpose

This document is the **Stage A pre-ratification sign-off artifact** for FerrumGate v2
single-node production-ready declaration. It collects all Phase 1–5 gate evidence
into a single sign-off record, confirming that v2 is ready for formal ratification
pending Stage A review and approval.

This doc does **not** declare v2 production-ready. It records the evidence that
supports a future Stage A sign-off decision. **v2 remains DRAFT/PROPOSED until
Stage A ratification is complete.**

v1 is the current authoritative support contract. Do not treat v2 as canonical
until this doc is updated to reflect Stage A ratification.

---

## Scope of the Proposed v2 Claim

### T1 — Proposed Production-Supported

v2 extends the v1 single-node scope to include confirmed adapter T1 promotion:

- fs adapter — promoted to T1 (Phase 3, FS-1–FS-8 all ✅)
- sqlite adapter — promoted to T1 (Phase 3, SQ-1–SQ-10 all ✅)
- git adapter — promoted to T1 (Phase 5, GT-1–GT-13 all ✅)
- http adapter — promoted to T1 (Phase 5, HT-1–HT-10 all ✅)

### T2 — Confirmed Partial-Contract Level

maildraft remains T2 partial in v2 (MD-1–MD-5 all ✅); real provider send
integration is post-v2 backlog.

### T3 — Still Deferred / Out of Scope

v2 does **NOT** cover:
- multi-node / HA / read-replica
- U2/U3/U4 upgrade tracks
- broader production-verified external adapter integrations (remote git workflows beyond bounded local, external http, real mail provider send)
- policy bundle authoring / migration tooling

---

## Phase Gate Evidence Summary

| Phase | Description | Status | Evidence |
|-------|-------------|--------|----------|
| Phase 1 | v2 Scope Lock | ✅ DONE | `44-v2-production-execution-plan.md` Phase 1 |
| Phase 2 | Promotion Criteria Confirmation | ✅ DONE | G-E1–G-E5 inherited from v1 sign-off 2026-04-08; confirmed applicable to v2 scope |
| Phase 3 | fs/sqlite Adapter Promotion to T1 | ✅ DONE | FS-1–FS-8 ✅; SQ-1–SQ-10 ✅; evidence: `45-v2-adapter-promotion-criteria.md` |
| Phase 4 | U1 Core Capability + Policy Tooling | ✅ DONE | U1 core confirmed; P4.2 deferred post-v2 |
| Phase 5 | git/http Adapter Promotion to T1 | ✅ DONE | GT-1–GT-13 ✅; HT-1–HT-10 ✅; evidence: `45-v2-adapter-promotion-criteria.md` |
| Phase 6 | v2 Sign-off | ⏸ PENDING | This doc — Stage A input; awaiting ratification |

**Inherited from v1 broader production sign-off (2026-04-08):**

| Gate | Description | Status | Evidence |
|------|-------------|--------|----------|
| G-E1 | P2 adapter hardening complete | ✅ DONE 2026-04-08 | `30-production-roadmap.md` P2.1–P2.7 |
| G-E2 | P2 performance baseline established | ✅ DONE 2026-04-08 | `42-p2-performance-baseline-evidence.md` |
| G-E3 | P4 ferrumctl advanced flows complete | ✅ DONE 2026-04-08 | `bins/ferrumctl/src/main.rs` |
| G-E4 | P5 Sync-1 preflight + decision table ratified | ✅ DONE 2026-04-08 | `ferrum-sync`/`ferrum-store` sync tests |
| G-E5 | Production evaluation sign-off | ✅ DONE 2026-04-08 | `43-production-readiness-signoff.md` |

---

## Verification Inputs Used for Pre-Ratification Review

- Phase 3 adapter promotion: FS-1–FS-8, SQ-1–SQ-10 all ✅ per `45-v2-adapter-promotion-criteria.md`
- Phase 5 adapter promotion: GT-1–GT-13, HT-1–HT-10 all ✅ per `45-v2-adapter-promotion-criteria.md`
- All gates confirmed in `44-v2-production-execution-plan.md` Phase Completion Tracking
- v1 evidence base unchanged: `25-v1-single-node-rc-evidence.md`; `43-production-readiness-signoff.md`

---

## Stage A Sign-off Decision (Pending)

This section records the Stage A sign-off decision once taken.

| Decision | Status | Date | Notes |
|----------|--------|------|-------|
| Stage A review initiated | ⏸ PENDING | — | Pre-ratification input assembled |
| Stage A sign-off declared | ⬜ NOT YET | — | Awaiting formal Stage A approval |

**Stage A ratification criteria:**
- All Phase 1–5 gates ✅ confirmed
- Evidence chain complete and cross-referenced
- v1/v2 boundary clearly maintained (v1 remains authoritative until ratification)
- Support contract `20-v2-single-node-production-support-contract.md` finalized

---

## Relationship to v1 Sign-off

This v2 Stage A artifact is modeled on `43-production-readiness-signoff.md` (v1 G-E5
sign-off). It inherits all v1 evidence (G-E1–G-E5, all ✅ DONE 2026-04-08) and adds
Phase 3 and Phase 5 adapter promotion evidence.

Upon Stage A ratification, `43-production-readiness-signoff.md` remains the canonical
v1 sign-off; this doc becomes the canonical v2 sign-off. Both support contracts
(`19-v1-single-node-support-contract.md` and `20-v2-single-node-production-support-contract.md`)
will coexist with clear scope boundaries.

---

## Key References

| Topic | File | Status |
|-------|------|--------|
| v1 RC evidence | `docs/implementation-path/25-v1-single-node-rc-evidence.md` | ✅ Ratified |
| v1 broader production sign-off | `docs/implementation-path/43-production-readiness-signoff.md` | ✅ Ratified |
| v1 support contract | `docs/19-v1-single-node-support-contract.md` | **Currently authoritative** |
| v2 support contract | `docs/20-v2-single-node-production-support-contract.md` | **DRAFT — proposed, not ratified** |
| v2 execution plan | `docs/implementation-path/44-v2-production-execution-plan.md` | **DRAFT — proposed, not ratified** |
| v2 adapter promotion criteria | `docs/implementation-path/45-v2-adapter-promotion-criteria.md` | **DRAFT — proposed, not ratified** |
| v2 sign-off (this doc) | `docs/implementation-path/46-v2-readiness-signoff.md` | **⏸ PENDING — Stage A input** |
| Production roadmap | `docs/implementation-path/30-production-roadmap.md` | Contains v1 gate evidence |
| Docs governance | `docs/90-docs-governance.md` | Governance policy |
