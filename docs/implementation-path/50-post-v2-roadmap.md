# 50 — Post-v2 Roadmap

**Last updated:** 2026-04-12
**Status:** PLANNED — post-v2 backlog, not yet started
**Owner:** Engineering
**Scope:** Post-ratification FerrumGate capability expansion. v1 is current authority;
v2 is DRAFT/PROPOSED pending Stage A sign-off. This roadmap is grounded in the
post-v1 backlog from `11-remaining-tasks.md` and the Priority 5/6 tracks in
`30-production-roadmap.md`.

---

## Purpose

This document splits the post-v1 backlog into explicit Horizons so that:
1. Near-term post-v2 work is clearly separated from long-term/deferred work
2. The backlog is materially clearer and less mixed
3. Roadmap docs (`30-production-roadmap.md`) remain internally consistent and non-overclaiming

**What this does NOT do:**
- Does not ratify v2 (Stage A sign-off is pending)
- Does not change v1 current authority
- Does not create new commitments — Horizons describe rough sequencing, not promises

**Source of truth for v1/v2 status:** `00-project-canon.md` Section "v2 planning status";
`46-v2-readiness-signoff.md` Stage A decision table.

---

## Horizon Map

```
v1 (authoritative)  →  v2 (DRAFT — pending Stage A sign-off)
                                         │
                    ┌────────────────────┴────────────────────┐
                    │         Post-v2 Backlog                  │
                    │                                           │
              Horizon 1               Horizon 2          Horizon 3
         (near-term post-v2)     (next capability)    (long-term/deferred)
              ~3–6 months            ~6–12 months           ~12+ months
```

---

## Horizon 1 — Near-term Post-v2 (Horizon H1)

**Goal:** Complete what v1 deferred and expand the v2 single-node foundation.

Items below are explicitly deferred from v1/v2 scope. They are the closest to
ready for execution because they build on the v1/v2 single-node base without
requiring new architectural work.

### H1.1 — Policy Bundle Lifecycle Tooling (P4.2)

- **Source:** `30-production-roadmap.md` Priority 4 (P4.2); `11-remaining-tasks.md` line 124
- **What:** CLI authoring workflows for policy bundle creation, migration, and versioning
- **Status:** ⏸ DEFERRED (post-G-E3) — separate scope required
- **Constraint:** Requires v2 sign-off before starting; does not block v2 ratification

### H1.2 — U1 Remaining Backlog (Expressiveness + Authoring Tooling)

- **Source:** `11-remaining-tasks.md` lines 88-91; `30-production-roadmap.md` Priority 6 (U1.1/U1.2)
- **What:** Two distinct sub-items:
  - **H1.2a:** Richer outcome clause expressiveness — nested selectors, temporal constraints
  - **H1.2b:** Policy bundle authoring CLI (distinct from H1.1 bundle lifecycle tooling — focuses on intent/policy creation rather than migration)
- **Status:** ⬜ PLANNED
- **Note:** U1 core (S1–S8a) is ✅ DONE in v1/v2 scope. H1.2 covers the remaining expressiveness backlog and the authoring CLI gap.

### H1.3 — git Adapter — Deeper Remote Integration Hardening

- **Source:** `11-remaining-tasks.md` lines 83-87; `00-project-canon.md` line 88
- **What:** Beyond the bounded local push/fetch/pull (P2.4 ✅ DONE), expand to:
  - Authenticated remote support (HTTPS with credentials, SSH with key)
  - Non-temporary remote tracking (persist remote config across executions)
  - Multi-remote support and remote mirroring
- **Status:** ⬜ PLANNED
- **Note:** Current P2.4 git remote workflows use local temporary remotes only. Broader remote/external workflows are post-v1 backlog.

### H1.4 — fs/sqlite — Broader Production-Verified Integration

