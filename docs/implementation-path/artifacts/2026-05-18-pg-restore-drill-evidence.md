# PostgreSQL Backup/Restore Drill Evidence — PG-3 — 2026-05-18

## Status

- **Scope**: PG-3 local Docker PostgreSQL backup/restore drill only.
- **Verdict**: ✅ PASS for local backup/restore drill.
- **Production-ready**: NO.
- **Full G2**: NOT COMPLETE.
- **Block A**: WAIVED/CONDITIONAL, not closed.
- **HA/PostgreSQL production**: NOT CLAIMED.
- **Scheduled backup/retention**: NOT IMPLEMENTED / DEFERRED.

This artifact records a local Docker PostgreSQL backup and restore drill. No production deployment, no target-host validation, no scheduled backup automation, and no retention pruning were performed.

---

## Drill Metadata

| Field | Value |
|-------|-------|
| Date | 2026-05-18 |
| Operator | Engineering (automated drill) |
| Target Environment | local Docker PostgreSQL (`postgres_p2` container) |
| Drill Version | Both (V1 backup + V2 restore) |

---

## Target Database (Sanitized)

| Field | Description (do NOT include credentials) |
|-------|------------------------------------------|
| Host | `localhost` |
| Port | `5432` |
| Database Name | `ferrumgate_p2_test` |
| DSN Description | `postgres://ferrumgate_dev:<REDACTED>@localhost:5432/ferrumgate_p2_test` — local dev credentials from compose; no real secret. |

> **Safety rule**: Password replaced with `<REDACTED>`. No credential-bearing values are recorded in this artifact.

---

## P5c.V1 — Backup Drill Evidence

| Step | Command / Check | Exit Code | Evidence |
|------|-----------------|-----------|----------|
| V1.1 | `pg_dump` executed with `-Fc --no-owner --no-privileges` inside Docker container | `0` | ✅ PASS |
| V1.2 | Backup artifact exists and size > 0 | `19852` bytes | ✅ PASS |
| V1.3 | Backup artifact path | `/tmp/ferrumgate_pg3_restore_drill.dump` | ✅ PASS |
| V1.4 | Backup artifact checksum (SHA-256) | `e75c933a64b85b083dd38f4d75e070e50d020675f6156eb7539d69780d9a39fc` | ✅ PASS |
| V1.5 | `pg_restore -l "${BACKUP_FILE}"` result | ✅ OK — listable; TOC entries 57; 11 public tables | ✅ PASS |
| V1.6 | Backup taken during low-write window | N/A — local Docker schema baseline; no live writes | ✅ N/A |
| V1.7 | No secrets logged in command history | ✅ VERIFIED — password was `<REDACTED>` in all commands | ✅ PASS |

**V1 Stop Conditions**: All V1 checks passed. Backup artifact is listable and checksum-verified.

---

## P5c.V2 — Restore Drill Evidence

| Step | Command / Check | Exit Code | Evidence |
|------|-----------------|-----------|----------|
| V2.1 | `pg_restore -l` dry inspection performed | ✅ OK | ✅ PASS |
| V2.2 | Drill target database name | `ferrumgate_pg3_drill` | ✅ PASS |
| V2.3 | `pg_restore` into drill DB exit code | `0` | ✅ PASS |
| V2.4 | All expected tables present in drill DB | ✅ YES — 11 public tables present | ✅ PASS |
| V2.5 | Row count verification (key tables) | All counts matched (all 0 — local Docker schema baseline) | ✅ PASS |
| V2.6 | Optional: hash verification | N/A — row counts and schema presence are sufficient for empty baseline | ☐ N/A |
| V2.7 | Drill database dropped after verification | ✅ YES | ✅ PASS |
| V2.8 | Restore log path | N/A — logged to stdout/stderr only | ☐ N/A |

**V2 Stop Conditions**: V2.3 exit 0, V2.4 YES, row counts matched. Proceed to readiness verification.

### Restored table inventory

The following 11 public tables were present after restore:

1. `_migration_checkpoints`
2. `approvals`
3. `capabilities`
4. `executions`
5. `intents`
6. `ledger_entries`
7. `policy_bundles`
8. `proposals`
9. `provenance_edges`
10. `provenance_events`
11. `rollback_contracts`

### Row count verification

All 10 key tables had matching row counts between source and restored drill database. Because the source was a local Docker schema baseline with no live workload, all counts were 0. This still validates schema completeness, table enumeration, and restore fidelity.

