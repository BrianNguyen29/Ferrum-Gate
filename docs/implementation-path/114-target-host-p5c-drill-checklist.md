# 114 — Target-Host P5c Drill Checklist

> **Status**: Operator checklist. Not executed. No production-ready claim.  
> **Purpose**: Step-by-step target-host P5c.V1 (backup) and P5c.V2 (restore) drill checklist for operators.  
> **Scope**: Single-node PostgreSQL target host only. No HA/multi-node.  
> **Constraint**: This checklist does NOT authorize production PostgreSQL deployment. Production deployment remains gated on P5b–P5e completion and P6 assessment. Do not record secrets.

---

## 1. Purpose

This checklist guides the operator through executing P5c.V1 and P5c.V2 on a **real target host** (non-production or staging) after selecting **Option B — PostgreSQL** in `113-operator-path-selection-packet.md`.

It references:
- `109-p5c-postgresql-backup-restore-runbook.md` — commands and acceptance criteria
- `110-p5c-postgresql-drill-evidence-template.md` — fillable evidence template

**Operator-owned**: All execution, credential management, and evidence recording are operator responsibilities.

---

## 2. Explicit Non-Claims

- **No production-ready claim**: Completing this checklist does NOT make FerrumGate production-ready.
- **No PostgreSQL production deployment**: Production deployment remains gated on P5b–P5e and P6.
- **No HA/multi-node**: Single-node PostgreSQL only.
- **No secret recording**: Do not record passwords, tokens, or full DSNs in this checklist or in evidence template `110`.
- **No fabricated evidence**: Check boxes only after executing the step on the target host.

---

## 3. Prerequisites

Before starting, confirm all prerequisites. If any fail, **stop** and resolve before proceeding.

| # | Prerequisite | Verification | Status |
|---|---|---|---|
| P1 | Operator has selected Option B (PostgreSQL) in doc 113 | `113-operator-path-selection-packet.md` signed for Option B | ☐ |
| P2 | Target PostgreSQL instance running and reachable | `pg_isready -h <target-host> -p <target-port>` returns `accepting connections` | ☐ |
| P3 | `pg_dump` and `pg_restore` available on operator workstation or target | `pg_dump --version` and `pg_restore --version` succeed | ☐ |
| P4 | Backup destination directory exists and is writable | `ssh <target-host> "test -w <backup-dir>"` | ☐ |
| P5 | FerrumGate schema initialized in target database | `\dt` in target DB shows 11 FerrumGate tables | ☐ |
| P6 | `.pgpass` or `PGPASSFILE` configured (no inline passwords) | `chmod 600 ~/.pgpass` verified; no passwords in shell history | ☐ |
| P7 | Drill evidence template `110` ready | Blank copy available for filling | ☐ |
| P8 | Low-write window scheduled | Backup taken during period of minimal write activity | ☐ |

> **Safety rule**: If you paste a command that contained a password, replace the password with `<REDACTED>` before saving evidence.

---

## 4. Configuration Template

Fill these values **once** before executing drills. Use placeholders; do not record actual credentials in this document.

```text
TARGET_HOST="<postgres-host>"
TARGET_PORT="<postgres-port>"
TARGET_DB="<ferrumgate-db>"
TARGET_USER="<backup-user>"
BACKUP_DIR="<backup-dir>"
DRILL_LOG_DIR="<log-dir>"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.dump"
DRILL_DB="ferrumgate_drill_${TIMESTAMP}"
RESTORE_LOG="${DRILL_LOG_DIR}/pg_restore_drill_${TIMESTAMP}.log"
```

> **Redaction rule**: Before recording any command in evidence template `110`, replace passwords with `<REDACTED>`.

---

## 5. P5c.V1 — Target-Host Backup Drill

### 5.1 Pre-Backup Checks

