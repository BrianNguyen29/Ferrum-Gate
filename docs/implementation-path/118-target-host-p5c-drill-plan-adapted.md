# 118 — Target-Host P5c Drill Plan (Adapted from Local Docker)

> **Status**: Adapted plan — ready for operator adaptation. **NOT executed**.  
> **Purpose**: Generic target-host adaptation of `111-p5c-local-docker-drill-plan.md` for P5c.V1 (backup) and P5c.V2 (restore) drills on a real PostgreSQL target.  
> **Scope**: Single-node PostgreSQL target host only. No production deployment. No HA/multi-node.  
> **Constraint**: This plan does NOT authorize production PostgreSQL deployment. Production remains gated on P5b–P5e completion and a future P6 assessment. Do not record secrets.

---

## 1. Plan Metadata

| Field | Value / Placeholder |
|-------|---------------------|
| Service name | `<TARGET_PG_SERVICE>` *(operator-defined)* |
| Target host | `<TARGET_HOST>` *(e.g., `db.example.com` or IP)* |
| Target port | `<TARGET_PORT>` *(default: `5432`)* |
| Target database | `<TARGET_DB>` *(e.g., `ferrumgate`)* |
| Backup user | `<BACKUP_USER>` *(dedicated backup role preferred)* |
| Backup directory | `<BACKUP_DIR>` *(e.g., `/var/backups/ferrumgate-postgres`)* |
| Drill log directory | `<DRILL_LOG_DIR>` *(e.g., `/var/log/ferrumgate-drills`)* |
| Expected DSN | `postgres://<BACKUP_USER>@<TARGET_HOST>:<TARGET_PORT>/<TARGET_DB>` *(password omitted)* |

---

## 2. Preconditions

Before starting the drill, confirm all items below. If any precondition fails, **stop** and resolve it before proceeding.

| # | Precondition | Verification Command | Status |
|---|---|---|---|
| P1 | Target PostgreSQL reachable from operator workstation | `pg_isready -h <TARGET_HOST> -p <TARGET_PORT>` returns `accepting connections` | ☐ |
| P2 | `pg_dump` and `pg_restore` client version ≥ server major version | `pg_dump --version && pg_restore --version` | ☐ |
| P3 | Backup destination directory exists and is writable on target or operator host | `ssh <TARGET_HOST> "mkdir -p <BACKUP_DIR> && test -w <BACKUP_DIR>"` | ☐ |
| P4 | D2=A signed (operator selected `pg_dump` logical backup) | `105-g3-5-operator-d1-d3-signoff-packet.md` | ☐ |
| P5 | `.pgpass` or `PGPASSFILE` configured (no inline passwords) | `chmod 600 ~/.pgpass` verified | ☐ |

---

## 3. Target-Host `.pgpass` / `PGPASSFILE` Guidance

Instead of passing passwords inline or via environment variables, use PostgreSQL's `.pgpass` mechanism.

### Option A — Per-User `.pgpass`

```bash
# Create ~/.pgpass on the operator workstation
cat > ~/.pgpass <<'EOF'
<TARGET_HOST>:<TARGET_PORT>:<TARGET_DB>:<BACKUP_USER>:<REDACTED>
EOF
chmod 600 ~/.pgpass
```

### Option B — Dedicated `PGPASSFILE`

```bash
# Create a dedicated passfile for FerrumGate drills
mkdir -p ~/.config/ferrumgate
cat > ~/.config/ferrumgate/.pgpass_drill <<'EOF'
<TARGET_HOST>:<TARGET_PORT>:<TARGET_DB>:<BACKUP_USER>:<REDACTED>
EOF
chmod 600 ~/.config/ferrumgate/.pgpass_drill
export PGPASSFILE="${HOME}/.config/ferrumgate/.pgpass_drill"
```

> **Redaction rule**: Before recording any command in evidence template `110`, replace the password with `<REDACTED>` and do not include `.pgpass` contents in evidence.

---

## 4. Step-by-Step Drill Procedure

### 4.1 Target-Host Connectivity Check

```bash
PGHOST="<TARGET_HOST>"
PGPORT="<TARGET_PORT>"
PGDATABASE="<TARGET_DB>"
PGUSER="<BACKUP_USER>"

# Connectivity
pg_isready -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${PGDATABASE}"

# Schema presence
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${PGDATABASE}" -c "\dt"
```