- **Source:** `00-project-canon.md` line 88; `11-remaining-tasks.md` lines 79-83
- **What:** Beyond bounded local T2→T1 current state (per `30-production-roadmap.md` P2.1/P2.2):
  - fs: networked/storage-area-attached filesystem integration, permission boundary hardening
  - sqlite: WAL-mode production tuning, backup/restore automation, larger-than-memory dataset handling
- **Status:** ⬜ PLANNED

### H1.5 — http Adapter — Broader External HTTP Integration

- **Source:** `00-project-canon.md` line 88; `11-remaining-tasks.md` lines 79-83
- **What:** Beyond bounded local HTTP (P2.5 ✅ DONE):
  - Client certificate / mTLS authentication
  - OAuth2 / bearer token refresh flows
  - Retry/backoff with idempotency key management
- **Status:** ⬜ PLANNED

---

## Horizon 2 — Next Capability (Horizon H2)

**Goal:** HA-ready topology and the first of the U2 upgrade track.

Items in H2 require more significant architectural investment than H1 items.
They build on H1 work or require new runtime capability.

### H2.1 — HA / Multi-Leader Replication (P5.7)

- **Source:** `30-production-roadmap.md` Priority 5 (P5.7); `11-remaining-tasks.md` line 131
- **What:** Multi-node v1 with HA-ready topology. Work items:
  - Leader-election implementation
  - Write-ahead log replication
  - Conflict resolution for concurrent mutations
  - Read-replica routing
- **Status:** ⬜ PLANNED (post-P2)
- **Note:** P5.1–P5.6 analysis/design docs exist; P5.7 implementation is pending.
- **Depends on:** H1.4 (sqlite production hardening) for WAL replication design

### H2.2 — U2 Reversible Execution Planner

- **Source:** `11-remaining-tasks.md` line 93; `30-production-roadmap.md` Priority 6 (U2)
- **What:** Extends the current rollback-contract model with:
  - Full execution reversal with dependency-aware planning
  - Multi-step compensation graph optimization
  - Cross-adapter rollback sequencing
- **Status:** ⬜ PLANNED
- **Design doc:** `91-phase-success-criteria-and-kpis.md` section 8.2

---

## Horizon 3 — Long-term / Deferred (Horizon H3)

**Goal:** Capability infrastructure that requires significant architectural investment
or market-driver alignment before starting.

Items in H3 are explicitly out of the v1/v2 single-node scope. They represent
the full FerrumGate vision and are tracked here for completeness, not as near-term
work.

### H3.1 — U3 Cross-runtime Provenance Fabric

- **Source:** `11-remaining-tasks.md` line 96; `30-production-roadmap.md` Priority 6 (U3)
- **What:** Unified provenance graph spanning multiple FerrumGate runtimes and adapter
  boundaries:
  - Cross-runtime lineage correlation
  - Trust federation across runtime boundaries
  - Provenance query API across runtime mesh
- **Status:** ⬜ PLANNED
- **Design doc:** `91-phase-success-criteria-and-kpis.md` section 8.3
- **Note:** Requires multi-runtime context; not achievable in single-node v1/v2 scope

### H3.2 — U4 Runtime Integrations (MCP / local / NemoClaw)

- **Source:** `11-remaining-tasks.md` line 99; `30-production-roadmap.md` Priority 6 (U4)
- **What:** Deep MCP (Model Context Protocol) integration and local tool runtime binding:
  - MCP tool descriptor registration and capability minting
  - NemoClaw local agent runtime binding
  - MCP-first FerrumGate operator surface
- **Status:** ⬜ PLANNED
- **Design doc:** `91-phase-success-criteria-and-kpis.md` section 8.4

### H3.3 — Real Mail Provider Send Integration

- **Source:** `11-remaining-tasks.md` lines 81-82; `00-project-canon.md` line 89
- **What:** EmailSend adapter real provider integration (beyond the P2.6 scaffold ✅):
  - SMTP submission with retry
  - DKIM / SPF signing
  - External mail provider API integration (SendGrid, Postmark, SES)
