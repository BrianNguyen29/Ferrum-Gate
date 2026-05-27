# 09 — HA/Multi-Node Roadmap

> **Status**: Phase 9 topology ADR selected. ADR approved as planning decision; local simulation added 2026-05-26; Tier 1.5 same-VM HA evidence complete; Phase 9 selected topology is two independent PostgreSQL hosts with streaming replication + PgBouncer/manual failover; multi-host production HA implementation remains NOT COMPLETE.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-26
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **HA ADR**: [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md) — approved as planning decision 2026-05-21; no implementation claim

> **Delegated signoff (planning-only)**
> - **Signed by**: BrianNguyen (session authorization)
> - **Date**: 2026-05-21
> - **Scope**: HA roadmap and ADR approved as planning decision only; Phase 9 prerequisites recorded as unblocked on 2026-05-27.
> - **Nature**: Planning/decision/prerequisite signoff only. This does not constitute evidence of multi-host production HA, Tier 2 production readiness, or full G2 completion. Does not substitute for missing multi-host evidence.
> - **Authority**: User explicitly authorized delegated signoff for planning and decision documents.

---

## Goal

Design the path from single-node production to multi-node/HA, starting with an ADR and manual failover, then read replicas, and only then automated failover.

## Current state

- Tier 1 and Tier 1.5 are complete/acknowledged; PostgreSQL target deployment and same-VM HA evidence exist for Tier 1.5.
- HA ADR approved as planning decision 2026-05-21.
- Manual failover runbook drafted as planning artifact 2026-05-21.
- **Local HA simulation added 2026-05-26**: Docker Compose primary/standby with streaming replication, `pg_basebackup`, and manual `pg_promote()` failover drill. Latest measured RTO 3 s, RPO 0 rows lost locally. See [`docs/implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md`](../../implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md).
- **Tier 1.5 same-VM HA evidence added 2026-05-27**: nonprod target PostgreSQL primary/standby streaming replication, same-VM automated failover drills, and operator acknowledgment are complete. See [`docs/production-readiness-v2/13-tier-1.5-completion-status.md`](./13-tier-1.5-completion-status.md).
- **Phase 9 prerequisites unblocked 2026-05-27**: PostgreSQL foundation, security/tenant decisions, SLO metrics, and backup/restore evidence are now available for beginning the next HA workstream. See [`docs/implementation-path/artifacts/2026-05-27-ha-phase9-prerequisites-unblocked.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-prerequisites-unblocked.md).
- **Phase 9 topology ADR selected 2026-05-27**: two independent PostgreSQL hosts/VMs with streaming replication, PgBouncer routing, and manual/operator-controlled failover drills before any automated multi-host claim. See [`2026-05-27-ha-phase9-multihost-topology-adr.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-multihost-topology-adr.md).
- Multi-host production HA remains NOT COMPLETE.

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| Multi-host production HA NOT COMPLETE | Critical | Tier 1.5 evidence is same-VM only; independent-host failover evidence is still required before any production HA claim |
| Operator-environment Phase 9 HA-4/HA-5 evidence missing | Critical | Multi-host automated failover and RPO/RTO measurement have not been executed |
| Read replica implementation deferred | High | Read scaling design exists; code/deployment require follow-up ADR/implementation |
| Sustained SLO window missing | High | Available SLO evidence is bounded/canonical, not a 7–30 day observation window |

## Implementation tasks

### HA-1 — HA ADR

- [x] Compare options: managed PostgreSQL HA, Patroni, repmgr, manual failover, read replicas only. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §2.
- [x] Define: failover strategy, replica strategy, split-brain prevention, leader/writer model, read routing, migration handling, RPO/RTO target. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §3–§6.
- [x] Operator review and signoff of [`ha-adr.md`](./ha-adr.md). — **APPROVED AS PLANNING DECISION** 2026-05-21 (no implementation claim; no HA claim).
- [x] Phase 9 multi-host topology ADR selected. — **PLANNING ADR COMPLETE** 2026-05-27: two independent PostgreSQL hosts + streaming replication + PgBouncer/manual failover. No multi-host HA claim.

### HA-2 — Manual failover

