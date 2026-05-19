# DEP-6 Hosted Backup-Mode Planning/Preflight — Prepared Only

> **Status**: PREPARED — planning checklist and preflight steps ready for operator execution. DEP-6 remains OPEN until a hosted backup/restore drill is executed and evidence is captured.
> **Date**: 2026-05-19
> **Scope**: Hosted backup/restore planning for SQLite (single-node) and PostgreSQL (production foundation) deployment modes. NOT production-ready. NOT HA.
> **Owner**: Operator + Engineering

---

## Non-claims

- **DEP-6 is NOT complete**: This document is a planning/preflight artifact only. It does not constitute evidence of a successful hosted backup/restore drill.
- **NOT production-ready**: Backup packaging is a prerequisite, not a production claim.
- **NOT validated in hosted mode**: The existing PG-3 restore drill is local-only. A target-host or hosted-environment drill is required to close DEP-6.
- **NOT a replacement for operator runbooks**: This is a preflight checklist. Operational cadence and incident response remain separate.
- **NO real secrets in this doc**: All tokens, DSNs, offsite URLs, and bucket names are placeholders. Redact sensitive values in evidence artifacts.
- **NOT a guarantee of RPO/RTO**: RPO/RTO must be measured during the actual drill; values here are planning targets only.

---

## Prerequisites

| # | Item | Verification |
|---|------|-------------|
| P1 | Deployment mode is defined (SQLite single-node OR PostgreSQL self-hosted) | See `docs/guides/hosted-deployment.md` |
| P2 | If PostgreSQL: PG-1 baseline is complete and `docs/production-readiness-v2/02-postgres-production-plan.md` PG-3 local drill passed | `docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md` |
| P3 | Backup storage directory exists with correct ownership and permissions | `ls -ld /var/backups/ferrumgate` |
| P4 | Backup user/service account exists (e.g., `backupuser` for PostgreSQL) | `id backupuser` |
| P5 | Retention policy is defined (e.g., keep last 24 hourly, 7 daily, 4 weekly) | Documented below |
| P6 | Offsite sync target is identified (if applicable) — e.g., S3-compatible bucket, rsync target, or tape | Placeholder only; redact real URLs/buckets in evidence |
| P7 | `ferrumctl` is available on PATH if using SQLite mode | `ferrumctl --version` |
| P8 | `pg_dump` and `pg_restore` are available if using PostgreSQL mode | `pg_dump --version && pg_restore --version` |
| P9 | systemd is available if using timer-based scheduled backups | `systemctl --version` |

---

## Planning checklist

### 1. Choose backup strategy by deployment mode

| Mode | Strategy | Tool | Schedule |
|------|----------|------|----------|
| SQLite single-node | File-level snapshot + `ferrumctl backup` | `ferrumctl` + `tar` / `rsync` | Daily or before upgrades |
| PostgreSQL self-hosted | `pg_dump` custom-format + verify | `pg_dump` / `pg_restore` | Every 15 min (see `postgres-backup.timer`) or per RPO target |

### 2. Define retention policy

Example retention matrix (adjust to operator risk posture):

| Tier | Count | Purpose |
|------|-------|---------|
| Hourly | 24 | Point-in-time recovery within a day |
| Daily | 7 | Week-long recovery window |
| Weekly | 4 | Month-long recovery window |
| Monthly | 3 | Quarter-long archive |

**Action**: Document the chosen retention matrix in the operator runbook. Do not rely on defaults.

### 3. Define RPO/RTO targets (planning values)

| Metric | SQLite pilot target | PostgreSQL target |
|--------|---------------------|-------------------|
| RPO | 24 hours | 15 minutes |
| RTO | 30 minutes | 15 minutes |

**Non-claim**: These are planning targets. Actual RPO/RTO must be measured during the drill and recorded in the evidence artifact.

### 4. Identify offsite sync target (if applicable)

| Attribute | Placeholder / Check |
|-----------|---------------------|
| Target type | `s3://BUCKET_NAME/ferrumgate-backups/` OR `rsync://HOST/path` OR `nfs:/path` |
| Authentication | IAM role / API key / SSH key — redact in evidence |
| Encryption at rest | Yes / No — must be Yes for production path |
| Encryption in transit | TLS / SSH — must be enabled |
| Bandwidth/latency | Measure with `aws s3 cp` or `rsync --dry-run` |

**Action**: Run a dry-run sync to validate connectivity and bandwidth. Do not upload real backups until the preflight passes.

---

## Preflight / dry-run checks

### Check A — SQLite mode preflight