**Stop condition**: If `pg_isready` does not return `accepting connections`, verify network path, firewall rules, and `pg_hba.conf` on the target before continuing.

---

### 4.2 P5c.V1 — Target-Host Backup Drill

#### V1.1 Pre-Backup Checks

| # | Step | Command / Check | Status |
|---|---|---|---|
| V1-P1 | Verify target connectivity | `pg_isready -h "${PGHOST}" -p "${PGPORT}"` | ☐ |
| V1-P2 | Verify schema tables present | `psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${PGDATABASE}" -c "\dt"` | ☐ |
| V1-P3 | Record pre-backup row counts (key tables) | `SELECT COUNT(*) FROM intents;` etc. | ☐ |
| V1-P4 | Ensure backup directory exists | `mkdir -p "<BACKUP_DIR>"` | ☐ |
| V1-P5 | Confirm low-write window | Operator confirms minimal writes expected during backup | ☐ |

#### V1.2 Execute Backup

```bash
# Configuration
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="<BACKUP_DIR>/ferrumgate_${TIMESTAMP}.dump"

mkdir -p "<BACKUP_DIR>"

pg_dump \
  -h "${PGHOST}" \
  -p "${PGPORT}" \
  -U "${PGUSER}" \
  -d "${PGDATABASE}" \
  -Fc \
  -v \
  --no-owner \
  --no-privileges \
  -f "${BACKUP_FILE}"
```

Record in evidence template `110`:
- V1.1 exit code
- V1.2 backup file size (`ls -lh "${BACKUP_FILE}"`)
- V1.3 absolute path of backup file
- V1.4 SHA-256 checksum (`sha256sum "${BACKUP_FILE}"`)
- V1.5 `pg_restore -l "${BACKUP_FILE}"` result
- V1.6 whether backup was taken during low-write window
- V1.7 verification that no secrets were logged in command history

#### V1.3 Post-Backup Verification

```bash
# File size check
ls -lh "${BACKUP_FILE}"

# Integrity listing
pg_restore -l "${BACKUP_FILE}" > /dev/null && echo "Backup listable: OK" || echo "Backup listable: FAIL"

# Optional object count
OBJECT_COUNT=$(pg_restore -l "${BACKUP_FILE}" | wc -l)
echo "Backup objects: ${OBJECT_COUNT}"
```

#### V1 Stop Conditions

| Trigger | Action |
|---------|--------|
| `pg_dump` exits non-zero | Do not use backup; investigate connection, permissions, or disk space |
| Backup file size is 0 or missing | Retry; check disk space and write permissions |
| `pg_restore -l` fails | Backup may be corrupt; retry dump before scheduling |
| Password visible in shell history | Clear history; reconfigure `.pgpass`; do not record evidence until clean |

---

### 4.3 P5c.V2 — Target-Host Restore Drill

#### V2.1 Prepare Drill Target Database

```bash
# Configuration
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESTORE_LOG="<DRILL_LOG_DIR>/pg_restore_drill_${TIMESTAMP}.log"
DRILL_DB="ferrumgate_drill_${TIMESTAMP}"

mkdir -p "<DRILL_LOG_DIR>"

# Step 1: List backup contents (dry inspection)
echo "=== Step 1: List backup contents ===" | tee -a "${RESTORE_LOG}"
pg_restore -l "${BACKUP_FILE}" | tee -a "${RESTORE_LOG}"

# Step 2: Create drill target DB
echo "=== Step 2: Create drill target DB ===" | tee -a "${RESTORE_LOG}"
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d postgres \
  -c "CREATE DATABASE ${DRILL_DB};" 2>&1 | tee -a "${RESTORE_LOG}"
```

Record in evidence template `110`:
- V2.1 `pg_restore -l` dry inspection status
- V2.2 drill target database name (`${DRILL_DB}`)

#### V2.2 Restore into Drill Database

```bash
# Step 3: Restore into drill DB
echo "=== Step 3: Restore into drill DB ===" | tee -a "${RESTORE_LOG}"
pg_restore \
  -h "${PGHOST}" \
  -p "${PGPORT}" \
  -U "${PGUSER}" \
  -d "${DRILL_DB}" \
  --no-owner \
  --no-privileges \
  -v \
  "${BACKUP_FILE}" 2>&1 | tee -a "${RESTORE_LOG}"
RESTORE_EXIT=$?
echo "pg_restore exit code: ${RESTORE_EXIT}" | tee -a "${RESTORE_LOG}"
```

