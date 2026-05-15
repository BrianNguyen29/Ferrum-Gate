# Artifact: 2026-05-15 G3.6 T3b Restore Drill — Timeout + Rollback Evidence

> **Type**: Destructive restore-to-production drill attempt evidence artifact
> **Date**: 2026-05-15
> **Scope**: T3b restore drill attempt, 180s timeout, rollback success, post-rollback verification
> **Status**: **T3b ATTEMPTED / NOT ACCEPTED — RESTORE COMMAND TIMED OUT; ROLLBACK SUCCEEDED; G3.6 FULL ACCEPTANCE STILL NO**
> **Run ID**: `20260515T070905Z`

---

## 1. Executive Summary

This artifact records the T3b destructive restore-to-production drill attempt executed with explicit user authorization. The restore command timed out after 180 seconds. Rollback was executed successfully and the target host returned to a healthy operational state.

| Aspect | Result |
|--------|--------|
| User authorization | **YES** — explicit user approval obtained before T3b execution |
| Pre-checks | **PASS** — service active; readyz/deep HTTP 200 |
| Backup selection | **PASS** — `/var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db` (16,060,416 bytes) |
| Backup verify before restore | **PASS** — exit 0; `Database integrity check passed` and `OK` |
| Pre-restore copy created | **PASS** — `/var/lib/ferrumgate/data/ferrumgate.db.pre_restore_t3b_20260515T070905Z` (16,060,416 bytes) |
| Dry-run restore | **PASS** — exit 0; backup integrity check passed; exclusive lock check passed |
| Service stop | **PASS** — exit 0 |
| Actual restore | **TIMEOUT** — `ferrumctl backup restore` timed out after 180s |
| Rollback execution | **SUCCESS** — pre-restore copy restored; service restarted; readyz/deep HTTP 200 |
| Post-rollback live DB verify | **PASS** — `Database integrity check passed: /var/lib/ferrumgate/data/ferrumgate.db` and `OK` |
| No lingering restore process | **PASS** — no ferrumctl restore process found |
| Post-rollback metrics | **PASS** — `ferrumgate_rate_limit_per_second 2`, `ferrumgate_rate_limit_burst 50` |
| SSH firewall | **RESTORED** — `118.69.4.63/32` |
| T3b accepted | **NO** — restore did not complete successfully |
| G3.6 full accepted | **NO** — at the time of this artifact; see [`2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md`](./2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md) for the fixed rerun |

---

## 2. T3b Execution Log

### 2.1 Run Identification

| Field | Value |
|-------|-------|
| Run ID | `20260515T070905Z` |
| Log file | `/tmp/ferrum-g36-t3b-restore-drill-20260515T070905Z.log` |
| Summary file | `/tmp/ferrum-g36-t3b-restore-drill-20260515T070905Z.json` |

### 2.2 Pre-Checks

| Check | Result |
|-------|--------|
| Service status | Active |
| `/v1/readyz` | HTTP 200 |
| `/v1/readyz/deep` | HTTP 200 |

### 2.3 Backup Selection and Verify

| Field | Value |
|-------|-------|
| Backup file | `/var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db` |
| Backup size | 16,060,416 bytes |
| Backup mtime | `2026-05-15T06:50:07Z` |
| Backup verify exit code | 0 |
| Backup verify output | `Database integrity check passed` and `OK` |

### 2.4 Pre-Restore Copy

| Field | Value |
|-------|-------|
| Pre-restore copy path | `/var/lib/ferrumgate/data/ferrumgate.db.pre_restore_t3b_20260515T070905Z` |
| Pre-restore copy size | 16,060,416 bytes |

### 2.5 Dry-Run Restore

| Check | Result |
|-------|--------|
| Dry-run exit code | 0 |
| Backup integrity check | Passed |
| Exclusive lock check | Passed |
| Dry-run completion | Confirmed |

### 2.6 Service Stop

| Check | Result |
|-------|--------|
| Stop command exit code | 0 |
| Service stopped | Confirmed |

### 2.7 Actual Restore Attempt (TIMED OUT)

| Field | Value |
|-------|-------|
| Command | `sudo /opt/ferrumgate/ferrumctl backup restore --db-path /var/lib/ferrumgate/data/ferrumgate.db --from /var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db --confirm` |
| Timeout | 180 seconds |
| Result | **TIMEOUT** — command did not complete within the timeout window |
| Live DB state after timeout | Unconfirmed — restore may have partially progressed |

### 2.8 Rollback Execution

Rollback was executed immediately after the restore timeout to return the service to a known-good state:

| Step | Result |
|------|--------|
| Service stop (pre-rollback) | Exit 0 |
| Copy pre-restore copy to live DB path | Completed |
| Service start | Exit 0 |
| `/v1/readyz` | HTTP 200 |
| `/v1/readyz/deep` | HTTP 200 |

### 2.9 Post-Rollback Verification

