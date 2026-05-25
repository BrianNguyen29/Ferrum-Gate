# TEMPLATE — PostgreSQL Production Deployment Signoff

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output and operator signoff.
> See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) for the execution runbook.
> See [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) §Claim 4 for prerequisites.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `production / staging / target-host-name` |
| **Operator** | `name` |
| **ferrumd version / commit** | `git describe --always` |
| **PostgreSQL version** | `N.N` |
| **DSN (sanitized — no password)** | `postgres://user@host:port/dbname?sslmode=require` |

---

## Prerequisites Checklist

All items below must be checked before this signoff is valid.

| # | Prerequisite | Evidence Artifact | Status |
|---|-------------|-------------------|--------|
| P.1 | PG target/staging provisioned and reachable | `YYYY-MM-DD-pg-target-deployment-evidence.md` | ☐ |
| P.2 | ferrumd starts with PostgreSQL DSN and stays up | `YYYY-MM-DD-pg-target-deployment-evidence.md` §PG-1.2 | ☐ |
| P.3 | `/v1/readyz/deep` reports `store: healthy` | `YYYY-MM-DD-pg-target-deployment-evidence.md` §PG-1.3 | ☐ |
| P.4 | `ferrum-migrate` completes with row count + hash match | `YYYY-MM-DD-pg-target-deployment-evidence.md` §PG-1.4–1.6 | ☐ |
| P.5 | Connection hardening configured (timeout, metrics) | `02-postgres-production-plan.md` §PG-2 | ☐ |
| P.6 | TLS/SSL DSN validated (if required) | `TEMPLATE-pg-tls-dsn-evidence.md` filled | ☐ |
| P.7 | Scheduled backup executing and verified | `TEMPLATE-pg-scheduled-backup-evidence.md` filled | ☐ |
| P.8 | Retention pruning executing and verified | `TEMPLATE-pg-retention-pruning-evidence.md` filled | ☐ |
| P.9 | Offsite sync executing and verified | `TEMPLATE-pg-offsite-sync-evidence.md` filled | ☐ |
| P.10 | Alert rules deployed to live Prometheus | `TEMPLATE-pg-alert-deployment-evidence.md` filled | ☐ |
| P.11 | Restore drill passed on production-like data | `YYYY-MM-DD-pg-restore-drill-evidence.md` | ☐ |
| P.12 | PgBouncer validated (if multi-instance) | `TEMPLATE-pg-pgbouncer-evidence.md` filled (optional) | ☐ |

**Overall prerequisites**: `PASS / FAIL` *(requires P.1–P.11 checked; P.12 optional)*

---

## Deployment Configuration

| Parameter | Value |
|-----------|-------|
| Store backend | `postgres` |
| `pg_max_connections` | `N` |
| `pg_min_idle` | `N` |
| `pg_acquire_timeout_secs` | `N` |
| `pg_statement_timeout_ms` | `N` (0 = disabled) |
| `pg_idle_in_transaction_timeout_ms` | `N` (0 = disabled) |
| TLS mode | `disable / require / verify-ca / verify-full` |
| PgBouncer in use | `YES / NO` |
| HA/replication in use | `YES / NO` *(if YES, see `TEMPLATE-ha-multinode-evidence-pack.md`)* |

---

## Health and Metrics

| Check | Command / Method | Expected | Actual | Pass/Fail |
|-------|------------------|----------|--------|-----------|
| Deep readiness | `curl -sf http://<bind>/v1/readyz/deep` | HTTP 200, `store: healthy` | | ☐ |
| PG pool metrics | `curl -sf http://<bind>/v1/metrics` | `ferrumgate_store_pg_pool_*` present | | ☐ |
| Acquire timeout metric | `curl -sf http://<bind>/v1/metrics` | `ferrumgate_store_pg_acquire_timeouts_total` present | | ☐ |
| Prometheus scrape | Prometheus UI target status | `UP` | | ☐ |