Record in evidence template `110`:
- V2.3 `pg_restore` exit code

#### V2.3 Post-Restore Verification

```bash
# Step 4: Verify tables and row counts
echo "=== Step 4: Row count verification ===" | tee -a "${RESTORE_LOG}"
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${DRILL_DB}" \
  -c "\dt" 2>&1 | tee -a "${RESTORE_LOG}"

# Example table count — adapt table names as needed
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${DRILL_DB}" \
  -c "SELECT COUNT(*) FROM intents;" 2>&1 | tee -a "${RESTORE_LOG}"
```

Record in evidence template `110`:
- V2.4 all expected tables present (YES/NO)
- V2.5 row counts for key tables
- V2.6 optional hash verification (if performed)

#### V2.4 Cleanup Drill Database

```bash
# Step 5: Drop drill database
echo "=== Step 5: Cleanup drill DB ===" | tee -a "${RESTORE_LOG}"
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d postgres \
  -c "DROP DATABASE ${DRILL_DB};" 2>&1 | tee -a "${RESTORE_LOG}"

echo "=== Restore drill complete ===" | tee -a "${RESTORE_LOG}"
echo "Log: ${RESTORE_LOG}"
```

Record in evidence template `110`:
- V2.7 drill database dropped after verification (YES/NO)
- V2.8 restore log absolute path

#### V2 Stop Conditions

| Trigger | Action |
|---------|--------|
| `pg_restore` exits non-zero | Do not proceed to production restore; investigate errors in log |
| Tables missing after restore | Investigate schema version mismatch; do not overwrite production |
| Row counts significantly different | Backup may be inconsistent; take new backup before proceeding |
| Drill DB not dropped | Complete cleanup before claiming V2 done |

---

## 5. Scheduler Verification Checklist

After confirming V1 and V2 work manually, verify the backup scheduler configuration:

### 5.1 Cron-Based Scheduler

| # | Check | Command / Verification | Status |
|---|---|---|---|
| S1-C1 | Cron file installed | `ls -l /etc/cron.d/ferrumgate-postgres-backup` or `crontab -l -u backupuser` | ☐ |
| S1-C2 | Cron interval matches RPO target | Confirm interval ≤ RPO (e.g., 15 min for RPO=15min) | ☐ |
| S1-C3 | PGPASSFILE referenced in cron | No inline passwords in cron line | ☐ |
| S1-C4 | Backup directory writable by cron user | `sudo -u backupuser touch <BACKUP_DIR>/test` | ☐ |
| S1-C5 | Retention policy configured | `find <BACKUP_DIR> -name "ferrumgate_*.dump" -mmin +<RETENTION_MINUTES> -delete` present | ☐ |
| S1-C6 | First scheduled backup produced | `ls -lt <BACKUP_DIR>/*.dump | head -n1` within one interval of enabling | ☐ |

### 5.2 Systemd Timer-Based Scheduler

| # | Check | Command / Verification | Status |
|---|---|---|---|
| S2-C1 | Service and timer files installed | `systemctl status ferrumgate-postgres-backup.service ferrumgate-postgres-backup.timer` | ☐ |
| S2-C2 | Timer interval matches RPO target | `systemctl list-timers ferrumgate-postgres-backup.timer` shows next trigger ≤ RPO | ☐ |
| S2-C3 | PGPASSFILE referenced in service | `Environment=PGPASSFILE=...` in service file; no inline passwords | ☐ |
| S2-C4 | Backup directory writable by service user | `sudo -u backupuser touch <BACKUP_DIR>/test` | ☐ |
| S2-C5 | Backup verify step present in service | `ExecStartPost` runs `pg_restore -l` or equivalent | ☐ |
| S2-C6 | First timer-fired backup produced | `journalctl -u ferrumgate-postgres-backup.service --since "1 hour ago"` shows success | ☐ |

---

## 6. Cleanup and Reset

After the drill is complete (success or failure), perform the following cleanup:

| Step | Command | Purpose |
|------|---------|---------|
| C1 | `rm -f ~/.config/ferrumgate/.pgpass_drill` | Remove drill-specific `.pgpass` file |
| C2 | `rm -f <BACKUP_DIR>/*.dump` | Remove drill backup artifacts (retain per retention policy if transitioning to production) |
| C3 | `rm -f <DRILL_LOG_DIR>/pg_restore_drill_*.log` | Remove drill restore logs |
| C4 | `history -c` or review shell history | Ensure no secrets remain in shell history |

