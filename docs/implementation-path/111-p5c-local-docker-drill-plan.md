# 111 — P5c Local Docker PostgreSQL Drill Plan

> **Status**: Draft plan — pending operator approval. **NOT executed**.  
> **Purpose**: Concrete, local-only Docker PostgreSQL drill plan for P5c.V1 (backup) and P5c.V2 (restore).  
> **Scope**: Single-node PostgreSQL in local Docker (`postgres_p2`). No production deployment. No HA/multi-node.  
> **Constraint**: This plan does NOT authorize production PostgreSQL deployment. Production remains gated on P5b–P5e completion and a future P6 assessment. Do not use these credentials outside local Docker.

---

## 1. Plan Metadata

| Field | Value |
|-------|-------|
| Service name | `postgres_p2` |
| Container name | `ferrumgate_postgres_p2` |
| Local database | `ferrumgate_p2_test` |
| Local user | `ferrumgate_dev` |
| Local password | `ferrumgate_dev_password` *(local dev placeholder only)* |
| Host port | `5432` |
| Expected DSN | `postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test` |
| Docker Compose file | `docker-compose.postgres.yml` |

---

## 2. Preconditions

Before starting the drill, confirm all items below. If any precondition fails, **stop** and resolve it before proceeding.

| # | Precondition | Verification Command | Status |
|---|--------------|----------------------|--------|
| P1 | Docker Engine running | `docker info` returns server info | ☐ |
| P2 | `docker-compose.postgres.yml` present in repo root | `test -f docker-compose.postgres.yml` | ☐ |
| P3 | `pg_dump` and `pg_restore` client tools installed | `pg_dump --version && pg_restore --version` | ☐ |
| P4 | Backup destination directory exists and is writable | `mkdir -p /tmp/ferrumgate_drill_backups && test -w /tmp/ferrumgate_drill_backups` | ☐ |
| P5 | D2=A signed (operator selected `pg_dump` logical backup) | `105-g3-5-operator-d1-d3-signoff-packet.md` | ☑ DONE |

---

## 3. Step-by-Step Drill Procedure

### 3.1 Start Local PostgreSQL Container

```bash
# Start the postgres_p2 service in detached mode
docker compose -f docker-compose.postgres.yml up -d postgres_p2

# Wait for healthy status (poll until healthy)
docker compose -f docker-compose.postgres.yml ps postgres_p2

# Quick connectivity check
pg_isready -h localhost -p 5432 -U ferrumgate_dev -d ferrumgate_p2_test
```

**Stop condition**: If `pg_isready` does not return `accepting connections` within 60 seconds, inspect container logs (`docker logs ferrumgate_postgres_p2`) and resolve before continuing.

---

### 3.2 (Optional) Seed Schema and Smoke Data

If you want realistic row counts for V2 evidence, seed the database from a populated SQLite source or apply migrations.

**Option A — Seed from SQLite (requires populated SQLite DB)**:

```bash
# Example: migrate from an in-memory or file-based SQLite source to the local PostgreSQL target
# Replace <sqlite_source> with an actual source DSN if available
cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite::memory:" \
  --to "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test" \
  --apply \
  --chunk-size 100
```

> **Note**: If you do not have a populated SQLite source, skip this step. The drill can still proceed with an empty schema; V2 row-count evidence will read `0`.

**Option B — Verify schema is present**:

```bash
psql "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test" -c "\dt"
```

---

### 3.3 Configure Local `.pgpass` (Preferred)

Instead of passing the password inline or via environment variables, create a local `.pgpass` file. This minimizes exposure in shell history and process listings.

```bash
# Create local .pgpass
mkdir -p ~/.config/ferrumgate
cat > ~/.config/ferrumgate/.pgpass_drill <<'EOF'
localhost:5432:ferrumgate_p2_test:ferrumgate_dev:ferrumgate_dev_password
EOF
chmod 600 ~/.config/ferrumgate/.pgpass_drill
```

Set the environment variable so `pg_dump` / `pg_restore` / `psql` use it:

```bash
export PGPASSFILE="${HOME}/.config/ferrumgate/.pgpass_drill"
```

> **Redaction rule**: Before recording any command in evidence template `110`, replace the password with `<REDACTED>` and remove or sanitize `.pgpass` contents from logs.

---

### 3.4 P5c.V1 — Backup Drill

#### V1.1 Run `pg_dump`

```bash
# Configuration
PGHOST="localhost"
PGPORT="5432"
PGDATABASE="ferrumgate_p2_test"
PGUSER="ferrumgate_dev"
BACKUP_DIR="/tmp/ferrumgate_drill_backups"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/ferrumgate_drill_${TIMESTAMP}.dump"

mkdir -p "${BACKUP_DIR}"

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
- V1.6 whether backup was taken during low-write window (local Docker = likely yes)
- V1.7 verification that no secrets were logged in command history

#### V1.2 Post-Backup Verification

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

---

### 3.5 P5c.V2 — Restore Drill

#### V2.1 Prepare Drill Target Database

```bash
# Configuration
PGHOST="localhost"
PGPORT="5432"
PGUSER="ferrumgate_dev"
BACKUP_FILE="<absolute-path-to-backup-file-from-V1>"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESTORE_LOG="/tmp/pg_restore_drill_${TIMESTAMP}.log"
DRILL_DB="ferrumgate_drill_${TIMESTAMP}"

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

