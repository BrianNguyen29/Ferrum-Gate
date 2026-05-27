# 09 — HA/Multi-Node Roadmap

> **Status**: Phase 9 multi-host manual evidence captured. ADR approved as planning decision; local simulation added 2026-05-26; Tier 1.5 same-VM HA evidence complete; Phase 9 deployed two independent PostgreSQL hosts with streaming replication + PgBouncer/manual failover evidence on 2026-05-27, including 4 manual drills and failback; multi-host production HA and automated failover remain NOT COMPLETE.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-27
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
- **Phase 9 multi-host manual drill evidence captured 2026-05-27**: host A `ferrumgate-nonprod` (`10.0.0.2`) and host B `ferrumgate-pg-ha-b` (`10.0.0.3`) were configured with PostgreSQL streaming replication; four manual drills passed, including A→B failover and B→A failback, with observed RPO 0 marker loss and RTO improving from 246s to 22s after TLS/config parity fixes; bounded partition check confirmed standby stayed read-only. See [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-multihost-drill-evidence.md).
- **Phase 9 automated failover/fencing ADR drafted 2026-05-27**: next safe step selected as automated detection + operator-confirmed manual promotion; automatic promotion without fencing is rejected. See [`2026-05-27-ha-phase9-automated-failover-fencing-adr.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-automated-failover-fencing-adr.md).
- **Phase 9 detection-only watchdog evidence captured 2026-05-27**: detection-only watchdog installed/enabled on both hosts; healthy and alert paths verified without auto-promotion; PostgreSQL TLS/WAL parity normalized; Alertmanager service/API mismatch resolved as unit-name mismatch (`prometheus-alertmanager.service`). See [`2026-05-27-ha-phase9-watchdog-config-parity-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-watchdog-config-parity-evidence.md).
- **Phase 9 GCP fencing mechanism evidence captured 2026-05-27**: `scripts/gcp/phase9_fencing.sh` validated (`bash -n` pass); dry-run confirmed no action; app-host guard blocked fencing of `ferrumgate-nonprod` without `--force-app-host`; real safe fencing test on standby host B succeeded (instance `TERMINATED`, host A remained primary and app healthy); recovery succeeded (VM restart, PostgreSQL manual start, B returned to standby, A replication verified). This is fencing-mechanism evidence only, not HA-4 automated failover. See [`2026-05-27-ha-phase9-gcp-fencing-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-gcp-fencing-evidence.md).
- Multi-host production HA and multi-host automated failover remain NOT COMPLETE.

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| Multi-host production HA NOT COMPLETE | Critical | Manual independent-host drills exist, but production HA still requires automated/fenced failover evidence, full incident-response signoff, and operator production posture signoff |
| Operator-environment Phase 9 HA-4 evidence missing | Critical | Multi-host automated failover has not been executed; ADR selects detection-only/manual promotion until fencing gates pass. GCP fencing script exists and was tested on standby host B only; app-host SPOF still blocks safe host-A fencing and automated HA-4 |
| Read replica implementation deferred | High | Read scaling design exists; code/deployment require follow-up ADR/implementation |
| Sustained SLO window missing | High | Available SLO evidence is bounded/canonical, not a 7–30 day observation window |

## Implementation tasks

### HA-1 — HA ADR

- [x] Compare options: managed PostgreSQL HA, Patroni, repmgr, manual failover, read replicas only. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §2.
- [x] Define: failover strategy, replica strategy, split-brain prevention, leader/writer model, read routing, migration handling, RPO/RTO target. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §3–§6.
- [x] Operator review and signoff of [`ha-adr.md`](./ha-adr.md). — **APPROVED AS PLANNING DECISION** 2026-05-21 (no implementation claim; no HA claim).
- [x] Phase 9 multi-host topology ADR selected. — **PLANNING ADR COMPLETE** 2026-05-27: two independent PostgreSQL hosts + streaming replication + PgBouncer/manual failover. No multi-host HA claim.
- [x] Phase 9 automated failover/fencing ADR drafted. — **ADR COMPLETE** 2026-05-27: selected automated detection + operator-confirmed manual promotion; rejected auto-promotion without fencing. No HA-4 completion claim.
- [x] Detection-only/manual-promotion watchdog installed and verified. — **DETECTION ONLY** 2026-05-27: no auto-promotion, no fencing, no HA-4 completion claim.

### HA-2 — Manual failover

- [x] Primary down detection procedure. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §3.
- [x] Standby promotion procedure (manual). — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §4.
- [x] ferrumd reconnect/reroute procedure. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §5.
- [x] RPO/RTO expectations documented. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §6 and [`ha-adr.md`](./ha-adr.md) §3.
- [x] RPO/RTO measured during local simulation drill. — **LOCAL EVIDENCE** 2026-05-26; latest RTO 3 s, RPO 0 rows lost. See [`docs/implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md`](../../implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md).
- [x] RPO/RTO measured during multi-host/operator-environment drills. — **MANUAL EVIDENCE CAPTURED** 2026-05-27; four manual drills measured RTO/RPO, including B→A failback; observed RPO 0 marker loss. This does not complete automated failover or production HA.

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

> **Fencing progress 2026-05-27**: GCP instance-stop fencing script created and tested on standby host B; app-host guard blocks host A by default. HA-4 remains deferred because host A app/PgBouncer SPOF prevents safe automated fencing of the primary. Host B PgBouncer/app endpoint or managed routing is required before HA-4 can proceed safely.

## Do not start before

- [x] PostgreSQL production foundation is stable for Tier 1.5 nonprod target.
- [x] Security/tenant model is decided for current T1 scope.
- [x] SLO metrics are available as bounded/canonical evidence.
- [x] Backup/restore evidence exists for Tier 1.5 nonprod target.

See [`2026-05-27-ha-phase9-prerequisites-unblocked.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-prerequisites-unblocked.md) for the consolidated prerequisite notice.