| Table | Source count | Restored count | Match |
|-------|-------------:|---------------:|-------|
| `intents` | 0 | 0 | ✅ |
| `proposals` | 0 | 0 | ✅ |
| `capabilities` | 0 | 0 | ✅ |
| `executions` | 0 | 0 | ✅ |
| `rollback_contracts` | 0 | 0 | ✅ |
| `approvals` | 0 | 0 | ✅ |
| `provenance_events` | 0 | 0 | ✅ |
| `provenance_edges` | 0 | 0 | ✅ |
| `ledger_entries` | 0 | 0 | ✅ |
| `policy_bundles` | 0 | 0 | ✅ |

### Readiness verification against restored DB

After pointing `ferrumd` at the restored drill database, `/v1/readyz/deep` returned:

- HTTP status: `200`
- Body: `status ok`, `healthy true`

Result: ✅ PASS.

---

## Scheduled Backup / Retention — NOT IMPLEMENTED

| Item | Status | Note |
|------|--------|------|
| Scheduled `pg_dump` or WAL backup | ☐ NOT STARTED | No cron, no systemd timer, no orchestrated backup job. |
| Retention pruning | ☐ NOT STARTED | No retention policy or automated pruning implemented. |
| Offsite backup replication | ☐ NOT STARTED | Local artifact only; no offsite copy for this drill. |

These items are explicitly deferred and are **not** claimed as complete by this artifact.

---

## Failures / Deviations

| ID | Failure / Deviation | Resolution / Waiver |
|----|---------------------|---------------------|
| F1 | Source database was empty (all counts 0) | Expected for local Docker schema baseline; does not invalidate restore fidelity. |
| F2 | No scheduled backup or retention pruning | Deferred — not in scope for this drill. |
| F3 | No target-host or production PostgreSQL validation | Expected — local Docker drill only. |

---

## Cleanup Confirmation

| Step | Check | Status |
|------|-------|--------|
| C1 | Drill database `ferrumgate_pg3_drill` dropped | ✅ YES |
| C2 | Temporary dump artifact removed from shared directories | ✅ YES — `/tmp/ferrumgate_pg3_restore_drill.dump` removed after verification |
| C3 | No secrets left in `/tmp` or shell history | ✅ VERIFIED |

---

## Operator Signoff

> **P6 CONDITIONAL GO**: This evidence supports a conditional go/no-go assessment. It is NOT a production-ready declaration. Production PostgreSQL deployment remains gated on P5b–P5e completion and a future P6 assessment. HA/multi-node remains NO. Scheduled backup and retention pruning remain NOT STARTED.

| Criterion | Pass / Fail / N/A | Initials |
|-----------|-------------------|----------|
| P5c.V1 backup drill evidence complete | ✅ PASS | ENG |
| P5c.V2 restore drill evidence complete | ✅ PASS | ENG |
| No secrets recorded in this template | ✅ VERIFIED | ENG |
| Operator acknowledges conditional posture | ✅ ACK | ENG |

**Operator Name**: Engineering (automated drill)
**Date**: 2026-05-18
**Signature**: N/A — engineering-run drill

---

## Cross-References

| This Artifact | Links To | Purpose |
|---------------|----------|---------|
| `2026-05-18-pg-restore-drill-evidence.md` | `docs/implementation-path/110-p5c-postgresql-drill-evidence-template.md` | Template used |
| `2026-05-18-pg-restore-drill-evidence.md` | `docs/production-readiness-v2/02-postgres-production-plan.md` §PG-3 | Phase tracking |
| `2026-05-18-pg-restore-drill-evidence.md` | `docs/production-readiness-v2/10-evidence-checklist.md` §1.13 | Checklist tracking |
| `2026-05-18-pg-restore-drill-evidence.md` | `docs/PRODUCTION_NOTES.md` | Production notes readiness/metrics update |
| `2026-05-18-pg-restore-drill-evidence.md` | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` | Companion PG-1 evidence |

---

## Non-claims

- `production-ready = NO`.
- `full G2 = NOT COMPLETE`.
- `Block A = WAIVED/CONDITIONAL`.
- `PostgreSQL production deployment = NO`.
- `HA/multi-node = NO`.
- `scheduled backup/retention automation = NOT IMPLEMENTED`.

---

*Artifact version: 2026-05-18. PG-3 Local Docker PostgreSQL Backup/Restore Drill Evidence. No production-ready claim. No HA/multi-node. No secret values.*