| Check | Result | Evidence |
|-------|--------|----------|
| Service status | Active | `systemctl is-active ferrumgate.service` |
| `/v1/readyz` | HTTP 200 | Target-local curl |
| `/v1/readyz/deep` | HTTP 200 | Target-local curl |
| Live DB verify | **OK** | `ferrumctl backup verify --db-path /var/lib/ferrumgate/data/ferrumgate.db` exit 0; output: `Database integrity check passed: /var/lib/ferrumgate/data/ferrumgate.db` and `OK` |
| No lingering restore process | **PASS** — no ferrumctl restore process found | `ps` / process listing |
| Rate-limit config | `per_second=2`, `burst=50` | `/v1/metrics` gauges |
| SSH firewall | `118.69.4.63/32` | GCP firewall rule check |

---

## 3. Impact Assessment

| Criterion | Status | Rationale |
|-----------|--------|-----------|
| User authorization | **PASS** | Explicit user YES obtained before T3b attempt |
| Pre-checks | **PASS** | Service healthy before attempt |
| Backup verify | **PASS** | Backup integrity confirmed before restore |
| Pre-restore copy | **PASS** | Live DB copied before stop |
| Dry-run restore | **PASS** | No issues detected in dry-run |
| Service stop | **PASS** | Clean stop before restore |
| Actual restore | **TIMEOUT / NOT ACCEPTED** | Command did not complete within 180s |
| Rollback | **SUCCESS** | Service returned to healthy state with pre-restore copy |
| Post-rollback DB integrity | **PASS** | Live DB verify OK after rollback |
| T3b accepted | **NO** | Restore did not complete; drill NOT accepted |
| G3.6 full accepted | **NO** | T3b remains the blocking criterion |

---

## 4. Conservative Verdict

The T3b destructive restore-to-production drill was **attempted with explicit user authorization** but the restore command **timed out after 180 seconds** and did not complete successfully.

**Rollback succeeded**: the pre-restore copy was restored to the live DB path, the service was restarted, and post-rollback verification confirmed the target host returned to a healthy operational state (`readyz/deep` HTTP 200, live DB integrity check passed).

However, **T3b is NOT accepted** because the restore itself did not complete. At the time of this artifact, G3.6 full acceptance remained blocked.

> **Update**: A root-cause fix (`std::fs::copy` for pre-restore snapshot) was implemented and a successful fixed T3b rerun completed on 2026-05-15. See [`2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md`](./2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md).

Current conservative status (as of this artifact):

- `T3b restore drill`: **ATTEMPTED / NOT ACCEPTED** — timeout after 180s.
- `T3b rollback`: **SUCCESS** — service restored healthy.
- `G3.6 full accepted`: **NO** — T3b restore completion remained the blocking criterion at the time of this artifact.

---

## 5. Next Steps (Pending)

Before the next T3b attempt, the following should be investigated:

| # | Action | Owner |
|---|--------|-------|
| N1 | Root-cause analysis: why did `ferrumctl backup restore` timeout after 180s? | Engineering |
| N2 | Evaluate alternate restore methods (e.g., `cp` + service restart, `sqlite3` CLI restore) | Engineering |
| N3 | Increase timeout or add progress logging to restore command if appropriate | Engineering |
| N4 | Re-attempt T3b only after root cause is understood or alternate method is validated | Operator (with explicit YES) |

---

## 5.1 Root-Cause Analysis (Added Post-Fix)

**Finding**: `backup_restore()` in `bins/ferrumctl/src/backup.rs` created the pre-restore snapshot by calling `copy_db_snapshot()`, which uses the rusqlite backup API (`run_to_completion(5, Duration::from_millis(250), None)`). For a ~16 MB database (~4,100 pages) this step alone can take ~205 s, exceeding the 180 s external T3b timeout.

**Fix applied**: In the restore path, after the exclusive lock check confirms no concurrent writers, the pre-restore snapshot is now created with `std::fs::copy(db_path, pre_restore_path)` instead of `copy_db_snapshot()`. This reduces the pre-restore copy from ~200 s to <1 s. The `copy_db_snapshot()` function is retained for `backup create` where online consistency may still matter.

**Safety preserved**:
- `--confirm` still required (or `--dry-run`).
- Backup integrity check still runs before any mutation.
- Exclusive lock check still refuses locked databases.
- Pre-restore copy still created before overwriting.
- Restored database still verified with `PRAGMA integrity_check`.

---

## 6. Cross-References

| Document | Purpose |
|----------|---------|
| [`106-g3-6-pilot-metrics-evidence-packet.md`](../106-g3-6-pilot-metrics-evidence-packet.md) | G3.6 evidence packet and acceptance assessment |
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan and acceptance checklist |
| [`2026-05-14-g36-a3-spike-confirmatory-evidence.md`](./2026-05-14-g36-a3-spike-confirmatory-evidence.md) | A3/spike confirmatory evidence and safe preflight |
| [`2026-05-14-g36-p0-p1-full-rerun-evidence.md`](./2026-05-14-g36-p0-p1-full-rerun-evidence.md) | P0+P1 full-duration rerun evidence |

---

## 7. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-15 | T3b restore drill attempt evidence artifact created. Records attempted restore with timeout, successful rollback, post-rollback verification, and explicit NOT ACCEPTED status. | Engineering |

---

*Artifact created: 2026-05-15. No secrets, no token values, no production-ready claim, no pilot-ready claim, no full acceptance claim. T3b restore drill NOT ACCEPTED due to timeout. G3.6 full acceptance remains blocked pending root-cause investigation or alternate restore method.*
