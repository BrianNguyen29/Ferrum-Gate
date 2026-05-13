# 121 — P5c Target-Host Go / No-Go Checklist

> **Status**: Operator self-assessment checklist. Not executed. No production-ready claim.  
> **Purpose**: One-page go/no-go checklist for the operator to self-assess readiness before executing target-host P5c.V1 (backup) or P5c.V2 (restore) drills.  
> **Scope**: Single-node PostgreSQL target host. Non-production or staging only.  
> **Constraint**: This checklist does NOT authorize production PostgreSQL deployment. Production remains gated on P5b–P5e and P6. Do not record secrets.

---

## 1. How to Use This Checklist

1. Read the entire checklist **before** scheduling any target-host drill.
2. Answer every item honestly. If any item is **NO** or **UNKNOWN**, the verdict is **NO-GO**.
3. Resolve all NO/UNKNOWN items before re-evaluating.
4. Record the completed checklist in evidence template `110-p5c-postgresql-drill-evidence-template.md` (reference only; do not record secrets here).

---

## 2. Go / No-Go Criteria

| # | Criterion | Question | Response | Verdict |
|---|-----------|----------|----------|---------|
| G1 | **Path Selection** | Has the operator formally selected Option B (PostgreSQL) in `113-operator-path-selection-packet.md`? | ☐ YES ☐ NO / N/A | |
| G2 | **Environment Safety** | Is the target host explicitly **non-production** or staging? | ☐ YES ☐ NO | |
| G3 | **Connectivity** | Does `pg_isready -h <TARGET_HOST> -p <TARGET_PORT>` return `accepting connections`? | ☐ YES ☐ NO | |
| G4 | **Tool Compatibility** | Is the `pg_dump` / `pg_restore` client major version **≥** the target PostgreSQL server major version? (See `117-postgresql-readiness-acceleration-plan.md` §A.3 compatibility matrix.) | ☐ YES ☐ NO / UNKNOWN | |
| G5 | **Credential Safety** | Is `.pgpass` or `PGPASSFILE` configured with mode `600`, and are there **no inline passwords** in shell history, scripts, or cron entries? | ☐ YES ☐ NO | |
| G6 | **Evidence Template Ready** | Is a blank copy of `110-p5c-postgresql-drill-evidence-template.md` available for filling? | ☐ YES ☐ NO | |
| G7 | **Time Window** | Is a low-write window scheduled, and does the operator have ≥1 hour of uninterrupted time? | ☐ YES ☐ NO | |
| G8 | **Cleanup Plan Understood** | Does the operator know how to drop drill databases, remove `.pgpass` files, and clear shell history after the drill? | ☐ YES ☐ NO | |
| G9 | **RPO/RTO Awareness** | Does the operator acknowledge that RPO=15min and RTO=30min are **design targets**, not guarantees? | ☐ YES ☐ NO | |
| G10 | **Rollback Posture** | If the drill fails, does the operator know **not** to retry against production and to follow the stop conditions in `118-target-host-p5c-drill-plan-adapted.md`? | ☐ YES ☐ NO | |
| G11 | **Schema Presence** | Does `\dt` in the target database show the expected FerrumGate tables (including `_migration_checkpoints`)? | ☐ YES ☐ NO / UNKNOWN | |
| G12 | **Backup Destination** | Is `<BACKUP_DIR>` created, writable, and excluded from automatic cleanup during the drill? | ☐ YES ☐ NO | |

---

## 3. Verdict

### 3.1 GO Criteria

**GO** is authorized only when **ALL** of the following are true:

- G1 = YES (Option B selected) **OR** G1 = N/A (drill is local-only rehearsal, not target-host)
- G2 = YES (non-production target)
- G3 = YES (connectivity confirmed)
- G4 = YES or UNKNOWN-with-plan (client version compatible or upgrade plan confirmed)
- G5 = YES (no inline passwords)
- G6 = YES (evidence template ready)
- G7 = YES (time window secured)
- G8 = YES (cleanup understood)
- G9 = YES (RPO/RTO understood as targets)
- G10 = YES (rollback posture confirmed)
- G11 = YES (schema present)
- G12 = YES (backup destination ready)

### 3.2 NO-GO Triggers

If **any** of the following are true, the verdict is **NO-GO**:

| Trigger | Why It Blocks |
|---------|---------------|
| G2 = NO | Drills must never run against production databases |
| G3 = NO | No point in attempting drill if host is unreachable |
| G5 = NO | Inline passwords are a security risk; drill is unsafe |
| G10 = NO | Operator may panic-retry against production on failure |
| G2 = NO AND G10 = NO | **Double-block** — highest risk combination |

If G1 = NO but the operator still wants to run a **local-only** rehearsal, that is acceptable but must be explicitly recorded as "local rehearsal; target-host GO not claimed."

---

## 4. Pre-Drill Safety Check

Immediately before executing the drill, perform these 60-second checks:

| # | Check | Command | Pass |
|---|-------|---------|------|
| S1 | Shell history is clean of passwords | `history | grep -i pass` returns nothing | ☐ |
| S2 | `.pgpass` permissions are strict | `ls -l ~/.pgpass` shows `-rw-------` | ☐ |
| S3 | No secrets in environment | `env | grep -i pass` returns nothing | ☐ |
| S4 | Backup directory exists and is writable | `touch <BACKUP_DIR>/.write_test && rm <BACKUP_DIR>/.write_test` | ☐ |
| S5 | Evidence template is open and ready | Visual confirmation | ☐ |

If any S1–S5 fails, **abort** the drill immediately and resolve before rescheduling.

---

## 5. Post-Drill Confirmation

After the drill (success or failure), confirm:

| # | Check | Status |
|---|-------|--------|
| P1 | Drill database dropped (if created) | ☐ YES ☐ N/A |
| P2 | `.pgpass` drill file removed (if created only for drill) | ☐ YES ☐ N/A |
| P3 | Backup artifacts retained or moved per retention policy | ☐ YES |
| P4 | Evidence template filled and reviewed for secrets | ☐ YES |
| P5 | This go/no-go checklist signed and dated | ☐ YES |

---

## 6. Operator Signoff

| Field | Value |
|-------|-------|
| Verdict | ☐ **GO** ☐ **NO-GO** ☐ **GO — Local Rehearsal Only** |
| Operator Name | ____________________ |
| Date | ____________________ |
| Signature | ____________________ |

> **Reminder**: A GO verdict on this checklist does NOT authorize production PostgreSQL deployment. Production remains gated on P5b–P5e completion and P6 assessment.

---

## 7. Cross-References

| This Checklist | Links To | Purpose |
|----------------|----------|---------|
| `121-p5c-target-host-go-no-go-checklist.md` | `117-postgresql-readiness-acceleration-plan.md` | Parent plan (Track A.4) |
| `121-p5c-target-host-go-no-go-checklist.md` | `118-target-host-p5c-drill-plan-adapted.md` | Drill procedures and stop conditions |
| `121-p5c-target-host-go-no-go-checklist.md` | `114-target-host-p5c-drill-checklist.md` | Operator step-by-step checklist |
| `121-p5c-target-host-go-no-go-checklist.md` | `110-p5c-postgresql-drill-evidence-template.md` | Fillable evidence template |
| `121-p5c-target-host-go-no-go-checklist.md` | `109-p5c-postgresql-backup-restore-runbook.md` | RPO/RTO targets and commands |
| `121-p5c-target-host-go-no-go-checklist.md` | `113-operator-path-selection-packet.md` | Option B prerequisite |

---

*Document created: 2026-05-13. P5c Target-Host Go/No-Go Checklist — operator self-assessment only. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim.*
