# 109 — P5c PostgreSQL Backup / Restore Operator Runbook

> **Status**: Documentation/design complete. Ready for operator review. P5c is design/docs only; no live DB operations.  
> **Purpose**: Operator runbook for PostgreSQL logical backup (`pg_dump`) and restore (`pg_restore`) under D2=A (external scheduler, operator-owned).  
> **Scope**: Single-node PostgreSQL only. No HA, no streaming replication, no external backup tools.  
> **Constraint**: This document does NOT authorize production PostgreSQL deployment. Production deployment remains gated on P5b–P5e completion and P6 assessment. Do not execute against production databases without operator signoff.

---

## Purpose

This runbook provides the operator-facing backup and restore procedures for a single-node PostgreSQL store (D1=A, D2=A, D3=A). It covers:

- **P5c.1** — `pg_dump` backup procedure
- **P5c.2** — `pg_restore` restore drill
- **P5c.3** — RPO/RTO targets (operator-approved)
- **P5c.4** — Snapshot consistency guidance
- **P5c.5** — External scheduler examples (cron / systemd timer)

**Operator-owned**: Scheduling, retention, credential management, and restore drills are operator responsibilities. FerrumGate does not provide in-tree backup automation.

---

## Explicit Non-Claims

- **No production-ready claim**: This runbook is design/documentation only. It does not make FerrumGate production-ready.
- **No PostgreSQL production deployment**: Production deployment of PostgreSQL remains gated on P5b–P5e completion and a future P6 assessment.
- **No HA/multi-node**: D1=A explicitly selects single-node PostgreSQL. Read replicas, clustering, and failover automation are out of scope.
- **No streaming replication**: D2=A selects `pg_dump` logical backup only. Streaming replication and tools such as pgBackRest are out of scope.
- **No automated failover**: D3=A selects none/manual recovery. No failover automation is documented or implied.
- **No in-tree scheduler**: Backup scheduling is external to FerrumGate (cron, systemd timer, or CI job). FerrumGate does not manage scheduler state.
- **No secret management**: Database credentials, bearer tokens, and TLS keys remain operator-managed. Do not commit secrets to version control.
- **RPO/RTO are targets, not guarantees**: Approved targets define the operator's design goal. Actual achievable RPO/RTO depend on workload, scheduler latency, and operator execution time.

---

## Prerequisites

Before using this runbook, confirm the following:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | Single-node PostgreSQL running (local Docker or non-prod) | `pg_isready -h <host> -p <port>` returns `accepting connections` | ☐ Operator confirms |
| R2 | `pg_dump` and `pg_restore` available (client tools) | `pg_dump --version` and `pg_restore --version` succeed | ☐ Operator confirms |
| R3 | Backup destination directory exists and is writable | `test -w <backup-dir>` | ☐ Operator confirms |
| R4 | FerrumGate schema initialized in target database | `\dt` in target DB shows FerrumGate tables | ☐ Operator confirms |
| R5 | D2=A signed (operator selected `pg_dump` logical backup) | `105-g3-5-operator-d1-d3-signoff-packet.md` | ☑ DONE |

---

## P5c.1 — pg_dump Backup Procedure

### 1.1 Backup Command

Run `pg_dump` as a PostgreSQL superuser or a user with `SELECT` on all FerrumGate tables. Use the **custom format** (`-Fc`) for flexibility during restore.

```bash
# Configuration — replace placeholders before running
PGHOST="<postgres-host>"
PGPORT="<postgres-port>"
PGDATABASE="<ferrumgate-db>"
PGUSER="<backup-user>"
# PGPASSWORD should be set via env file or prompt; do NOT hardcode in scripts
BACKUP_DIR="/var/backups/ferrumgate-postgres"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.dump"

# Ensure backup directory exists
mkdir -p "${BACKUP_DIR}"

# Run pg_dump with custom format and verbose output
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

**Options explained**:

| Option | Purpose |
|--------|---------|
| `-Fc` | Custom format — enables selective restore and parallel restore |
| `-v` | Verbose — logs progress to stderr for operator visibility |
| `--no-owner` | Omit `ALTER OWNER` commands; useful when restoring to a different user |
| `--no-privileges` | Omit `GRANT`/`REVOKE`; privileges are operator-managed |

### 1.2 Post-Backup Verification

Verify the backup file is non-empty and can be listed by `pg_restore`:

```bash
# File size check
ls -lh "${BACKUP_FILE}"

# Integrity listing (does not validate all data, but confirms format)
pg_restore -l "${BACKUP_FILE}" > /dev/null && echo "Backup listable: OK" || echo "Backup listable: FAIL"

