# 09 — HA/Multi-Node Roadmap

> **Status**: Planning artifact. ADR approved as planning decision; no implementation.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **HA ADR**: [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md) — approved as planning decision 2026-05-21; no implementation claim

> **Delegated signoff (planning-only)**
> - **Signed by**: BrianNguyen (session authorization)
> - **Date**: 2026-05-21
> - **Scope**: HA roadmap and ADR approved as planning decision only.
> - **Nature**: Planning/decision document signoff only. This does not constitute evidence of HA implementation, multi-node deployment, automated failover, or production readiness. Does not substitute for missing evidence.
> - **Authority**: User explicitly authorized delegated signoff for planning and decision documents.

---

## Goal

Design the path from single-node production to multi-node/HA, starting with an ADR and manual failover, then read replicas, and only then automated failover.

## Current state

- Single-node SQLite is the only supported runtime.
- PostgreSQL local runtime exists but is not production-deployed.
- HA ADR approved as planning decision 2026-05-21; implementation remains NOT STARTED.
- Manual failover runbook drafted as planning artifact 2026-05-21; no live drill performed.
- No replication configs.

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| HA ADR approved as planning decision; implementation NOT STARTED | Critical | Cannot implement HA without an approved design |
| No manual failover runbook | High | Operator cannot recover from primary failure |
| Read replica design drafted; implementation NOT STARTED | High | Read scaling design exists; no code or deployment |
| No automated failover | Critical | Not true HA without automation |
| No split-brain prevention | Critical | HA claim is impossible without this |

## Implementation tasks

### HA-1 — HA ADR

- [x] Compare options: managed PostgreSQL HA, Patroni, repmgr, manual failover, read replicas only. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §2.
- [x] Define: failover strategy, replica strategy, split-brain prevention, leader/writer model, read routing, migration handling, RPO/RTO target. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §3–§6.
- [x] Operator review and signoff of [`ha-adr.md`](./ha-adr.md). — **APPROVED AS PLANNING DECISION** 2026-05-21 (no implementation claim; no HA claim).

### HA-2 — Manual failover

- [x] Primary down detection procedure. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §3.
- [x] Standby promotion procedure (manual). — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §4.
- [x] ferrumd reconnect/reroute procedure. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §5.
- [x] RPO/RTO expectations documented. — **DOCUMENTED** in [`manual-failover-runbook.md`](./manual-failover-runbook.md) §6 and [`ha-adr.md`](./ha-adr.md) §3.
- [ ] RPO/RTO measured during live drill. — **DEFERRED** until operator environment with replication exists.

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

- PostgreSQL production foundation is stable.
- Security/tenant model is decided.
- SLO metrics are available.
- Backup/restore evidence exists.

## Acceptance criteria

- [ ] HA ADR approved.
- [ ] Manual failover drill passes with measured RPO/RTO.
- [ ] Read replica behavior documented and tested.
- [ ] Automated failover deferred until tenant/security model is stable.

## Evidence required

- `ha-adr.md`
- `manual-failover-runbook.md` (planning artifact; no live drill)
- `manual-failover-drill-evidence.md` (deferred until operator environment ready)
- `read-replica-design.md` (planning artifact; no implementation)
- `read-replica-test-evidence.md` (deferred until operator environment ready)

## Non-claims

- **NOT HA yet**: This is a roadmap and ADR; no HA code exists.
- **NOT production-ready**: HA is explicitly out of scope for production-ready claim.
- **NOT automated failover soon**: Manual failover and read replicas come first.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3.3 Phase PG-5, §4 Phase 9
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites.
