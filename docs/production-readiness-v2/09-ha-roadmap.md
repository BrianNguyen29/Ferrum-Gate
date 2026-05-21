# 09 — HA/Multi-Node Roadmap

> **Status**: Planning artifact. ADR approved as planning decision; no implementation.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **HA ADR**: [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md) — approved as planning decision 2026-05-21; no implementation claim

---

## Goal

Design the path from single-node production to multi-node/HA, starting with an ADR and manual failover, then read replicas, and only then automated failover.

## Current state

- Single-node SQLite is the only supported runtime.
- PostgreSQL local runtime exists but is not production-deployed.
- HA ADR approved as planning decision 2026-05-21; implementation remains NOT STARTED.
- No replication configs.
- No failover runbook.

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| HA ADR approved as planning decision; implementation NOT STARTED | Critical | Cannot implement HA without an approved design |
| No manual failover runbook | High | Operator cannot recover from primary failure |
| No read replica plan | High | Read scaling is not possible |
| No automated failover | Critical | Not true HA without automation |
| No split-brain prevention | Critical | HA claim is impossible without this |

## Implementation tasks

### HA-1 — HA ADR

- [x] Compare options: managed PostgreSQL HA, Patroni, repmgr, manual failover, read replicas only. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §2.
- [x] Define: failover strategy, replica strategy, split-brain prevention, leader/writer model, read routing, migration handling, RPO/RTO target. — **DRAFTED** in [`ha-adr.md`](./ha-adr.md) §3–§6.
- [x] Operator review and signoff of [`ha-adr.md`](./ha-adr.md). — **APPROVED AS PLANNING DECISION** 2026-05-21 (no implementation claim; no HA claim).

### HA-2 — Manual failover

- [ ] Primary down detection procedure.
- [ ] Standby promotion procedure (manual).
- [ ] ferrumd reconnect/reroute procedure.
- [ ] RPO/RTO measurement.

### HA-3 — Read replicas

- [ ] Read-only endpoints can use replica.
- [ ] Writes go to primary.
- [ ] Readiness shows replica lag.
- [ ] Stale reads documented.

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
- `manual-failover-drill-evidence.md`
- `read-replica-test-evidence.md`

## Non-claims

- **NOT HA yet**: This is a roadmap and ADR; no HA code exists.
- **NOT production-ready**: HA is explicitly out of scope for production-ready claim.
- **NOT automated failover soon**: Manual failover and read replicas come first.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3.3 Phase PG-5, §4 Phase 9
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites.
