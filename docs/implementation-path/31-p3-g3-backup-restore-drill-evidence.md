# 31 — P3.G3 Backup / Restore Drill Evidence

**Purpose:** Operator drill record template for P3.G3 — backup capture and
restore drill under rollback scenario.

**Scope:** Single-node, SQLite-backed, v1 only.

**Audience:** Operators performing scheduled backup/restore drills, SREs
validating recovery procedures, compliance attestors.

**Last updated:** 2026-04-03.

---

## 0. Relationship to Other Documents

This document is the **P3.G3 evidence pack** for the production roadmap.
It complements the procedures in the operations runbook:

| Topic | Doc |
|---|---|
| Backup procedure (how to capture) | [18-single-node-operations-runbook.md Section 5](./18-single-node-operations-runbook.md#5-backup-procedure-manual-sqlite-file-backup) |
| Restore procedure (how to restore) | [18-single-node-operations-runbook.md Section 6](./18-single-node-operations-runbook.md#6-restore-procedure-manual-sqlite-file-restore) |
| Restore drill procedure | [18-single-node-operations-runbook.md Section 6.4](./18-single-node-operations-runbook.md#64-restore-drill-procedure-and-evidence) |
| Rollback/compensate reference | [18-single-node-operations-runbook.md Section 7](./18-single-node-operations-runbook.md#7-recovery-procedure-compensate--manual-restore-fallback) |

**Do not use this document as a procedures guide.** It is an evidence template.
Use the runbook sections above for step-by-step procedures.

---

## 1. Drill Overview

P3.G3 requires two operator-verifiable actions:

1. **Backup capture** — a successful SQLite file-level backup was taken,
   validated with `PRAGMA integrity_check`, and stored in the designated
   backup directory.
2. **Restore drill** — the backup was successfully restored to a test
   location, verified with `PRAGMA integrity_check`, and confirmed that
   a known durable governance record (intent and its provenance chain) is
   present in the restored store.

> **Drill cadence:** Perform at minimum once per quarter and after any
> backup script or storage infrastructure change. See runbook Section 6.4.

---

## 2. Backup Capture Evidence

### 2.1 Executed Drill Record — 2026-04-03

```
Backup Capture Record — FerrumGate v1 Single-Node
==================================================
Date:                  2026-04-03
Time (UTC):            ~17:05 (from backup filename)
Operator:              live drill (automated)
Node ID:               localhost

--- Pre-backup checks ---
ferrumd running:        yes
Store DSN:             sqlite:///tmp/ferrum-p3g3/ferrumgate.db
Store file path:        /tmp/ferrum-p3g3/ferrumgate.db
Disk space available:  yes (checked)

--- Backup operation ---
Backup command used:   cp (file-level copy)
Backup source:         /tmp/ferrum-p3g3/ferrumgate.db
Backup destination:    /tmp/ferrum-p3g3/backups/ferrumgate_20260403_1705.db
Backup size (bytes):   225280
Backup duration (s):   unknown

--- Integrity validation ---
Integrity check cmd:    sqlite3 /tmp/ferrum-p3g3/backups/ferrumgate_20260403_1705.db "PRAGMA integrity_check;"
Integrity result:       ok

--- Post-backup checks ---
Backup file exists:     yes
Backup permissions:     (default)
Retention policy met:   N/A — first drill

Overall backup outcome: PASS
Notes:                  Drill on localhost; backup captured from running ferrumd
                        store and validated before restore drill commenced.
```

### 2.2 Pass Criteria — Backup Capture

| Check | Required | Result |
|---|---|---|
| `PRAGMA integrity_check` returns `ok` | Yes | PASS — returned `ok` |
| Backup file is non-zero size | Yes | PASS — 225280 bytes |
| Backup destination is in the designated backup directory | Yes | PASS — `/tmp/ferrum-p3g3/backups/` |

---

## 3. Restore Drill Evidence

Complete one block per restore drill event. Drill must be performed on a
**non-production store** (temporary test path). Do not overwrite the
production store during a drill.

### 3.1 Executed Drill Record — 2026-04-03

```
Restore Drill Record — FerrumGate v1 Single-Node
==================================================
Date:                  2026-04-03
Time (UTC):            ~17:05 (drill run)
Operator:              live drill (automated)
Node ID:               localhost
Drill type:            ad-hoc (first drill)

--- Pre-drill ---
Backup file used:      /tmp/ferrum-p3g3/backups/ferrumgate_20260403_1705.db
Production store:      /tmp/ferrum-p3g3/ferrumgate.db (NOT modified)
Test store path:      /tmp/ferrum-p3g3/restored.db
Production ferrumd:   running (source node at port 18082)

--- Restore operation ---
Restore command used:  cp
Test store created:    yes
Test store size (bytes): 225280

--- Integrity validation ---
Integrity check cmd:    sqlite3 /tmp/ferrum-p3g3/restored.db "PRAGMA integrity_check;"
Integrity result:       ok

--- Post-restore functional probe ---
Test ferrumd started:   yes
Test ferrumd port:     18083
readyz endpoint:        GET /v1/readyz
readyz HTTP status:     200
readyz outcome:         PASS

approvals endpoint:     GET /v1/approvals?limit=1
approvals HTTP status:  200
approvals JSON parseable: yes
approvals outcome:      PASS

--- Record presence check ---
Known intent_id:        09996e3b-7a9b-4c55-b806-8713486cee44
Intent present:         present (intent_count=1, provenance_event_count=1)
Intent status:          Active
Intent normalized_goal: create durable record for backup restore drill

Overall restore outcome: PASS
Notes:                  Drill confirms backup/restore preserves intent chain.
                        Source ferrumd (port 18082) was NOT stopped; restored
                        store verified on separate port (18083) to avoid conflict.
```

### 3.2 Pass Criteria — Restore Drill

| Check | Required | Result |
|---|---|---|
| `PRAGMA integrity_check` on test store returns `ok` | Yes | PASS — returned `ok` |
| Test ferrumd responds to `readyz` with 200 | Yes | PASS — 200, `{"status":"ready"}` |
| `GET /v1/approvals?limit=1` returns 200 with valid JSON | Yes | PASS — 200, `{"items":[]}` |
| A known durable governance record (intent + provenance) is present in test store | Yes | PASS — intent_id `09996e3b-7a9b-4c55-b806-8713486cee44` verified present with 1 provenance event |
| Production store was NOT modified | Yes | PASS — source store at `/tmp/ferrum-p3g3/ferrumgate.db` untouched; restore was to `/tmp/ferrum-p3g3/restored.db` |

---

## 4. Combined Attestation Block

```
P3.G3 — Backup / Restore Drill — Operator Attestation
======================================================
Date of backup capture:  2026-04-03
Date of restore drill:   2026-04-03
Operator:                live drill (automated)
Node ID:                 localhost

Backup capture outcome:  PASS
Restore drill outcome:   PASS

I confirm:
  [x] The backup was captured using an approved procedure.
  [x] The backup passed PRAGMA integrity_check.
  [x] The restore drill was performed on a non-production store.
  [x] The restored store passed all functional probe checks.
  [x] All pass criteria in Sections 2 and 3 above are satisfied.

Drill findings:          None. Backup and restore both succeeded; intent and
                        provenance records persisted correctly across the drill.
Corrective actions taken: None.

Overall P3.G3 verdict:   PASS
Operator sign-off:       live drill attestation — 2026-04-03
