# 00 — Scope and Non-Claims

> **Status**: Planning artifact. Not a claim of readiness.
> **Owner**: Engineering
> **Last updated**: 2026-05-19
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Lock the scope of the post-conditional-pilot production path and make all non-claims explicit so no reader can misinterpret planning docs as production-ready certification.

## Current state

- FerrumGate v1 is **RC-ready/conditional** for single-node SQLite pilot.
- G2.1–G2.8 are **signed for conditional single-node SQLite pilot only** (BrianNguyen, 09/05/2026).
- Block A is **WAIVED/CONDITIONAL** — DuckDNS accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure.
- Block B is **CLOSED**.
- Block C is **CLOSED**.
- PostgreSQL local runtime/Docker is implemented; production PG deployment is **NOT** done.
- HA/multi-node is **NOT** implemented.
- MCP local smoke passes; target-host MCP live workload is **NOT** evidence-backed.

## Gaps

- No unified scope boundary doc exists for the post-pilot production path.
- No explicit non-claims checklist exists across all planning docs.
- Risk of reader misinterpreting planning docs as production-ready certification.

## Implementation tasks

- [x] Review every doc in `docs/production-readiness-v2/` for overclaim.
- [x] Ensure every doc links back to this scope doc.
- [x] Ensure every doc repeats the non-claims table.
- [x] Engineering signoff that no doc uses "production-ready" without `= NO` qualifier.

## Non-claims

| Non-claim | Meaning |
|-----------|---------|
| **production-ready = NO** | FerrumGate is not production-ready. Do not deploy to unbounded production workloads. |
| **full G2 = NOT COMPLETE** | G2.1–G2.8 are signed for conditional pilot only, not full production signoff. |
| **Block A = WAIVED/CONDITIONAL** | Real domain is deferred. Block A is not closed. |
| **PostgreSQL production = NO** | Local PG runtime exists; production PG target deployment + evidence does not. |
| **HA/multi-node = NO** | Not implemented. Single-node SQLite is the only supported runtime. |
| **Target-host MCP live workload = NOT EVIDENCE-BACKED** | Local smoke passes; target-host sustained workload evidence is pending. |
| **Scoped auth/RBAC = NO** | Single global bearer token is the only auth mode in production pilot config. |
| **Multi-tenant = NO** | No tenant isolation exists. |

## Scope boundaries

### In scope for this doc pack

- PostgreSQL production hardening plan and acceptance gates.
- SLO/SLA definitions and validation runbooks.
- Target-host MCP/live workload validation plan.
- Security/tenant model ADR (design only; implementation is later).
- Evidence checklist per phase.

### Out of scope

- Real domain acquisition and DNS configuration (operator-owned; prerequisite for final claim).
- Code implementation (these are planning docs; code changes happen per phase).
- SOC2/enterprise compliance evidence pack.
- Visual policy builder or web admin dashboard.

## Naming crosswalk

See [`docs/ROADMAP.md` §"Naming crosswalk"](../../ROADMAP.md#naming-crosswalk) for the canonical distinction between:
- **ROADMAP Phase 0–9** (current post-pilot execution sequence)
- **Priority labels P0–P3** (task urgency within a phase)
- **Legacy quarters Q1–Q4** (historical baseline roadmap-v2 work packages)

## Glossary

| Term | Definition |
|------|------------|
| conditional pilot | Single-node SQLite deployment with operator signoff for bounded evaluation; not production-ready. |
| production-ready | Requires: real domain + revalidation + G2 re-signoff + SLO evidence window + operator final signoff. |
| WAIVED/CONDITIONAL | Blocker is acknowledged but not resolved; accepted for current pilot scope only. |

## Acceptance criteria

- [x] Every doc in `docs/production-readiness-v2/` links back to this scope doc.
- [x] Every doc in `docs/production-readiness-v2/` repeats the non-claims table.
- [x] No doc uses the phrase "production-ready" without the `= NO` qualifier.

## Evidence required

- Review signoff: engineering confirms no doc overclaims readiness.

## Relationship to legacy roadmap docs

This doc pack **supplements**—it does not supersede—the
[`docs/ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/`](../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/README.md).
The legacy roadmap-v2 pack remains the historical/baseline planning reference.
`docs/production-readiness-v2/` is the current active post-pilot execution and
evidence planning layer.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) — Parent roadmap with full gap analysis and phase plan.
- [`docs/implementation-path/67-production-readiness-roadmap.md`](../../implementation-path/67-production-readiness-roadmap.md) — Prior v1 production-readiness tracker.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime configuration and stress baselines.