- [x] Primary down detection procedure. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §3.
- [x] Standby promotion procedure (manual). — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §4.
- [x] ferrumd reconnect/reroute procedure. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §5.
- [x] RPO/RTO expectations documented. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §6 and [`ha-adr.md`](./ha-adr.md) §3.
- [x] RPO/RTO measured during local simulation drill. — **LOCAL EVIDENCE** 2026-05-26; latest RTO 3 s, RPO 0 rows lost. See [`docs/implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md`](../../implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md).
- [ ] RPO/RTO measured during multi-host/operator-environment drill. — **OPEN** for Phase 9 follow-up; prerequisites are unblocked, but evidence does not exist yet.

### HA-3 — Read replicas

- [x] Read-only endpoints can use replica. — **DESIGNED** in [`read-replica-design.md`](./read-replica-design.md) §5.2.
- [x] Writes go to primary. — **DESIGNED** in [`read-replica-design.md`](./read-replica-design.md) §5.1.
- [x] Readiness shows replica lag. — **DESIGNED** in [`read-replica-design.md`](./read-replica-design.md) §7.2.
- [x] Stale reads documented. — **DESIGNED** in [`read-replica-design.md`](./read-replica-design.md) §6.
- [ ] Read replica code implemented and tested. — **DEFERRED** until follow-up ADR selects Strategy A or B.

### HA-4 — Automated failover (deferred)

- [ ] Automated failover drill.
- [ ] No split-brain.
- [ ] Writes resume.
- [ ] Data consistency verified.
- [ ] Incident log generated.

## Do not start before

- [x] PostgreSQL production foundation is stable for Tier 1.5 nonprod target.
- [x] Security/tenant model is decided for current T1 scope.
- [x] SLO metrics are available as bounded/canonical evidence.
- [x] Backup/restore evidence exists for Tier 1.5 nonprod target.

See [`2026-05-27-ha-phase9-prerequisites-unblocked.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-prerequisites-unblocked.md) for the consolidated prerequisite notice.

## Acceptance criteria

- [x] HA ADR approved as planning decision.
- [x] Manual failover drill passes with measured RPO/RTO locally. — **LOCAL ONLY** 2026-05-26.
- [ ] Multi-host/operator-environment failover drill passes with measured RPO/RTO.
- [ ] Read replica behavior implemented and tested.
- [x] Tenant/security model stable enough to begin Phase 9 planning; automated failover evidence remains open for multi-host/operator-environment scope.

## Evidence required

- `ha-adr.md`
- [`2026-05-27-ha-phase9-multihost-topology-adr.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-multihost-topology-adr.md) — selected Phase 9 multi-host topology ADR; no implementation/evidence claim
- `manual-failover-runbook.md` (planning artifact; no live drill)
- `manual-failover-drill-evidence.md` — local simulation [`2026-05-26-ha-local-failover-simulation-evidence.md`](../../implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md) exists; operator-environment drill deferred
- `read-replica-design.md` (planning artifact; no implementation)
- `read-replica-test-evidence.md` (deferred until operator environment ready)
- [`2026-05-27-ha-phase9-prerequisites-unblocked.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-prerequisites-unblocked.md) — prerequisite unblock notice; no multi-host HA claim

## Non-claims

- **NOT multi-host production HA yet**: This is a roadmap/prerequisite notice; local simulation and Tier 1.5 same-VM HA exist but do not prove independent-host HA.
- **NOT production-ready**: HA is explicitly out of scope for production-ready claim.
- **NOT Phase 9 automated failover complete**: Same-VM Tier 1.5 automated failover exists, but Phase 9 multi-host/operator-environment automated failover evidence remains open.
- **NOT true multi-node production HA**: Local simulation and Tier 1.5 topology share host fate.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3.3 Phase PG-5, §4 Phase 9
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites.
- [`docs/implementation-path/artifacts/HA-multi-node-evidence-runbook.md`](../../implementation-path/artifacts/HA-multi-node-evidence-runbook.md) — Detailed operator execution guide for capturing HA/multi-node evidence (failover drill, RPO/RTO measurement, read replica validation, rollback criteria)
