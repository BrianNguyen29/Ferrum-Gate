# 00a — Domainless Readiness Tier Model

> **Status**: Tier 1 COMPLETE / ACKNOWLEDGED. This doc remains the canonical tiered readiness model definition.
> **Owner**: Engineering
> **Last updated**: 2026-05-27
> **Parent**: [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md)
> **Completion tracker**: [`docs/production-readiness-v2/12-domainless-completion-status.md`](./12-domainless-completion-status.md)

---

## Goal

Introduce a safe, explicit tiered readiness model that lets FerrumGate advance from RC-ready/conditional to domainless intermediate milestones **without** conflating those milestones with the legacy `production-ready` claim. Legacy `production-ready` remains a Tier 2 gate that requires real domain + revalidation + full G2 re-signoff.

This doc is the canonical definition. All other docs must reference it when describing Tier 0, Tier 1, optional Tier 1.5, or Tier 2.

---

## Tiered model

| Tier | Name | What it means | Gated by |
|------|------|---------------|----------|
| **Tier 0** | RC-ready / conditional | Single-node SQLite pilot with operator conditional signoff. DuckDNS accepted. Engineering evidence exists for bounded local and target runs. | — (current state) |
| **Tier 1** | Domainless production-candidate | All engineering hardening for B (domainless readiness semantics), C (PostgreSQL local hardening/sustained workload), and HA-B (local Docker HA/failover simulation) is complete. System is a credible production candidate **except** for real owned domain and live production operations. No real domain required at this tier. | B + C + HA-B evidence complete; operator acknowledgment |
| **Tier 1.5** | Domainless production infrastructure complete | Optional, final intermediate tier. PostgreSQL production deployment, HA multi-node topology, and automated failover are complete in an operator environment. System is a credible production candidate **except** for real owned domain and sustained SLO window. `production-ready = NO` remains explicit. | PG production + HA multi-node + automated failover evidence complete; operator acknowledgment |
| **Tier 2** | Production-ready / domain-backed | Full production-ready claim. Requires real owned domain, revalidation, sustained SLO window, full G2 re-signoff, and operator final signoff. | Real domain + revalidation + sustained SLO + full G2 + operator final signoff |

---

## Tier 1 (domainless production-candidate) definition

Tier 1 is **not** production-ready. It is an intermediate milestone that says:

- Engineering work for the selected B+C+HA-B scope is complete.
- Evidence artifacts for that scope exist and are reviewable.
- The system is a credible candidate for production deployment **once** a real domain is added and Tier 2 gates are satisfied.
- `production-ready = NO` remains true at Tier 1.

### What Tier 1 includes (B+C+HA-B scope)

- **B — Domainless readiness semantics**: Tier 1 uses the `domainless production-candidate` label instead of weakening the legacy Tier 2 `production-ready` definition. Block A remains WAIVED/CONDITIONAL.
- **C — PostgreSQL local hardening**: Local Docker PostgreSQL drills cover migration, restore, backup/retention/offsite, deterministic resume, scheduled-timer simulation, and a bounded sustained workload with PG pool metrics/readiness checks.
- **HA-B — Local HA/failover simulation**: Local Docker PostgreSQL primary/standby streaming replication setup and manual promotion drill pass with local RPO/RTO measurement. This is **not** a production HA or automated failover claim.

### What Tier 1 explicitly does NOT include

| Non-claim | Status at Tier 1 |
|-----------|------------------|
| **production-ready** | **NO** — Tier 1 is not production-ready. |
| **full G2** | **NOT COMPLETE** — G2.1–G2.8 remain signed for conditional pilot only. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain still required for Tier 2. |
| **PostgreSQL production** | **NO** — local Docker/runtime support exists; production PG deployment does not. |
| **HA/multi-node** | **NO** — local simulation only. Single-node remains the only supported runtime. |
| **Sustained SLO window** | **NO** — bounded runs only; no 7–30 day observation window. |
| **Real domain** | **NO** — Tier 1 is explicitly domainless. |