**Overall health**: `PASS / FAIL`

---

## Backup Discipline

| Check | Expected | Actual | Pass/Fail |
|-------|----------|--------|-----------|
| Last backup age | `< 15 minutes` | | ☐ |
| Backup integrity (listable by pg_restore) | `PASS` | | ☐ |
| Retention pruning removing old backups | `PASS` | | ☐ |
| Offsite sync lag | `< 1 hour` | | ☐ |
| Offsite hash match | `MATCH` | | ☐ |

**Overall backup discipline**: `PASS / FAIL`

---

## Known Limitations at Time of Signoff

**Placeholder**: List any known PG-specific limitations.

- [ ] *(example)* Circuit breaker not implemented — single-node only.
- [ ] *(example)* HA/failover not implemented — manual recovery only.
- [ ] *(example)* Replication lag monitoring placeholder — real metric not available.
- [ ] *(add as applicable)*

---

## Non-Claims

- **NOT a production-ready claim by itself**: PostgreSQL production deployment is a prerequisite for production-ready, not sufficient alone.
- **NOT HA**: If HA is not implemented, this signoff covers single-node PostgreSQL only.
- **NOT validated for all configs**: TLS, PgBouncer, and alert validation are operator-environment-dependent.
- **NOT self-executing**: This template records signoff only after real deployment evidence exists.
- **NOT retroactive**: Signoff applies only to the specific PG version, ferrumd version, and environment listed.
- **Block A remains open unless separately closed**: PostgreSQL deployment does not close Block A (real domain).

---

## Signoff

### Planning/Template-Readiness Signoff (BrianNguyen)

> **Signed by**: BrianNguyen (session authorization)
> **Date**: 2026-05-22
> **Scope**: This template is reviewed and accepted as a valid PostgreSQL production deployment signoff form.
> **Nature**: Planning/decision document signoff only. This does **not** constitute evidence of a production PostgreSQL deployment or production readiness. Does **not** substitute for missing evidence.
> **Authority**: User explicitly authorized delegated signoff for planning and template readiness.

| Template Section | Status |
|-----------------|--------|
| Prerequisites checklist | ✅ Template ready |
| Deployment configuration table | ✅ Template ready |
| Health and metrics checks | ✅ Template ready |
| Backup discipline checks | ✅ Template ready |
| Non-claims section | ✅ Template ready |
| Final operator signoff block | ✅ Template ready (intentionally blank below) |

### Final Operator Signoff (Intentionally Blank — Requires Real Evidence)

> **Operator name**: ________________________
> **Date**: ________________________
> **Signature / Ack**: ________________________
>
> **I confirm that**:
> - A production PostgreSQL instance is deployed and reachable.
> - ferrumd starts, reports healthy, and remains stable with the PostgreSQL DSN.
> - Migration completed with validated row counts and hashes.
> - Backup, retention, and offsite discipline are executing.
> - Alert rules are deployed and validated.
> - I accept responsibility for ongoing PostgreSQL operations.

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | | |
| Operator (required) | | | |

---

## Related Docs

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md)
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md)
- [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md)
- [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-target-deployment-evidence.md`](./TEMPLATE-pg-target-deployment-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-tls-dsn-evidence.md`](./TEMPLATE-pg-tls-dsn-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-scheduled-backup-evidence.md`](./TEMPLATE-pg-scheduled-backup-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-retention-pruning-evidence.md`](./TEMPLATE-pg-retention-pruning-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-offsite-sync-evidence.md`](./TEMPLATE-pg-offsite-sync-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-alert-deployment-evidence.md`](./TEMPLATE-pg-alert-deployment-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-pgbouncer-evidence.md`](./TEMPLATE-pg-pgbouncer-evidence.md)
- [`docs/implementation-path/artifacts/PG-production-evidence-pack-runbook.md`](./PG-production-evidence-pack-runbook.md) — Operator execution guide for capturing each prerequisite (commands, redaction, pass/fail, rollback checks)