---

## 4. Cleanup and Reset

After the drill is complete (success or failure), perform the following cleanup:

| Step | Command | Purpose |
|------|---------|---------|
| C1 | `docker compose -f docker-compose.postgres.yml down -v` | Stop container and remove named volume (destroys local DB) |
| C2 | `rm -f ~/.config/ferrumgate/.pgpass_drill` | Remove local `.pgpass` file |
| C3 | `rm -f /tmp/ferrumgate_drill_backups/*.dump` | Remove drill backup artifacts |
| C4 | `rm -f /tmp/pg_restore_drill_*.log` | Remove drill restore logs |
| C5 | `history -c` or review shell history | Ensure no secrets remain in shell history |

> **Warning**: `docker compose ... down -v` is destructive for local data. Only run it after evidence has been recorded.

---

## 5. Approval Checklist

This plan must be reviewed and approved by an operator before execution. Check each item:

| # | Criterion | Status |
|---|-----------|--------|
| A1 | Operator has read `109-p5c-postgresql-backup-restore-runbook.md` | ☐ |
| A2 | Operator has the fillable `110-p5c-postgresql-drill-evidence-template.md` ready | ☐ |
| A3 | Operator confirms this is a **local-only** drill; no production systems involved | ☐ |
| A4 | Operator confirms `.pgpass` will be used instead of inline passwords | ☐ |
| A5 | Operator confirms all commands will be redacted (`<REDACTED>`) before recording evidence | ☐ |
| A6 | Operator confirms cleanup steps (C1–C5) will be executed after drill | ☐ |
| A7 | Operator acknowledges production PostgreSQL deployment remains **NO** until P5b–P5e and P6 are complete | ☐ |

**Approver Name**: ____________________  
**Date**: ____________________  
**Signature**: ____________________

---

## 6. Evidence Mapping

Use this mapping to transfer drill results into the evidence template.

| This Plan Step | Evidence Template Field (Doc 110) | What to Record |
|----------------|-----------------------------------|----------------|
| 3.4 V1.1 | V1.1 | `pg_dump` exit code |
| 3.4 V1.2 | V1.2 | Backup file size |
| 3.4 V1.3 | V1.3 | Backup absolute path |
| 3.4 V1.4 | V1.4 | SHA-256 checksum |
| 3.4 V1.5 | V1.5 | `pg_restore -l` OK/FAIL |
| 3.4 V1.6 | V1.6 | Low-write window YES/NO/N/A |
| 3.4 V1.7 | V1.7 | No secrets in history VERIFIED |
| 3.5 V2.1 | V2.1 | `pg_restore -l` dry inspection OK |
| 3.5 V2.2 | V2.2 | Drill DB name |
| 3.5 V2.2 | V2.3 | `pg_restore` exit code |
| 3.5 V2.3 | V2.4 | Expected tables present YES/NO |
| 3.5 V2.3 | V2.5 | Row counts (key tables) |
| 3.5 V2.3 | V2.6 | Optional hash |
| 3.5 V2.4 | V2.7 | Drill DB dropped YES/NO |
| 3.5 V2.4 | V2.8 | Restore log path |

---

## 7. Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `111-p5c-local-docker-drill-plan.md` | `110-p5c-postgresql-drill-evidence-template.md` | Fillable evidence template for V1/V2 results |
| `111-p5c-local-docker-drill-plan.md` | `109-p5c-postgresql-backup-restore-runbook.md` | Backup/restore commands and acceptance criteria |
| `111-p5c-local-docker-drill-plan.md` | `docker-compose.postgres.yml` | Local Docker PostgreSQL service definition |
| `111-p5c-local-docker-drill-plan.md` | `105-g3-5-operator-d1-d3-signoff-packet.md` | G3.5 D1=A/D2=A/D3=A selections |

---

## 8. Safety and Non-Claims

- **Not executed**: This document is a plan only. It does not claim any drill has been run.
- **Local-only**: All commands target `localhost:5432` and the `postgres_p2` Docker service. Do not run against production databases.
- **No production-ready claim**: Production PostgreSQL deployment remains gated on P5b–P5e completion and a future P6 assessment.
- **No HA/multi-node**: Single-node only. No read replicas, no streaming replication, no failover.
- **Secret safety**: Passwords in this plan are local dev placeholders. Do not record them in evidence templates without redaction.
- **Cleanup required**: Drill artifacts (backups, logs, `.pgpass`) must be removed after use.

---

*Document created: 2026-05-12. P5c Local Docker PostgreSQL Drill Plan — pending operator approval. NOT executed. No production-ready claim. No HA/multi-node.*

(End of file)
