# 54 — Operator Signoff Packet

> **Status**: Documentation-only. No production deployment performed.
> **Purpose**: Standalone fillable operator signoff form for FerrumGate v1 single-node SQLite production pilot.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. No production-ready claim.

---

## Purpose

This packet is the formal operator acceptance checklist before any production pilot deployment. It is **documentation-only** — completing these items does not claim production readiness; it confirms the operator has evaluated and accepted the known constraints.

**Do not mark these items complete on behalf of the operator.** Each item requires explicit operator acknowledgment and, where indicated, documented accepted-risk signoff.

---

## 1. SQLite Single-Node Limits — Operator Must Acknowledge

| Item | Required Action | Reference |
|---|---|---|
| Write throughput ceiling | Operator confirms expected sustained writes ≤300 writes/s; above this requires PostgreSQL (Phase 3) | `27-production-evaluation-plan.md` §1.2 |
| Single-node only | Operator acknowledges no multi-node/HA/replica support in v1 | Support contract §3 |
| Bounded execution history | Operator acknowledges SQLite file size and lineage traversal limits at scale | Support contract §3 |

**Signoff phrase required**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

Operator signature: _________________ Date: _________

---

## 2. Authentication and Transport Security

| Item | Required Action | Reference |
|---|---|---|
| Bearer token mode | Operator confirms `auth_mode = "Bearer"` with operator-managed token; `FERRUMD_BEARER_TOKEN` or config file | `27-production-evaluation-plan.md` §2.1 |
| TLS/reverse proxy | Operator confirms FerrumGate is deployed behind a TLS-terminating reverse proxy (not exposed bare on internet) | `27-production-evaluation-plan.md` §2.1 |
| Health endpoints unauthenticated | Operator acknowledges `/v1/healthz` and `/v1/readyz` are intentionally unauthenticated; governance routes require auth | `27-production-evaluation-plan.md` §2.1 |

**Signoff phrase required**: "Operator has configured bearer auth and confirmed TLS termination is handled by the reverse proxy."

Operator signature: _________________ Date: _________

---

## 3. Backup, Restore, and Recovery Objectives

| Item | Required Action | Reference |
|---|---|---|
| Backup schedule outside FerrumGate | Operator implements backup scheduling external to FerrumGate (cron, CI job, etc.); `ferrumctl backup` does not support automated scheduling | `27-production-evaluation-plan.md` §3.5 |
| Backup retention | Operator defines and enforces backup retention policy outside FerrumGate | `27-production-evaluation-plan.md` §3.5 |
| Restore drill performed | Operator has run `ferrumctl backup restore` in a non-production environment and verified data integrity with `PRAGMA integrity_check` | Operations runbook §4 |
| RPO accepted | Operator understands RPO = time since last backup; any writes after last backup are lost on restore | `27-production-evaluation-plan.md` §3.5 |
| RTO accepted | Operator understands RTO includes backup restore time + re-start + verification; FerrumGate has no automated recovery | `27-production-evaluation-plan.md` §3.5 |

**Signoff phrase required**: "Operator has performed a restore drill, confirmed RPO/RTO fit for the target workload, and backup retention is managed externally."

Operator signature: _________________ Date: _________

---

## 4. PostgreSQL / Multi-Node Deferred Status

| Item | Required Action | Reference |
|---|---|---|
| PostgreSQL not implemented | Operator acknowledges PostgreSQL is not implemented; `postgres://` DSNs are rejected at startup | ADR-50 |
| Multi-node/HA not implemented | Operator acknowledges v1 is single-node only; scale-out requires Phase 3 | ADR-50, Production roadmap |
| Phase 3 not started | Operator confirms no expectation of PostgreSQL support in the current pilot | ADR-50 Phase P1 |

**Signoff phrase required**: "Operator acknowledges PostgreSQL/multi-node is deferred and not part of the current production pilot scope."

Operator signature: _________________ Date: _________

---

## 5. Production Pilot Prerequisites

Before the first production pilot deployment, the following must be all confirmed:

| # | Prerequisite | Verification |
|---|---|---|
| 1 | Write workload modeled against SQLite capacity (≤300 writes/s sustained) | §1 signoff above |
| 2 | Bearer auth + TLS/reverse proxy confirmed | §2 signoff above |
| 3 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` |
| 4 | Restore drill completed with `PRAGMA integrity_check` passing | Operator evidence of successful restore |
| 5 | RPO/RTO formally accepted for target workload | §3 signoff above |
| 6 | All production evaluation dimensions SATISFIED or CONDITIONAL (with controls) | `27-production-evaluation-plan.md` Evaluation Decision Framework completed |
| 7 | Accepted-risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed |
| 8 | Compensate noop risk formally accepted | Operator acknowledges compensate may be noop-backed for target adapters |

**This is not a production-ready claim.** FerrumGate v1 is RC-ready with known accepted risks. Production pilot deployment is conditional on all eight prerequisites above being satisfied.

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- No production-ready claim is made in this document
- PostgreSQL/multi-node/HA are not implemented and not in scope
- Phase 2 transaction batching was deferred/regressed
- This signoff packet confirms operator evaluation only — it does not authorize deployment or claim production readiness

---

## Cross-References

| Document | Purpose |
|----------|---------|
| `27-production-evaluation-plan.md` | Canonical production evaluation framework |
| `19-v1-single-node-support-contract.md` | Accepted risks §4, support constraints §3 |
| `26-v1-single-node-invariant-control-test-evidence-matrix.md` | Weak Spots 1–4 resolved |
| `23-production-readiness-assessment.md` | RC-ready declaration |
| `31-release-paths-todo.md` §Path 2 | Full production pilot path with G2 gates |

---

*Document generated: 2026-04-28. Documentation-only — no production deployment performed.*