- **Status:** ⬜ PLANNED
- **Note:** P2.6 scaffold was G-E1 boundary-satisfying; real provider send is post-v1/v2 backlog

### H3.4 — Multi-Node Distributed Deployment (beyond HA)

- **Source:** `00-project-canon.md` line 90; `11-remaining-tasks.md` line 86
- **What:** Full distributed deployment beyond HA multi-leader:
  - Sharded execution state
  - Geo-distributed intent routing
  - Tenant isolation in multi-tenant SaaS
- **Status:** ⬜ PLANNED
- **Note:** H2.1 (HA) is the precursor. This item is for after H2.1 is complete.

---

## Execution Notes

### Execution Order

The recommended execution order (not a commitment) follows dependency and value:
1. **H1.1** (Policy bundle tooling) — operator-facing, unblocks advanced authoring
2. **H1.2** (U1 expressiveness + authoring CLI) — builds on U1 core ✅
3. **H2.1** (HA/multi-node) — infrastructure; high value but higher cost
4. **H2.2** (U2 Reversible Execution Planner) — core capability expansion
5. **H1.3/H1.4/H1.5** (deeper adapter hardening) — can be interleaved with H2.1/H2.2
6. **H3.1–H3.4** — long-term; start after H2 work is underway

### Relationship to v2 Execution Plan

This roadmap is orthogonal to the v2 execution plan (`44-v2-production-execution-plan.md`).
v2 is a ratification target (single-node, production-verified) with its own phase
plan (Phases 1–6). This roadmap begins where v2 ends — it is the **post-v2 backlog
structure**, not part of the v2 ratification path.

### Backlog Classification Principles

- **Horizon 1 (H1):** Builds on existing v1/v2 single-node base. Does not require new runtime architecture. Near-term value.
- **Horizon 2 (H2):** Requires significant architectural investment or new runtime capability. Medium-term.
- **Horizon 3 (H3):** Requires market-driver alignment or multi-runtime context. Long-term / deferred.

### Cross-references

| Item | Source doc | Notes |
|------|-----------|-------|
| H1.1 Policy bundle tooling | `30-production-roadmap.md` P4.2 | ⏸ DEFERRED post-G-E3 |
| H1.2 U1 remaining backlog | `11-remaining-tasks.md` lines 88-91 | U1 core ✅ DONE |
| H1.3 git deeper integration | `11-remaining-tasks.md` lines 83-87 | P2.4 ✅ bounded local |
| H2.1 HA / multi-leader | `30-production-roadmap.md` P5.7 | post-P2 |
| H2.2 U2 Reversible Planner | `11-remaining-tasks.md` line 93 | |
| H3.1 U3 Provenance Fabric | `11-remaining-tasks.md` line 96 | |
| H3.2 U4 Runtime Integrations | `11-remaining-tasks.md` line 99 | |
| H3.3 Mail send provider | `11-remaining-tasks.md` lines 81-82 | P2.6 scaffold ✅ |
| H3.4 Multi-node distributed | `00-project-canon.md` line 90 | after H2.1 |

---

## Key References

| Topic | File | Status |
|-------|------|--------|
| v1 support contract (current authority) | `docs/19-v1-single-node-support-contract.md` | ✅ RATIFIED |
| v2 support contract (DRAFT) | `docs/20-v2-single-node-production-support-contract.md` | DRAFT — not ratified |
| v2 execution plan | `docs/implementation-path/44-v2-production-execution-plan.md` | Phase 1–6; v2 DRAFT |
| v2 sign-off (pending) | `docs/implementation-path/46-v2-readiness-signoff.md` | ⏸ PENDING Stage A |
| Production roadmap (v1/v2) | `docs/implementation-path/30-production-roadmap.md` | Contains P1–P6 status |
| Remaining tasks (backlog) | `docs/implementation-path/11-remaining-tasks.md` | Source for this roadmap |
| v1 RC evidence | `docs/implementation-path/25-v1-single-node-rc-evidence.md` | ✅ RATIFIED |