| # | Step | Command / Check | Status |
|---|---|---|---|
| V1-P1 | Verify target connectivity | `pg_isready -h "${TARGET_HOST}" -p "${TARGET_PORT}"` | ☐ |
| V1-P2 | Verify schema tables present | `psql -h "${TARGET_HOST}" -p "${TARGET_PORT}" -U "${TARGET_USER}" -d "${TARGET_DB}" -c "\dt"` | ☐ |
| V1-P3 | Record pre-backup row counts (key tables) | `SELECT COUNT(*) FROM intents;` etc. | ☐ |
| V1-P4 | Ensure backup directory exists | `mkdir -p "${BACKUP_DIR}"` | ☐ |
| V1-P5 | Confirm low-write window | Operator confirms minimal writes expected during backup | ☐ |

### 5.2 Execute Backup

| # | Step | Command Template | Status |
|---|---|---|---|
| V1-E1 | Run `pg_dump` with custom format | `pg_dump -h "${TARGET_HOST}" -p "${TARGET_PORT}" -U "${TARGET_USER}" -d "${TARGET_DB}" -Fc -v --no-owner --no-privileges -f "${BACKUP_FILE}"` | ☐ |
| V1-E2 | Record exit code | `echo $?` | ☐ |
| V1-E3 | Verify backup file exists and size > 0 | `ls -lh "${BACKUP_FILE}"` | ☐ |
| V1-E4 | Compute SHA-256 checksum | `sha256sum "${BACKUP_FILE}"` | ☐ |
| V1-E5 | Verify backup listable | `pg_restore -l "${BACKUP_FILE}" > /dev/null && echo "OK" || echo "FAIL"` | ☐ |
| V1-E6 | Count objects in backup | `pg_restore -l "${BACKUP_FILE}" \| wc -l` | ☐ |

### 5.3 V1 Stop Conditions

| Trigger | Action |
|---|---|
| `pg_dump` exits non-zero | Do NOT schedule this backup. Investigate connection, permissions, disk space. |
| Backup file size is 0 or missing | Retry; check disk space and write permissions. |
| `pg_restore -l` fails | Backup may be corrupt. Retry dump before scheduling. |
| Password visible in shell history | Clear history; reconfigure `.pgpass`; do not record evidence until clean. |

### 5.4 V1 Evidence to Record in Template 110

| Template Field | Value to Record |
|---|---|
| V1.1 | `pg_dump` exit code |
| V1.2 | Backup file size |
| V1.3 | Absolute backup path |
| V1.4 | SHA-256 checksum |
| V1.5 | `pg_restore -l` result (OK/FAIL) |
| V1.6 | Low-write window (YES/NO/N/A) |
| V1.7 | No secrets in history (VERIFIED) |

---

## 6. P5c.V2 — Target-Host Restore Drill

### 6.1 Pre-Restore Checks

| # | Step | Command / Check | Status |
|---|---|---|---|
| V2-P1 | Verify backup artifact from V1 is available | `test -f "${BACKUP_FILE}"` | ☐ |
| V2-P2 | Dry-inspect backup contents | `pg_restore -l "${BACKUP_FILE}"` | ☐ |
| V2-P3 | Ensure drill log directory exists | `mkdir -p "${DRILL_LOG_DIR}"` | ☐ |

### 6.2 Execute Restore Drill

| # | Step | Command Template | Status |
|---|---|---|---|
| V2-E1 | Create drill target database | `psql -h "${TARGET_HOST}" -p "${TARGET_PORT}" -U "${TARGET_USER}" -d postgres -c "CREATE DATABASE ${DRILL_DB};"` | ☐ |
| V2-E2 | Restore into drill DB | `pg_restore -h "${TARGET_HOST}" -p "${TARGET_PORT}" -U "${TARGET_USER}" -d "${DRILL_DB}" --no-owner --no-privileges -v "${BACKUP_FILE}" 2>&1 \| tee -a "${RESTORE_LOG}"` | ☐ |
| V2-E3 | Record restore exit code | `echo $?` | ☐ |
| V2-E4 | List tables in drill DB | `psql -h "${TARGET_HOST}" -p "${TARGET_PORT}" -U "${TARGET_USER}" -d "${DRILL_DB}" -c "\dt"` | ☐ |
| V2-E5 | Verify row counts match source | `SELECT COUNT(*) FROM intents;` etc. (compare to V1-P3) | ☐ |
| V2-E6 | Drop drill database | `psql -h "${TARGET_HOST}" -p "${TARGET_PORT}" -U "${TARGET_USER}" -d postgres -c "DROP DATABASE ${DRILL_DB};"` | ☐ |
| V2-E7 | Confirm cleanup | Drill DB no longer appears in `\l` | ☐ |

