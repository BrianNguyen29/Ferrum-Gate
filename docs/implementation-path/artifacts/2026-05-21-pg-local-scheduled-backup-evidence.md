# Local PostgreSQL Scheduled Backup Evidence — 2026-05-21

> **Status**: LOCAL EVIDENCE — non-production Docker environment only.
> **Purpose**: Validate `pg_dump` backup creation and integrity against a local Docker PostgreSQL instance.
> **Scope**: Local Docker Compose (`docker-compose.postgres.yml`). NOT a production PostgreSQL deployment.
> **Constraint**: `production-ready = NO`. Block A remains WAIVED/CONDITIONAL. Full G2 remains NOT COMPLETE.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Local Docker environment only |
| **Production PostgreSQL backup** | **NO** | Executed against local Docker container, not production PG |
| **Scheduled automation validated** | **NO** | Manual `pg_dump` execution only; cron/systemd timer not configured or tested |
| **Full G2** | **NOT COMPLETE** | Conditional pilot only |
| **Block A** | **WAIVED/CONDITIONAL** | No real domain |

---

## Metadata

| Field | Value |
|-------|-------|
| **Timestamp** | 2026-05-21 |
| **Environment** | Local Docker Compose (`docker-compose.postgres.yml`) |
| **Container** | `ferrumgate_postgres_p2` |
| **PostgreSQL image** | `postgres:16` |
| **pg_dump version** | `pg_dump (PostgreSQL) 16.13 (Debian 16.13-1.pgdg13+1)` |
| **pg_restore version** | `pg_restore (PostgreSQL) 16.13 (Debian 16.13-1.pgdg13+1)` |
| **Database** | `ferrumgate_p2_test` |
| **User** | `ferrumgate_dev` |
| **Evidence owner** | Engineering |

---

## T-BAK-1 — PostgreSQL Instance Health Check

**Command**:
```bash
docker compose -f docker-compose.postgres.yml up -d postgres_p2
```
**Result**: Container `ferrumgate_postgres_p2` started successfully.

**Command**:
```bash
docker inspect ferrumgate_postgres_p2 --format {{.State.Health.Status}}
```
**Result**: Initially `starting`, after wait `healthy`.

**Command**:
```bash
docker exec ferrumgate_postgres_p2 pg_isready -U ferrumgate_dev -d ferrumgate_p2_test
```
**Result**: `/var/run/postgresql:5432 - accepting connections`

**Command**:
```bash
docker exec ferrumgate_postgres_p2 psql -U ferrumgate_dev -d ferrumgate_p2_test -c "SELECT 1 AS local_pg_ready"
```
**Result**:
```
 local_pg_ready
----------------
              1
```

**Pass/Fail**: ✅ PASS

---

## T-BAK-2 — Backup Creation

**Command**:
```bash
docker exec ferrumgate_postgres_p2 pg_dump -U ferrumgate_dev -d ferrumgate_p2_test -Fc --no-owner --no-privileges -f /tmp/ferrumgate_local_20260521.dump
```
**Exit code**: `0`
**Output**: *(no output — exit 0 indicates success)*

**Command**:
```bash
docker cp ferrumgate_postgres_p2:/tmp/ferrumgate_local_20260521.dump /tmp/opencode/ferrumgate-pg-evidence/backups/ferrumgate_local_20260521.dump
```
**Exit code**: `0`

**File size verification**:
```bash
stat --format=%n:%s /tmp/opencode/ferrumgate-pg-evidence/backups/ferrumgate_local_20260521.dump
```
**Result**: `/tmp/opencode/ferrumgate-pg-evidence/backups/ferrumgate_local_20260521.dump:919`

**Pass/Fail**: ✅ PASS

---

## T-BAK-3 — Backup Integrity Verification

**Command**:
```bash
docker exec ferrumgate_postgres_p2 pg_restore -l /tmp/ferrumgate_local_20260521.dump
```
**Result**:
```
; Archive created at 2026-05-21 17:01:23 UTC
;     dbname: ferrumgate_p2_test
;     TOC Entries: 4
;     Compression: -1
;     Dump Version: 1.15-0
;     Format: CUSTOM
;     Integer: 4 bytes
;     Offset: 8 bytes
;     Dumped from database version: 16.13
;     Dumped by pg_dump version: 16.13 (Debian 16.13-1.pgdg13+1)
```
**Object count**: 4 TOC entries

**Pass/Fail**: ✅ PASS

---

## T-BAK-4 — Restore Drill

**Step 1 — Create drill database**:
```bash
docker exec ferrumgate_postgres_p2 psql -U ferrumgate_dev -d postgres -c "CREATE DATABASE ferrumgate_restore_drill_20260521;"
```
**Result**: Success

**Step 2 — Restore into drill database**:
```bash
docker exec ferrumgate_postgres_p2 pg_restore -U ferrumgate_dev -d ferrumgate_restore_drill_20260521 --no-owner --no-privileges /tmp/ferrumgate_local_20260521.dump
```
**Exit code**: `0`

**Step 3 — Verify table count**:
```bash
docker exec ferrumgate_postgres_p2 psql -U ferrumgate_dev -d ferrumgate_restore_drill_20260521 -c "SELECT COUNT(*) AS table_count FROM information_schema.tables WHERE table_schema='public'"
```
**Result**: `0` (empty baseline — consistent with source)

**Step 4 — Cleanup drill database**:
```bash
docker exec ferrumgate_postgres_p2 psql -U ferrumgate_dev -d postgres -c "DROP DATABASE ferrumgate_restore_drill_20260521;"
```
**Result**: Success

**Pass/Fail**: ✅ PASS

---

## T-BAK-5 — Limitations and Non-Production Caveats

| Limitation | Why it matters |
|------------|---------------|
| **Manual execution** | This was a manual `pg_dump`, not a scheduled cron/systemd timer. Scheduler configuration remains untested. |
| **Empty baseline** | The source database had 0 public tables. Row-count validation is trivial. Production databases will require more rigorous validation. |
| **Local Docker only** | No network latency, no real disk I/O variability, no production load. |
| **No scheduler test** | The 15-minute interval and automated firing were not tested. |
| **No retention pruning tested here** | Retention pruning is covered by a separate local evidence artifact. |
| **No offsite sync tested here** | Offsite sync is covered by a separate local evidence artifact. |

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | 2026-05-21 | Local execution |
| Operator | | | *(blank — operator signoff requires production execution)* |

---

## Related Docs

- [`docs/implementation-path/artifacts/TEMPLATE-pg-scheduled-backup-evidence.md`](./TEMPLATE-pg-scheduled-backup-evidence.md) — Full template for operator production execution
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-3
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md) §P5c.5