---

## Tier progression rules

1. **Tier 0 → Tier 1**: Engineering completes B+C+HA-B evidence (domainless semantics, PG local hardening/sustained workload, HA local failover simulation). Operator acknowledges Tier 1 scope and non-claims. No real domain required.
2. **Tier 1 → Tier 1.5** (optional): Engineering completes PostgreSQL production deployment, HA multi-node topology, and automated failover evidence. Operator acknowledges Tier 1.5 scope and non-claims. No real domain required. Tier 1.5 is the final intermediate tier; no further subtier may be introduced without a written ADR and explicit operator acknowledgment.
3. **Tier 1.5 → Tier 2** (or Tier 1 → Tier 2 directly): Operator procures real domain; engineering re-runs L1–L5 with real domain; sustained SLO window observed; G2 re-signed; operator final signoff obtained.
4. **No skip**: Tier 2 cannot be claimed without passing through Tier 1 evidence completeness. If Tier 1.5 is pursued, its evidence must also be complete before Tier 2 is claimed (or an explicit waiver documented in an ADR).
5. **No retroactive upgrade**: Tier 1 or Tier 1.5 attainment does not alter the status of any prior conditional signoff.

---

## Relationship to legacy terminology

- **Legacy `production-ready`**: Maps exclusively to Tier 2. It must never be used for Tier 0, Tier 1, or Tier 1.5.
- **Legacy `conditional pilot`**: Maps to Tier 0. It remains valid and unchanged.
- **New term `domainless production-candidate`**: Maps to Tier 1. Use this exact phrase when describing Tier 1.

---

## Non-claims

- **Tier 1 is not production-ready**.
- **Tier 1 does not close Block A**.
- **Tier 1 does not complete full G2**.
- **Tier 1 does not implement production HA/multi-node**; HA-B is local Docker simulation only.
- **Tier 1 does not deploy PostgreSQL to production**.
- **Tier 1 does not validate a sustained SLO observation window**.
- **Tier 1 does not require or claim a real owned domain**.
- **Tier 1.5 is optional and not yet started**; when pursued, it remains not production-ready and does not close Block A or complete full G2.

---

## Related docs

- [`00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md) — Scope boundaries and master non-claims table.
- [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md) — Blocker status; BLK-A-DOM gates Tier 1→Tier 2.
- [`docs/implementation-path/01-current-state.md`](../implementation-path/01-current-state.md) — Current state with Tier 1 completion status.
- [`docs/ROADMAP.md`](../ROADMAP.md) — Milestone 0.5 (Domainless Production-Candidate) and phased completion roadmap.
- [`docs/production-readiness-v2/12-domainless-completion-status.md`](./12-domainless-completion-status.md) — Tier 1 completion tracker.
- [`docs/production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md`](./00b-tier-1.5-domainless-infrastructure.md) — Tier 1.5 canonical definition (PLANNED / NOT COMPLETE).
- [`docs/production-readiness-v2/13-tier-1.5-completion-status.md`](./13-tier-1.5-completion-status.md) — Tier 1.5 completion tracker (all items PENDING).
- [`docs/implementation-path/artifacts/2026-05-26-domainless-candidate-plan.md`](../implementation-path/artifacts/2026-05-26-domainless-candidate-plan.md) — Tier 1 scope, B+C+HA-B work, expected evidence, and non-claims.
- [`docs/implementation-path/artifacts/2026-05-26-domainless-tier1-complete-end-state.md`](../implementation-path/artifacts/2026-05-26-domainless-tier1-complete-end-state.md) — Final Tier 1 end-state declaration.
- [`docs/implementation-path/artifacts/2026-05-26-domainless-tier1-operator-acknowledgment.md`](../implementation-path/artifacts/2026-05-26-domainless-tier1-operator-acknowledgment.md) — Operator acknowledgment record.

---

*End of file — Domainless Readiness Tier Model. Tier 1 is complete/acknowledged; Tier 1.5 is planned/not complete; Tier 2 remains gated.*