### 6.3 V2 Stop Conditions

| Trigger | Action |
|---|---|
| `pg_restore` exits non-zero | Do NOT proceed to production restore. Investigate errors in `${RESTORE_LOG}`. |
| Tables missing after restore | Investigate schema version mismatch. Do not overwrite production. |
| Row counts differ from source | Backup may be inconsistent. Take new backup before proceeding. |
| Drill DB not dropped | Complete cleanup before claiming V2 done. |

### 6.4 V2 Evidence to Record in Template 110

| Template Field | Value to Record |
|---|---|
| V2.1 | `pg_restore -l` dry inspection (OK) |
| V2.2 | Drill database name |
| V2.3 | `pg_restore` exit code |
| V2.4 | All expected tables present (YES/NO) |
| V2.5 | Row counts (key tables) |
| V2.6 | Optional hash verification |
| V2.7 | Drill DB dropped (YES/NO) |
| V2.8 | Restore log absolute path |

---

## 7. Cleanup

After both drills are complete (success or failure):

| # | Step | Status |
|---|---|---|
| C1 | Drill database dropped (if created) | ☐ |
| C2 | Temporary logs reviewed for secrets; redacted if needed | ☐ |
| C3 | Backup artifact retained or moved to secure backup storage per retention policy | ☐ |
| C4 | Shell history reviewed; no passwords exposed | ☐ |
| C5 | Local `.pgpass` removed if created only for this drill | ☐ |

---

## 8. Acceptance Criteria

| Criterion | Pass / Fail | Evidence |
|---|---|---|
| V1: `pg_dump` exit code 0 | | Template 110 V1.1 |
| V1: Backup file size > 0 | | Template 110 V1.2 |
| V1: `pg_restore -l` succeeds | | Template 110 V1.5 |
| V1: No secrets in command history | | Template 110 V1.7 |
| V2: `pg_restore` exit code 0 | | Template 110 V2.3 |
| V2: All expected tables present | | Template 110 V2.4 |
| V2: Row counts match source | | Template 110 V2.5 |
| V2: Drill DB dropped | | Template 110 V2.7 |
| Cleanup completed | | This checklist §7 |

---

## 9. Operator Signoff

> **P6 CONDITIONAL GO**: This checklist supports a conditional go/no-go assessment. It is NOT a production-ready declaration. Production PostgreSQL deployment remains gated on P5b–P5e and P6.

| Criterion | Pass / Fail / N/A | Initials |
|---|---|---|
| P5c.V1 target-host backup drill complete | | |
| P5c.V2 target-host restore drill complete | | |
| No secrets recorded in evidence or this checklist | | |
| Operator acknowledges conditional posture | | |

**Operator Name**: ____________________  
**Date**: ____________________  
**Signature**: ____________________

---

## 10. Cross-References

| This Checklist | Links To | Purpose |
|---|---|---|
| `114-target-host-p5c-drill-checklist.md` | `109-p5c-postgresql-backup-restore-runbook.md` | Commands and acceptance criteria |
| `114-target-host-p5c-drill-checklist.md` | `110-p5c-postgresql-drill-evidence-template.md` | Fillable evidence template |
| `114-target-host-p5c-drill-checklist.md` | `113-operator-path-selection-packet.md` | Option B prerequisite |
| `114-target-host-p5c-drill-checklist.md` | `66-path-2-operator-handoff.md` §B.0 | Blockers B6/B7 closure |
| `114-target-host-p5c-drill-checklist.md` | `112-post-p5c-completion-execution-plan.md` §Track 1 | Planning context |

---

*Document created: 2026-05-12. Target-Host P5c Drill Checklist — operator-executable. No production-ready claim. No secret values. P6 CONDITIONAL GO.*