# Optional: count objects in backup
OBJECT_COUNT=$(pg_restore -l "${BACKUP_FILE}" | wc -l)
echo "Backup objects: ${OBJECT_COUNT}"
```

### 1.3 Backup Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| `pg_dump` exit code | 0 | |
| Backup file exists and size > 0 | true | |
| `pg_restore -l` succeeds | true | |
| No secrets logged in command history | true | |

### 1.4 Stop Conditions

| Trigger | Action |
|---------|--------|
| `pg_dump` exits non-zero | Do not use backup; investigate connection, permissions, or disk space |
| Backup file size is 0 or missing | Retry; check disk space and write permissions |
| `pg_restore -l` fails | Backup may be corrupt; retry dump before scheduling |

---

## P5c.4 — Snapshot Consistency Guidance

### 4.1 Consistency Options

For a single-node PostgreSQL instance, choose one of the following consistency strategies:

| Strategy | Command | Tradeoff |
|----------|---------|----------|
| **A — Concurrent dump (default)** | `pg_dump` without extra flags | May read slightly inconsistent state if writes occur mid-dump. Acceptable for D2=A low-concurrency workloads. |
| **B — Stop writes briefly** | Pause FerrumGate writes during dump window | Strongest consistency; requires operator coordination or maintenance window. |
| **C — Read-only replica snapshot** | Not applicable for D1=A | Skipped; D1=A excludes read replicas. |

### 4.2 Recommended Approach for D1=A

**Strategy A (concurrent dump)** is the default for D1=A single-node PostgreSQL with `pg_dump` logical backup. The operator should schedule backups during low-write windows.

If stronger consistency is required, the operator may:

1. Reduce FerrumGate write volume during the backup window (e.g., pause non-critical automation), or
2. Accept that a logical backup under concurrent writes may capture a transactionally inconsistent snapshot.

> **Note**: `pg_dump --snapshot` requires a specific snapshot ID from an open transaction and is generally used in replication contexts. For standalone `pg_dump`, the utility already uses a single read transaction for each table. Cross-table consistency is best-effest unless writes are paused or a replication snapshot is used. D1=A does not provide replication snapshots.

### 4.3 Consistency Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| Backup taken during low-write window | true (operator preference) | |
| Operator aware of best-effort cross-table consistency | true | |
| No requirement for read-replica or streaming snapshot | true (D1=A) | |

---

## P5c.2 — pg_restore Restore Drill

### 2.1 Restore Drill Procedure

Execute this drill in a **non-production environment** first. Do not test restore against a production database.

```bash
# Configuration
PGHOST="<postgres-host>"
PGPORT="<postgres-port>"
PGDATABASE="<ferrumgate-db>"
PGUSER="<restore-user>"
BACKUP_FILE="<path-to-backup>.dump"
RESTORE_LOG="/tmp/pg_restore_drill_${TIMESTAMP}.log"

# Step 1: List backup contents (dry inspection)
echo "=== Step 1: List backup contents ===" | tee -a "${RESTORE_LOG}"
pg_restore -l "${BACKUP_FILE}" | tee -a "${RESTORE_LOG}"

# Step 2: Create empty target database for drill
echo "=== Step 2: Create drill target DB ===" | tee -a "${RESTORE_LOG}"
DRILL_DB="ferrumgate_drill_${TIMESTAMP}"
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d postgres \
  -c "CREATE DATABASE ${DRILL_DB};" 2>&1 | tee -a "${RESTORE_LOG}"

# Step 3: Restore into drill database
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

# Step 4: Verify row counts match expected tables
echo "=== Step 4: Row count verification ===" | tee -a "${RESTORE_LOG}"
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${DRILL_DB}" \
  -c "\dt" 2>&1 | tee -a "${RESTORE_LOG}"

# Example: verify a key table has rows (adapt table names as needed)
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${DRILL_DB}" \
  -c "SELECT COUNT(*) FROM intents;" 2>&1 | tee -a "${RESTORE_LOG}"

# Step 5: Cleanup drill database
echo "=== Step 5: Cleanup drill DB ===" | tee -a "${RESTORE_LOG}"
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d postgres \
  -c "DROP DATABASE ${DRILL_DB};" 2>&1 | tee -a "${RESTORE_LOG}"

echo "=== Restore drill complete ===" | tee -a "${RESTORE_LOG}"
echo "Log: ${RESTORE_LOG}"
```

### 2.2 Production Restore Procedure (Emergency Only)

If a production restore is required:

1. **Stop FerrumGate** to prevent new writes during restore.
2. **Create a pre-restore copy** of the current database (or rename it).
3. **Restore** from the chosen backup using `pg_restore` into the production database name.
4. **Verify** key table row counts and run a smoke test via `/v1/readyz/deep`.
5. **Restart FerrumGate**.

```bash
# Emergency restore outline (operator adapts paths/credentials)
# 1. Stop ferrumd
systemctl stop ferrumd

