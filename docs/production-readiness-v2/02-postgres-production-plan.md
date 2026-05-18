# 02 — PostgreSQL Production Plan

> **Status**: Planning artifact. Production PG not deployed.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Transform PostgreSQL from "local feature-gated implementation" into a production-candidate backend with target-host deployment evidence, hardened connections, metrics, and backup/restore discipline.

## Current state

PostgreSQL local/runtime foundation is strong:

| Item | Status |
|------|--------|
| 9 repo implementations | ✅ Done |
| Embedded schema migration | ✅ Done |
| Feature-gated postgres build | ✅ Done |
| Runtime DSN switching | ✅ Done |
| Pool config cơ bản | ✅ Done |
| ferrum-migrate SQLite → PG | ✅ Done |
| Docker Compose PG local | ✅ Done |
| Config templates | ✅ Done |
| Integration tests local | ✅ Done |
| Backup/restore runbook | ✅ Done (docs) |
| Ops cadence docs | ✅ Done (docs) |

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| No connection pool health/reconnect | High | DB restart may require ferrumd restart |
| No circuit breaker | High | DB failure can cascade to gateway |
| No statement timeout | Medium/High | Slow queries can hold connections indefinitely |
| No PG pool metrics | Medium | Cannot observe pool saturation |
| No PG-specific alert rules | Medium | Hard to operate without alerts |
| No TLS/SSL DSN guidance | Medium | Production PG connection not hardened |
| No schema versioning standard | Medium/High | Migrations are currently one-shot |
| No target-host PG drills | High | No evidence of production PG behavior |
| No PG restore drill evidence | High | Backup docs exist; evidence does not |
| No CI for postgres feature | Medium | Drift risk |
| No PgBouncer/connection pooling story | Medium | Scaling is difficult |
| No HA/failover | Critical | No production HA |
| No replication configs | High | No standby/read replica |
| No failover runbook | High | No promote/reroute procedure |
| No split-brain prevention | High | HA claim impossible without this |

## Implementation tasks

### Phase PG-1 — Production PG baseline

- [ ] Deploy ferrumd with `postgres://...` on target/staging environment.
- [ ] Verify `/v1/readyz/deep` returns 200.
- [ ] Migrate SQLite snapshot to PG staging.
- [ ] Run full tests or targeted integration on PG.
- [ ] Create PG target env doc.
- [ ] Create PG deployment runbook.
- [ ] Create PG migration evidence artifact.
- [ ] Create PG readyz evidence artifact.

### Phase PG-2 — Connection hardening

- [ ] Add `statement_timeout` to PG connection config.
- [ ] Add `idle_in_transaction_session_timeout`.
- [ ] Add pool acquire timeout metrics.
- [ ] Implement reconnect/retry policy.
- [ ] Add DB health circuit breaker.
- [ ] Implement graceful degraded readiness.

### Phase PG-3 — Backup/restore evidence

- [ ] Implement scheduled `pg_dump` or WAL backup.
- [ ] Implement retention pruning.
- [ ] Execute restore drill to clean DB.
- [ ] Verify row counts and content hashes.
- [ ] Create evidence artifact.

### Phase PG-4 — Schema migration discipline

- [ ] Add migration version table.
- [ ] Convert to incremental migration files.
- [ ] Make migration runner idempotent.
- [ ] Document rollback/forward strategy.
- [ ] Add schema drift check to CI.

### Phase PG-5 — HA design (ADR first)

- [ ] Write HA ADR comparing managed PG, Patroni, repmgr, manual failover, read replicas.
- [ ] Select strategy: managed PG or manual failover runbook first.
- [ ] Document read replica plan for later.
- [ ] Defer automated failover until tenant/security model is stable.

## Acceptance criteria

| Gate | Criteria |
|------|----------|
| PG-1 | `FERRUMD_STORE_DSN=postgres://...` and `/v1/readyz/deep = 200` |
| PG-2 | Restart PG during test → ferrumd recovers or fails closed cleanly |
| PG-3 | `pg_dump exit 0`, `pg_restore exit 0`, restored row count matches, readiness after restore pass |
| PG-4 | Migration can run twice safely; version recorded; CI checks schema drift |
| PG-5 | HA ADR approved; primary failure drill documented; RPO/RTO measured for manual failover |

## Evidence required

- `pg-target-deployment-evidence.md`
- `pg-restore-drill-evidence.md`
- `pg-migration-evidence.md`
- `pg-ha-adr.md`

## Non-claims

- **NOT production-ready**: PG hardening is a prerequisite, not a completion.
- **NOT HA**: HA design is ADR-only in this plan; implementation is later.
- **NOT validated on target**: Until PG-1 target drill is executed, claims are local-only.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3, §4 Phase 1
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md)
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — SQLite vs PG scaling guidance