---

## 7. Approval Checklist

This plan must be reviewed and approved by an operator before execution. Check each item:

| # | Criterion | Status |
|---|-----------|--------|
| A1 | Operator has read `109-p5c-postgresql-backup-restore-runbook.md` | ☐ |
| A2 | Operator has the fillable `110-p5c-postgresql-drill-evidence-template.md` ready | ☐ |
| A3 | Operator confirms this is a **target-host rehearsal** on non-production or staging systems only | ☐ |
| A4 | Operator confirms `.pgpass` or `PGPASSFILE` will be used instead of inline passwords | ☐ |
| A5 | Operator confirms all commands will be redacted (`<REDACTED>`) before recording evidence | ☐ |
| A6 | Operator confirms cleanup steps (C1–C4) will be executed after drill | ☐ |
| A7 | Operator acknowledges production PostgreSQL deployment remains **NO** until P5b–P5e and P6 are complete | ☐ |

**Approver Name**: ____________________  
**Date**: ____________________  
**Signature**: ____________________

---

## 8. Evidence Mapping

Use this mapping to transfer drill results into the evidence template.

| This Plan Step | Evidence Template Field (Doc 110) | What to Record |
|----------------|-----------------------------------|----------------|
| 4.2 V1.2 | V1.1 | `pg_dump` exit code |
| 4.2 V1.2 | V1.2 | Backup file size |
| 4.2 V1.2 | V1.3 | Backup absolute path |
| 4.2 V1.2 | V1.4 | SHA-256 checksum |
| 4.2 V1.3 | V1.5 | `pg_restore -l` OK/FAIL |
| 4.2 V1.1 | V1.6 | Low-write window YES/NO/N/A |
| 4.2 V1.1 | V1.7 | No secrets in history VERIFIED |
| 4.3 V2.1 | V2.1 | `pg_restore -l` dry inspection OK |
| 4.3 V2.1 | V2.2 | Drill DB name |
| 4.3 V2.2 | V2.3 | `pg_restore` exit code |
| 4.3 V2.3 | V2.4 | Expected tables present YES/NO |
| 4.3 V2.3 | V2.5 | Row counts (key tables) |
| 4.3 V2.3 | V2.6 | Optional hash |
| 4.3 V2.4 | V2.7 | Drill DB dropped YES/NO |
| 4.3 V2.4 | V2.8 | Restore log path |
| 5.1/5.2 | S1-C6 / S2-C6 | Scheduler first-fire verification |

---

## 9. Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `118-target-host-p5c-drill-plan-adapted.md` | `110-p5c-postgresql-drill-evidence-template.md` | Fillable evidence template for V1/V2 results |
| `118-target-host-p5c-drill-plan-adapted.md` | `109-p5c-postgresql-backup-restore-runbook.md` | Backup/restore commands and acceptance criteria |
| `118-target-host-p5c-drill-plan-adapted.md` | `114-target-host-p5c-drill-checklist.md` | Operator checklist for target-host execution |
| `118-target-host-p5c-drill-plan-adapted.md` | `111-p5c-local-docker-drill-plan.md` | Original local drill plan this document adapts |
| `118-target-host-p5c-drill-plan-adapted.md` | `configs/examples/postgres-target-env.template` | Environment-variable-only connection template |

---

## 10. Safety and Non-Claims

- **Not executed**: This document is a plan only. It does not claim any drill has been run.
- **Target-host only**: All commands target a placeholder `<TARGET_HOST>`. Do not run against production databases without explicit operator approval.
- **No production-ready claim**: Production PostgreSQL deployment remains gated on P5b–P5e completion and a future P6 assessment.
- **No HA/multi-node**: Single-node only. No read replicas, no streaming replication, no failover.
- **Secret safety**: All credentials are placeholders (`<REDACTED>`). Do not record actual passwords in evidence templates without redaction.
- **Cleanup required**: Drill artifacts (backups, logs, `.pgpass`) must be removed after use.

---

*Document created: 2026-05-12. Adapted Target-Host P5c Drill Plan — pending operator adaptation and approval. NOT executed. No production-ready claim. No HA/multi-node.*
