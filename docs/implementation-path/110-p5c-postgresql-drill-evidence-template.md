# 110 — P5c PostgreSQL Drill Evidence Template

> **Status**: Fillable template. Operator completes one copy per drill. No secret values.  
> **Purpose**: Evidence record for P5c.V1 (backup) and P5c.V2 (restore) operator drills.  
> **Scope**: Single-node PostgreSQL only. No HA/multi-node.  
> **Constraint**: Do NOT record passwords, tokens, or full DSNs in this template. Use sanitized descriptions only. Production PostgreSQL deployment remains NO until P5b–P5e and P6 assessment are complete.

---

## Drill Metadata

| Field | Value |
|-------|-------|
| Date | `<YYYY-MM-DD>` |
| Operator | `<operator name>` |
| Target Environment | `<e.g., non-prod-docker, staging-vm>` |
| Drill Version | `☐ P5c.V1 (backup)` / `☐ P5c.V2 (restore)` / `☐ Both` |

---

## Target Database (Sanitized)

| Field | Description (do NOT include credentials) |
|-------|------------------------------------------|
| Host | `<e.g., localhost, pg-staging.internal>` |
| Port | `<e.g., 5432>` |
| Database Name | `<e.g., ferrumgate_staging>` |
| DSN Description | `<e.g., postgres://<user>@<host>/<db> — user has SELECT on all tables>` |

> **Safety rule**: If you paste a command that contained a password, replace the password with `<REDACTED>` before saving this template.

---

## P5c.V1 — Backup Drill Evidence

| Step | Command / Check | Exit Code | Evidence |
|------|-----------------|-----------|----------|
| V1.1 | `pg_dump` executed with `-Fc -v --no-owner --no-privileges` | `<0 or non-zero>` | |
| V1.2 | Backup artifact exists and size > 0 | `<size, e.g., 2.4M>` | |
| V1.3 | Backup artifact path | `<absolute path>` | |
| V1.4 | Backup artifact checksum (SHA-256) | `<checksum>` | |
| V1.5 | `pg_restore -l "${BACKUP_FILE}"` result | `☐ OK` / `☐ FAIL` | |
| V1.6 | Backup taken during low-write window | `☐ YES` / `☐ NO` / `☐ N/A` | |
| V1.7 | No secrets logged in command history | `☐ VERIFIED` | |

**V1 Stop Conditions**: If any of V1.1, V1.2, or V1.5 report FAIL, do NOT schedule this backup for production use. Investigate connection, permissions, or disk space, then retry.

---

## P5c.V2 — Restore Drill Evidence

| Step | Command / Check | Exit Code | Evidence |
|------|-----------------|-----------|----------|
| V2.1 | `pg_restore -l` dry inspection performed | `☐ OK` | |
| V2.2 | Drill target database name | `<e.g., ferrumgate_drill_20260512_143022>` | |
| V2.3 | `pg_restore` into drill DB exit code | `<0 or non-zero>` | |
| V2.4 | All expected tables present in drill DB | `☐ YES` / `☐ NO` | |
| V2.5 | Row count verification (key tables) | `<e.g., intents: 42, proposals: 7>` | |
| V2.6 | Optional: hash verification (`md5` or `sha256` of key table dumps) | `<hash>` | |
| V2.7 | Drill database dropped after verification | `☐ YES` / `☐ NO` | |
| V2.8 | Restore log path | `<absolute path>` | |

**V2 Stop Conditions**: If V2.3 is non-zero, or V2.4 is NO, or row counts differ significantly from source, do NOT proceed to production restore. Investigate schema version mismatch or backup corruption.

---

## Failures / Deviations

| ID | Failure / Deviation | Resolution / Waiver |
|----|---------------------|---------------------|
| F1 | | |
| F2 | | |
| F3 | | |

---

## Cleanup Confirmation

| Step | Check | Status |
|------|-------|--------|
| C1 | Drill database dropped (if created) | `☐ YES` / `☐ N/A` |
| C2 | Temporary logs or artifacts removed from shared directories | `☐ YES` / `☐ N/A` |
| C3 | No secrets left in `/tmp` or shell history | `☐ VERIFIED` |

---

## Operator Signoff

> **P6 CONDITIONAL GO**: This evidence template supports a conditional go/no-go assessment. P5c.V1 and P5c.V2 are NOT production-ready declarations. Production PostgreSQL deployment remains gated on P5b–P5e completion and a future P6 assessment. HA/multi-node remains NO.

| Criterion | Pass / Fail / N/A | Initials |
|-----------|-------------------|----------|
| P5c.V1 backup drill evidence complete | | |
| P5c.V2 restore drill evidence complete | | |
| No secrets recorded in this template | | |
| Operator acknowledges conditional posture | | |

**Operator Name**: ____________________  
**Date**: ____________________  
**Signature**: ____________________

---

## Cross-References

| This Template | Links To | Purpose |
|---------------|----------|---------|
| `110-p5c-postgresql-drill-evidence-template.md` | `109-p5c-postgresql-backup-restore-runbook.md` | P5c backup/restore commands and acceptance criteria |
| `110-p5c-postgresql-drill-evidence-template.md` | `105-g3-5-operator-d1-d3-signoff-packet.md` | G3.5 D1=A/D2=A/D3=A selections and waivers |
| `110-p5c-postgresql-drill-evidence-template.md` | `66-path-2-operator-handoff.md` | Consolidated operator blocker checklist |

---

*Template version: 2026-05-12. P5c PostgreSQL Drill Evidence Template — fillable. No production-ready claim. No HA/multi-node. No secret values.*