# 2. Rename current DB (or create a logical backup of it first)
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d postgres \
  -c "ALTER DATABASE ${PGDATABASE} RENAME TO ${PGDATABASE}_pre_restore;"

# 3. Create fresh target DB
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d postgres \
  -c "CREATE DATABASE ${PGDATABASE};"

# 4. Restore
pg_restore -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${PGDATABASE}" \
  --no-owner --no-privileges -v "${BACKUP_FILE}"

# 5. Verify
psql -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${PGDATABASE}" \
  -c "SELECT COUNT(*) FROM intents;"

# 6. Start ferrumd
systemctl start ferrumd
```

### 2.3 Restore Drill Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| `pg_restore` into drill DB exits 0 | true | |
| All expected tables present in drill DB | true | |
| Row counts in drill DB match source (±0 for static backup) | true | |
| Drill DB dropped after verification | true | |
| Restore log written and reviewed | true | |

### 2.4 Stop Conditions

| Trigger | Action |
|---------|--------|
| `pg_restore` exits non-zero | Do not proceed to production restore; investigate errors in log |
| Tables missing after restore | Investigate schema version mismatch; do not overwrite production |
| Row counts significantly different | Backup may be inconsistent; take new backup before proceeding |

---

## P5c.3 — RPO / RTO Targets

### 3.1 Operator-Approved Targets

| Metric | Target | Rationale |
|--------|--------|-----------|
| **RPO** (Recovery Point Objective) | **15 minutes** | Maximum acceptable data loss: 15 minutes of writes. Achieved by scheduling `pg_dump` at 15-minute intervals (or more frequently). Operator-approved. |
| **RTO** (Recovery Time Objective) | **30 minutes** | Maximum acceptable downtime: 30 minutes from failure decision to restored service. Achieved by documented restore procedure + restart + verification. Operator-approved. |

### 3.2 Deriving Scheduler Frequency from RPO

To meet RPO = 15 minutes, the backup interval must be **≤ 15 minutes**.

| Backup Interval | Effective RPO | Meets Target? |
|-----------------|---------------|---------------|
| 15 minutes | ~15 minutes | Yes |
| 10 minutes | ~10 minutes | Yes (conservative) |
| 5 minutes | ~5 minutes | Yes (very conservative) |
| 30 minutes | ~30 minutes | No |

> **Operator decision**: Select an interval ≤ 15 minutes based on workload write rate and storage capacity. Higher write rates may warrant more frequent backups or a shorter interval.

### 3.3 Deriving Restore Time Budget from RTO

RTO = 30 minutes total budget. Allocate time across phases:

| Phase | Time Budget | Owner |
|-------|-------------|-------|
| Detect failure + decision | ≤ 5 minutes | Operator / monitoring |
| Stop FerrumGate + pre-restore copy | ≤ 5 minutes | Operator |
| `pg_restore` execution | ≤ 10 minutes | Operator (depends on backup size) |
| Post-restore verification | ≤ 5 minutes | Operator |
| Restart FerrumGate + probe | ≤ 5 minutes | Operator |
| **Total** | **≤ 30 minutes** | |

> **Note**: Large databases or slow networks may exceed the 10-minute restore budget. The operator should benchmark restore time during the drill and adjust RTO or backup strategy accordingly.

### 3.4 RPO/RTO Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| Backup interval ≤ 15 minutes | true | |
| Restore drill completes within 30 minutes | true | |
| Operator acknowledges targets are design goals, not guarantees | true | |

### 3.5 RPO/RTO Stop Conditions

| Trigger | Action |
|---------|--------|
| Backup interval > 15 minutes | Reduce interval or accept higher RPO with documented risk |
| Restore drill exceeds 30 minutes | Investigate bottleneck (disk, network, DB size); adjust RTO target or optimize restore |
| Operator cannot meet either target | Document deviation and operator acceptance of revised target |

---

## P5c.5 — External Scheduler Examples

FerrumGate does not manage backup scheduling. The operator must configure an external scheduler.

### 5.1 Cron Example (15-Minute Interval)

```bash
# /etc/cron.d/ferrumgate-postgres-backup
# Review paths/user/credentials before installing. Operator-owned.
SHELL=/bin/bash
PATH=/usr/local/bin:/usr/bin:/bin