## Acceptance criteria

- [x] HA ADR approved as planning decision.
- [x] Manual failover drill passes with measured RPO/RTO locally. — **LOCAL ONLY** 2026-05-26.
- [x] Multi-host/operator-environment failover drill passes with measured RPO/RTO. — **MANUAL DRILLS ONLY** 2026-05-27; 4 manual multi-host drills passed, including failback. Automated/fenced failover remains open.
- [ ] Read replica behavior implemented and tested.
- [x] Tenant/security model stable enough to begin Phase 9 planning; automated failover evidence remains open for multi-host/operator-environment scope.

## Evidence required

- `ha-adr.md`
- [`2026-05-27-ha-phase9-multihost-topology-adr.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-multihost-topology-adr.md) — selected Phase 9 multi-host topology ADR; no implementation/evidence claim
- [`2026-05-27-ha-phase9-automated-failover-fencing-adr.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-automated-failover-fencing-adr.md) — selected detection-only/manual-promotion path; rejects auto-promotion without fencing
- [`2026-05-27-ha-phase9-watchdog-config-parity-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-watchdog-config-parity-evidence.md) — detection-only watchdog, config parity, and Alertmanager unit evidence; no HA-4 completion claim
- `manual-failover-runbook.md` (planning artifact; no live drill)
- `manual-failover-drill-evidence.md` — local simulation [`2026-05-26-ha-local-failover-simulation-evidence.md`](../../implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md) exists; operator-environment manual drill captured in [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-multihost-drill-evidence.md)
- `read-replica-design.md` (planning artifact; no implementation)
- `read-replica-test-evidence.md` (deferred until operator environment ready)
- [`2026-05-27-ha-phase9-prerequisites-unblocked.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-prerequisites-unblocked.md) — prerequisite unblock notice; no multi-host HA claim

## Non-claims

- **NOT multi-host production HA yet**: This is a roadmap/prerequisite notice; local simulation and Tier 1.5 same-VM HA exist but do not prove independent-host HA.
- **NOT production-ready**: HA is explicitly out of scope for production-ready claim.
- **NOT Phase 9 automated failover complete**: Same-VM Tier 1.5 automated failover exists, but Phase 9 multi-host/operator-environment automated failover evidence remains open.
- **NOT true multi-node production HA**: Manual multi-host drills exist, but automated/fenced failover, full incident-response evidence, and production signoff remain incomplete.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3.3 Phase PG-5, §4 Phase 9
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites.
- [`docs/implementation-path/artifacts/HA-multi-node-evidence-runbook.md`](../../implementation-path/artifacts/HA-multi-node-evidence-runbook.md) — Detailed operator execution guide for capturing HA/multi-node evidence (failover drill, RPO/RTO measurement, read replica validation, rollback criteria)