```bash
# 1. Verify ferrumctl can create a backup
sudo -u ferrumgate ferrumctl backup --output /var/backups/ferrumgate/ferrumgate_$(date +%Y%m%d_%H%M%S).db

# 2. Verify backup file exists and is non-empty
ls -lh /var/backups/ferrumgate/*.db | tail -n 5

# 3. Verify backup integrity (SQLite can open it)
sqlite3 /var/backups/ferrumgate/<latest>.db "PRAGMA integrity_check;"

# 4. Verify restore dry-run (to a temp path)
cp /var/backups/ferrumgate/<latest>.db /tmp/ferrumgate_restore_test.db
sqlite3 /tmp/ferrumgate_restore_test.db "SELECT COUNT(*) FROM sqlite_master;"
rm /tmp/ferrumgate_restore_test.db
```

**Evidence to capture:**
- `ferrumctl backup` output
- `ls -lh` showing backup file size and timestamp
- `PRAGMA integrity_check;` output (must be `ok`)
- Restore test row count

### Check B — PostgreSQL mode preflight

```bash
# 1. Verify pg_dump connectivity and create a test dump
PGPASSFILE=/etc/ferrumgate/.pgpass \
pg_dump -h localhost -p 5432 -U backupuser -d ferrumgate \
  -Fc --no-owner --no-privileges \
  -f /var/backups/ferrumgate-postgres/ferrumgate_preflight_$(date +%Y%m%d_%H%M%S).dump

# 2. Verify dump file exists and is non-empty
ls -lh /var/backups/ferrumgate-postgres/*.dump | tail -n 5

# 3. Verify dump restoreability (dry-run table of contents)
pg_restore -l /var/backups/ferrumgate-postgres/<latest>.dump > /dev/null && echo "RESTORE_LIST_OK" || echo "RESTORE_LIST_FAILED"

# 4. Verify a full restore to a CLEAN test database
# Create test DB first (operator with createdb privileges)
createdb -h localhost -U postgres ferrumgate_restore_test
pg_restore -h localhost -U postgres -d ferrumgate_restore_test \
  --no-owner --no-privileges \
  /var/backups/ferrumgate-postgres/<latest>.dump

# 5. Validate row counts match
psql -h localhost -U postgres -d ferrumgate_restore_test -c "
SELECT
  (SELECT COUNT(*) FROM policies) AS policy_count,
  (SELECT COUNT(*) FROM executions) AS execution_count,
  (SELECT COUNT(*) FROM intents) AS intent_count;
"

# 6. Drop test database
dropdb -h localhost -U postgres ferrumgate_restore_test
```

**Evidence to capture:**
- `pg_dump` command and exit code
- Dump file size and timestamp
- `pg_restore -l` result
- Test restore exit code
- Row count comparison between source and restored test DB
- `dropdb` confirmation

### Check C — Scheduled backup unit preflight (PostgreSQL)

```bash
# 1. Install timer and service
sudo cp configs/examples/postgres-backup.service /etc/systemd/system/ferrumgate-backup.service
sudo cp configs/examples/postgres-backup.timer   /etc/systemd/system/ferrumgate-backup.timer

# 2. Adapt paths and credentials in the service file before installing
#    - Ensure ExecStart path matches your pg_dump location
#    - Ensure PGPASSFILE path is correct
#    - Ensure backup directory exists

# 3. Validate unit syntax
systemd-analyze verify /etc/systemd/system/ferrumgate-backup.service
systemd-analyze verify /etc/systemd/system/ferrumgate-backup.timer

# 4. Enable timer
sudo systemctl daemon-reload
sudo systemctl enable ferrumgate-backup.timer

# 5. Trigger a manual test run
sudo systemctl start ferrumgate-backup.service
systemctl status ferrumgate-backup.service --no-pager

# 6. Verify backup file was created
ls -lht /var/backups/ferrumgate-postgres/*.dump | head -n 5
```

**Evidence to capture:**
- `systemd-analyze verify` output for both units
- `systemctl status ferrumgate-backup.service --no-pager` after manual run
- `journalctl -u ferrumgate-backup.service --no-pager -n 30`
- List of backup files created

### Check D — Retention pruning dry-run

```bash
# Example: keep last 24 backups, delete older
# DRY-RUN first (do not delete)
ls -1t /var/backups/ferrumgate-postgres/*.dump | tail -n +25

# If dry-run list looks correct, implement pruning in a wrapper script
# or via logrotate / tmpwatch / custom script.
# NEVER prune without a dry-run review.
```

**Evidence to capture:**
- Dry-run list of files that would be deleted
- Retention script contents (if created)
- Operator approval of retention policy

### Check E — Offsite sync dry-run (if applicable)