# Run pg_dump every 15 minutes
# PGPASSFILE must be readable by the backup user; do not inline passwords
*/15 * * * * backupuser PGPASSFILE=/etc/ferrumgate/.pgpass /usr/local/bin/pg_dump -h localhost -p 5432 -U backupuser -d ferrumgate -Fc --no-owner --no-privileges -f /var/backups/ferrumgate-postgres/ferrumgate_$(date +\%Y\%m\%d_\%H\%M\%S).dump && find /var/backups/ferrumgate-postgres -name "ferrumgate_*.dump" -type f -mmin +$((15*4*24)) -delete
```

**Retention**: The `find` command above prunes dumps older than ~4 days (for 15-minute intervals, this retains roughly 384 dumps). Adjust `-mmin` or use a dedicated retention script.

### 5.2 Systemd Timer Example (15-Minute Interval)

**Timer unit** (`ferrumgate-postgres-backup.timer`):

```ini
[Unit]
Description=FerrumGate PostgreSQL backup timer (15-minute interval)

[Timer]
OnCalendar=*:0/15
Persistent=true
RandomizedDelaySec=60

[Install]
WantedBy=timers.target
```

**Service unit** (`ferrumgate-postgres-backup.service`):

```ini
[Unit]
Description=FerrumGate PostgreSQL backup
Documentation=./docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md
After=postgresql.service

[Service]
Type=oneshot
User=backupuser
Group=backupuser
Environment="PGPASSFILE=/etc/ferrumgate/.pgpass"
ExecStart=/usr/local/bin/pg_dump -h localhost -p 5432 -U backupuser -d ferrumgate -Fc --no-owner --no-privileges -f /var/backups/ferrumgate-postgres/ferrumgate_%Y%m%d_%H%M%S.dump
ExecStartPost=/bin/sh -c 'latest=$(ls -1t /var/backups/ferrumgate-postgres/*.dump | head -n1); pg_restore -l "$latest" > /dev/null && echo "Backup OK: $latest" || echo "Backup VERIFY FAILED: $latest"'
PrivateTmp=true
NoNewPrivileges=true
```

### 5.3 Scheduler Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| Timer fires at configured interval (≤ 15 min) | true | |
| Backup file created on each firing | true | |
| Old backups pruned per retention policy | true | |
| No secrets in unit files or cron entries | true (use PGPASSFILE or env file) | |
| Service runs as non-root dedicated user | true | |

### 5.4 Scheduler Stop Conditions

| Trigger | Action |
|---------|--------|
| Backup job fails repeatedly | Investigate credentials, disk space, and PostgreSQL connectivity |
| Backups consume excessive disk | Reduce retention or move to secondary storage |
| Secrets exposed in process list | Switch to `.pgpass` file or connection service file; review permissions |

---

## Verification Gates (P5c)

| Gate | Criterion | Evidence | Status |
|------|-----------|----------|--------|
| P5c.V1 | Backup produces consistent snapshot | `pg_dump` completes with exit 0; `pg_restore -l` succeeds; taken during low-write window | ☐ Pending operator drill |
| P5c.V2 | Restore drill completes successfully | Operator drill log with restored DB row counts verified | ☐ Pending operator drill |
| P5c.V3 | RPO/RTO operator-accepted for PostgreSQL | Signed operator acknowledgment (this runbook or `105-g3-5-operator-d1-d3-signoff-packet.md` refresh) | ☑ Targets approved (RPO=15min, RTO=30min) |

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `109-p5c-postgresql-backup-restore-runbook.md` | `105-g3-5-operator-d1-d3-signoff-packet.md` | G3.5 D1=A/D2=A/D3=A selections |
| `109-p5c-postgresql-backup-restore-runbook.md` | `50-p4-postgres-store-facade-adr.md` §3.5.2 | ADR-50 P5c scope and verification gates |
| `109-p5c-postgresql-backup-restore-runbook.md` | `108-eng-2-p5b-p5e-implementation-planning-packet.md` | Eng.2 P5c planning and estimates |
| `109-p5c-postgresql-backup-restore-runbook.md` | `31-release-paths-todo.md` §Path 3 | P5c checklist in release paths |
| `109-p5c-postgresql-backup-restore-runbook.md` | `110-p5c-postgresql-drill-evidence-template.md` | **Fillable operator evidence template for P5c.V1/V2** |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | This doc | P5c design completion evidence |
| `50-p4-postgres-store-facade-adr.md` | This doc | P5c runbook cross-reference |

---

## Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-12 | Initial P5c runbook: P5c.1–P5c.5, RPO=15min/RTO=30min, scheduler examples, non-claims | Engineering |

---

*Document created: 2026-05-12. P5c PostgreSQL Backup/Restore Operator Runbook — design/docs only. No production-ready claim. No HA/multi-node. No PostgreSQL production deployment authorization. RPO=15min/RTO=30min operator-approved targets.*

(End of file)
