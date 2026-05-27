# PostgreSQL Backup/Restore Drill Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-pg-restore-drill-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 1 — PG-P.5 (Backup/restore drill passes)
> **Constraint**: Target VM deployment, GCS offsite. No production-ready claim.

---

## 1. Summary

This artifact records the successful setup of automated PostgreSQL backups with retention and offsite sync, plus a full restore drill validating backup integrity.

---

## 2. Backup Configuration

### Backup Schedule

| Parameter | Value |
|-----------|-------|
| Backup method | pg_dump -Fc (custom format) |
| Frequency | Every 15 minutes |
| Retention | 4 days (5760 minutes) |
| Backup directory | /var/backups/ferrumgate-postgres |
| Offsite target | gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/ |
| Offsite method | gsutil rsync |

### Systemd Timers

| Timer | Status | Next Trigger |
|-------|--------|--------------|
| ferrumgate-postgres-backup.timer | active | Every 15 min |
| ferrumgate-postgres-retention.timer | active | Hourly |

---

## 3. Restore Drill Procedure

1. Created drill database: `ferrumgate_restore_drill`
2. Restored from latest backup: `ferrumgate-20260527-045126.dump` (308K)
3. Verified row counts match between main and restored database
4. Started temporary ferrumd against restored database on port 19099
5. Verified `/v1/readyz/deep` returns 200
6. Cleaned up drill database

---

## 4. Evidence

| Check | Result |
|-------|--------|
| Backup script created | PASS |
| Backup timer active | PASS |
| Retention timer active | PASS |
| Initial backup created | PASS (308K) |
| Backup listable with pg_restore | PASS |
| Restore drill completed | PASS |
| Row counts match | PASS |
| Temporary ferrumd healthy | PASS |
| Offsite sync to GCS | PASS |

### Row Count Verification

| Table | Main DB | Restored DB | Match |
|-------|---------|-------------|-------|
| intents | 4,459 | 4,459 | ✅ |
| proposals | 13 | 13 | ✅ |
| capabilities | 13 | 13 | ✅ |
| provenance_events | 26 | 26 | ✅ |

### Temporary ferrumd Health Check

```json
{
  "status": "ok",
  "healthy": true,
  "components": [
    {"component": "store", "status": "ok", "healthy": true},
    {"component": "write_queue", "status": "ok: depth=0, threshold=100", "healthy": true},
    {"component": "pool", "status": "ok: idle=1/total=2/max=10", "healthy": true}
  ]
}
```

### Offsite Sync Result

```
Copying file:///var/backups/ferrumgate-postgres/ferrumgate-20260527-045126.dump ...
Operation completed over 1 objects/307.5 KiB.
```

### GCS Bucket Listing

```
gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/ferrumgate-20260527-045126.dump
```

---

## 5. Boundary and Non-Claims

- **Logical backup**: pg_dump custom format, not WAL archiving/PITR.
- **4-day retention**: Suitable for nonprod; production may require longer.
- **No production-ready claim**: Backup/restore validated on target VM only.

---

## 6. Related Artifacts

- [`2026-05-27-pg-pgbouncer-evidence.md`](./2026-05-27-pg-pgbouncer-evidence.md) — PgBouncer deployment
- [`2026-05-27-pg-alert-deployment-evidence.md`](./2026-05-27-pg-alert-deployment-evidence.md) — Alert rules

---

*Artifact created: 2026-05-27. PostgreSQL backup/restore drill evidence. No production-ready claim.*