```bash
# Example S3 dry-run (replace with your actual tool)
# aws s3 sync --dryrun /var/backups/ferrumgate-postgres/ s3://<BUCKET_NAME>/ferrumgate-backups/

# Example rsync dry-run
# rsync -avzn /var/backups/ferrumgate-postgres/ backup-host:/backups/ferrumgate/
```

**Evidence to capture:**
- Dry-run output showing files that would be transferred
- Network bandwidth measurement (e.g., `time aws s3 cp <test-file> s3://...`)
- Authentication method (redact keys/tokens)

---

## Required evidence for DEP-6 closure

DEP-6 can only be marked complete when ALL of the following evidence is captured:

1. **Backup creation evidence**
   - Tool version (`pg_dump --version` or `ferrumctl --version`)
   - Backup command executed (sensitive values redacted)
   - Backup file path, size, and checksum (`sha256sum`)
   - Backup timestamp

2. **Backup integrity evidence**
   - For SQLite: `PRAGMA integrity_check;` output
   - For PostgreSQL: `pg_restore -l` output and verify result

3. **Restore drill evidence**
   - Clean restore target provisioned (empty DB or temp DB)
   - Restore command executed (sensitive values redacted)
   - Restore exit code
   - Row counts and/or content hash comparison between source and restored data
   - `/v1/readyz/deep` pass after restore (if ferrumd is pointed at restored DB)

4. **Scheduled backup evidence (if applicable)**
   - systemd timer `systemctl list-timers ferrumgate-backup.timer`
   - At least one timer-fired backup file exists
   - Timer-fired backup passes integrity check

5. **Retention pruning evidence**
   - Retention policy documented
   - Pruning script or command
   - Before/after file listing showing old backups removed

6. **Offsite sync evidence (if applicable)**
   - Sync target type and path (bucket name redacted if sensitive)
   - At least one successful sync completed
   - Synced file verified at destination (e.g., `aws s3 ls` or remote `ls`)

7. **RPO/RTO measurement**
   - Time between last backup and restore start (RPO)
   - Time between restore start and `/v1/readyz/deep` 200 (RTO)

8. **Operator signoff**
   - Operator name/date confirming the drill was executed and evidence reviewed.

---

## Rollback and cleanup

| Scenario | Action |
|----------|--------|
| Preflight fails at Check A (SQLite backup) | Verify `ferrumctl` permissions on `/var/lib/ferrumgate`; verify disk space; check `ferrumd` is not holding an exclusive lock |
| Preflight fails at Check B (pg_dump) | Verify `PGPASSFILE` permissions (must be 600); verify `backupuser` has `CONNECT` and `SELECT` on `ferrumgate` DB; check PostgreSQL logs |
| Preflight fails at Check C (timer) | Verify `systemd-analyze verify` output; verify backup directory exists and `backupuser` can write to it |
| Restore test fails | Verify dump file is not truncated; verify target DB is clean (no existing objects); check `pg_restore` verbose output |
| Retention pruning too aggressive | Restore from offsite sync or earlier backup; adjust retention script; re-run drill |
| Offsite sync fails | Verify credentials/network; verify bucket/URL exists; check firewall/egress rules |
| Full rollback to pre-deployment state | `sudo systemctl disable --now ferrumgate-backup.timer`; `sudo rm /etc/systemd/system/ferrumgate-backup.*`; `sudo systemctl daemon-reload`; preserve backup files before any cleanup |

---

## Evidence artifact template

When DEP-6 is executed in a hosted environment, create an evidence artifact named:

```
docs/implementation-path/artifacts/YYYY-MM-DD-dep6-hosted-backup-restore-evidence.md
```

Populate it with:

1. Deployment mode (SQLite or PostgreSQL).
2. Target host OS/version.
3. All required evidence items listed in "Required evidence for DEP-6 closure" above.
4. Any deviations from this preflight and operator signoff.
5. Explicit non-claims matching this runbook.

---

## Related docs

- [`docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md`](./2026-05-18-pg-restore-drill-evidence.md) — Local-only PG-3 restore drill (does not close DEP-6).
- [`configs/examples/postgres-backup.service`](../../../configs/examples/postgres-backup.service) — PostgreSQL backup service example.
- [`configs/examples/postgres-backup.timer`](../../../configs/examples/postgres-backup.timer) — PostgreSQL backup timer example.
- [`docs/guides/hosted-deployment.md`](../../guides/hosted-deployment.md) — Deployment mode overview.
- [`docs/guides/operator.md`](../../guides/operator.md) — Operator guide (config, backup, incident response).
- [`docs/production-readiness-v2/08-hosted-deployment-plan.md`](../../production-readiness-v2/08-hosted-deployment-plan.md) — Hosted deployment plan.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) — PostgreSQL hardening prerequisites.
