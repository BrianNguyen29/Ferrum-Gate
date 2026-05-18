# 09 — HA/Multi-Node Roadmap

> **Status**: Planning artifact. ADR-only; no implementation.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Design the path from single-node production to multi-node/HA, starting with an ADR and manual failover, then read replicas, and only then automated failover.

## Current state

- Single-node SQLite is the only supported runtime.
- PostgreSQL local runtime exists but is not production-deployed.
- No HA design exists.
- No replication configs.
- No failover runbook.

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| No HA ADR | Critical | Cannot implement HA without a design |
| No manual failover runbook | High | Operator cannot recover from primary failure |
| No read replica plan | High | Read scaling is not possible |
| No automated failover | Critical | Not true HA without automation |
| No split-brain prevention | Critical | HA claim is impossible without this |

## Implementation tasks

### HA-1 — HA ADR

- [ ] Compare options: managed PostgreSQL HA, Patroni, repmgr, manual failover, read replicas only.
- [ ] Define: failover strategy, replica strategy, split-brain prevention, leader/writer model, read routing, migration handling, RPO/RTO target.

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
