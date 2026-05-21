# TEMPLATE — PostgreSQL Scheduled Backup Validation Evidence

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output.
> See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) Phase PG-3 and [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md) §P5c.5 for the runbook.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `staging / production / local-docker-compose` |
| **Operator** | `name` |
| **Scheduler type** | `cron / systemd timer / external CI` |
| **Backup interval** | `15 minutes` |
| **Backup directory** | `/var/backups/ferrumgate-postgres/` |
| **Evidence owner** | Operator |

---

## T-BAK-1 — Scheduler Configuration Review

**Check**: Scheduler is configured to run `pg_dump` at the approved interval.

- **Config file path**: `/etc/cron.d/ferrumgate-postgres-backup` or `/etc/systemd/system/ferrumgate-postgres-backup.timer`
- **Schedule expression**: `*/15 * * * *` or `OnCalendar=*:0/15`
- **Command executed by scheduler**:
  ```bash
  (paste exact command from cron/systemd unit)
  ```
- **User context**: `backupuser` (must not be root)
- **Credential method**: `PGPASSFILE` / env file / other (must not inline password)
- **Config reviewed for secrets**: `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-BAK-2 — Backup Job Execution Observation

**Check**: At least one scheduled backup executes successfully after configuration.

- **Observation start**: `YYYY-MM-DD HH:MM:SS UTC`
- **Observation end**: `YYYY-MM-DD HH:MM:SS UTC`
- **Number of scheduled intervals observed**: `N`
- **Number of successful backups**: `N`
- **Number of failed backups**: `N`
- **Most recent backup file**: `/var/backups/ferrumgate-postgres/ferrumgate_YYYYMMDD_HHMMSS.dump`
- **File size**: `N MiB`
- **Backup log location**: `/var/log/ferrumgate-postgres-backup.log`
- **Log excerpt**:
  ```
  (paste last successful backup log)
  ```

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-BAK-3 — Backup Integrity Verification

**Check**: Each backup can be listed by `pg_restore` and contains expected objects.

- **Verification command**:
  ```bash
  pg_restore -l /var/backups/ferrumgate-postgres/ferrumgate_YYYYMMDD_HHMMSS.dump > /dev/null && echo "OK" || echo "FAIL"
  ```
- **Object count**: `N`
- **Key tables present**: `intents`, `proposals`, `capabilities`, `executions`, `rollback_contracts`, `_schema_version`, `audit_logs` (list all)
- **Verification result**: `OK / FAIL`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-BAK-4 — Backup Timing Consistency

**Check**: Backups occur at the configured interval (±tolerance).

- **Observed interval between last 3 backups**:
  | Backup file | Timestamp | Interval since previous |
  |-------------|-----------|------------------------|
  | `ferrumgate_..._0015.dump` | `HH:MM` | — |
  | `ferrumgate_..._0030.dump` | `HH:MM` | `15 min` |
  | `ferrumgate_..._0045.dump` | `HH:MM` | `15 min` |
- **Acceptable tolerance**: `±2 minutes`
- **Deviations observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-BAK-5 — RPO Compliance Check

**Check**: Effective RPO is within the approved 15-minute target.

- **Backup interval**: `15 minutes`
- **Worst-case data loss window**: `15 minutes` (assuming backup succeeds at each interval)
- **Effective RPO**: `≤ 15 minutes`
- **Meets target**: `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## Known Gaps at Time of Evidence

- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: Scheduled backup validation is one component of PostgreSQL hardening. Production readiness requires PG-1 through PG-5 and operator signoff.
- **NOT restore verification**: This evidence validates backup creation only. Restore verification is a separate template (`TEMPLATE-pg-restore-drill-evidence.md`).
- **NOT retention verification**: Retention pruning verification is a separate template (`TEMPLATE-pg-retention-pruning-evidence.md`).
- **NOT offsite verification**: Offsite sync verification is a separate template (`TEMPLATE-pg-offsite-sync-evidence.md`).
- **NOT Block A closed**: Block A (real owned domain + DNS) remains WAIVED/CONDITIONAL.
- **NOT full G2**: G2 operator signoff requires real domain and final evidence pack review.

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Operator | | | |
| Engineering (witness) | | | |

---

## Related Docs

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-3
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md) §P5c.5
- [`docs/implementation-path/artifacts/TEMPLATE-pg-retention-pruning-evidence.md`](./TEMPLATE-pg-retention-pruning-evidence.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-offsite-sync-evidence.md`](./TEMPLATE-pg-offsite-sync-evidence.md)
- [`docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md`](./2026-05-21-phase-b-pg-production-foundation-prep.md)
