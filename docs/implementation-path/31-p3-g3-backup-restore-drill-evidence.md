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
   known execution and approval records are present in the restored store.

> **Drill cadence:** Perform at minimum once per quarter and after any
> backup script or storage infrastructure change. See runbook Section 6.4.

---

## 2. Backup Capture Evidence Template

Complete one block per backup capture event.

```
Backup Capture Record — FerrumGate v1 Single-Node
==================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>

--- Pre-backup checks ---
ferrumd running:       <yes | no>
Store DSN:             <sqlite:///path or sqlite::memory:>
Store file path:       <absolute path to the .db file>
Disk space available:  <yes | no | checked>

--- Backup operation ---
Backup command used:   <cp | rsync | sqlite3 .backup | other>
Backup source:         <store path>
Backup destination:    <path/to/backup_YYYYMMDD_HHMMSS.db>
Backup size (bytes):   <number>
Backup duration (s):  <number | unknown>

--- Integrity validation ---
Integrity check cmd:   sqlite3 <backup_path> "PRAGMA integrity_check;"
Integrity result:      <ok | FAIL>

--- Post-backup checks ---
Backup file exists:    <yes | no>
Backup permissions:    <restrictive | open>
Retention policy met:  <yes | no | N/A — first drill>

Overall backup outcome: <PASS | FAIL>
Notes:                 <any observations or corrective actions>
```

### Backup Capture Pass Criteria

| Check | Required |
|---|---|
| `PRAGMA integrity_check` returns `ok` | Yes |
| Backup file is non-zero size | Yes |
| Backup destination is in the designated backup directory | Yes |
| Retention policy satisfied (3 daily + 1 weekly minimum) | Yes (after first drill) |

---

## 3. Restore Drill Evidence Template

Complete one block per restore drill event. Drill must be performed on a
**non-production store** (temporary test path). Do not overwrite the
production store during a drill.

```
Restore Drill Record — FerrumGate v1 Single-Node
==================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>
Drill type:            <scheduled quarterly | post-infrastructure-change | ad-hoc>

--- Pre-drill ---
Backup file used:      <path/to/backup_YYYYMMDD_HHMMSS.db>
Production store:      <path/to/production.db> (NOT modified)
Test store path:       <path/to/test_restore.db>
Production ferrumd:    <stopped | running (drill node)>

--- Restore operation ---
Restore command used:  <cp | sqlite3 .restore | other>
Test store created:    <yes | no>
Test store size (bytes): <number>

--- Integrity validation ---
Integrity check cmd:   sqlite3 <test_store_path> "PRAGMA integrity_check;"
Integrity result:      <ok | FAIL>

--- Post-restore functional probe ---
Test ferrumd started:  <yes | no | N/A — used production ferrumd on drill node>
Test ferrumd port:     <port>
readyz endpoint:       GET /v1/readyz
readyz HTTP status:    <200 | other>
readyz outcome:        <PASS | FAIL>

approvals endpoint:    GET /v1/approvals?limit=1
approvals HTTP status:  <200 | other>
approvals JSON parseable: <yes | no>
approvals outcome:     <PASS | FAIL>

--- Record presence check ---
Known execution_id:    <id or "none available">
Execution present:    <present | absent | SKIP>
Known approval_id:     <id or "none available">
Approval present:      <present | absent | SKIP>

Overall restore outcome: <PASS | FAIL>
Notes:                 <any observations or corrective actions>
```

### Restore Drill Pass Criteria

| Check | Required |
|---|---|
| `PRAGMA integrity_check` on test store returns `ok` | Yes |
| Test ferrumd responds to `readyz` with 200 | Yes |
| `GET /v1/approvals?limit=1` returns 200 with valid JSON | Yes |
| At least one known execution or approval record is present in test store | Yes |
| Production store was NOT modified | Yes |

---

## 4. Combined Attestation Block

```
P3.G3 — Backup / Restore Drill — Operator Attestation
=======================================================
Date of backup capture:  <YYYY-MM-DD>
Date of restore drill:  <YYYY-MM-DD>
Operator:                <name or ticket>
Node ID:                 <host or instance identifier>

Backup capture outcome:  <PASS | FAIL>
Restore drill outcome:  <PASS | FAIL>

I confirm:
  [ ] The backup was captured using an approved procedure.
  [ ] The backup passed PRAGMA integrity_check.
  [ ] The restore drill was performed on a non-production store.
  [ ] The restored store passed all functional probe checks.
  [ ] All pass criteria in Sections 2 and 3 above are satisfied.

Drill findings:          <none | describe any anomalies>
Corrective actions taken: <none | describe actions>

Overall P3.G3 verdict:  <PASS | FAIL — requires re-drill>
Operator sign-off:      <name / ticket / date>
