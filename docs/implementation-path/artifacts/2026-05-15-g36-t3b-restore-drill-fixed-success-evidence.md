# Artifact: 2026-05-15 G3.6 T3b Restore Drill — Fixed Binary Success Evidence

> **Type**: Fixed destructive restore-to-production drill success evidence artifact
> **Date**: 2026-05-15
> **Scope**: Root-cause fix (`std::fs::copy` for pre-restore snapshot), target deploy, fixed T3b execution, success verification
> **Status**: **T3b RESTORE DRILL SUCCESS; G3.6 FULL ACCEPTANCE FOR P5b ENGINEERING REVIEW ONLY**
> **Run ID**: `20260515T074001Z`

---

## 1. Executive Summary

This artifact records the successful fixed T3b destructive restore-to-production drill. The prior attempt timed out because the restore path used a slow SQLite backup API for the pre-restore snapshot. The fix replaces the slow snapshot with `std::fs::copy`, reducing pre-restore snapshot time from ~205s to <1s. The fixed binary was deployed to the target host and T3b executed successfully.

| Aspect | Result |
|--------|--------|
| Root-cause identified | **PASS** — slow SQLite backup API (`run_to_completion(5, 250ms)`) for pre-restore snapshot caused ~205s for ~4100 pages |
| Fix implemented | **PASS** — `backup_restore()` now uses `std::fs::copy` for pre-restore snapshot after exclusive lock check; `copy_db_snapshot()` retained for backup create |
| Local tests | **PASS** — `cargo test --package ferrumctl backup`: 16/16 passed; `cargo check --package ferrumctl`: passed; release build: passed |
| Target deploy | **PASS** — fixed binary installed at `/opt/ferrumgate/ferrumctl`; old binary backed up |
| Fixed preflight | **PASS** — service active; readyz 200; backup verify OK; dry-run OK; metrics defaults 2/50 |
| Fixed T3b execution | **PASS** — restore completed in 0.463s; live DB verify OK; service restarted; readyz/deep 200 |
| G3.6 full accepted | **YES — FOR P5b ENGINEERING REVIEW ONLY** |

---

## 2. Root Cause

The prior T3b attempt (run `20260515T070905Z`) timed out after 180s because `ferrumctl backup restore` used the SQLite backup API for the pre-restore snapshot:

- **Location**: `bins/ferrumctl/src/backup.rs`, function `backup_restore()`
- **Slow path**: `run_to_completion(5, 250ms)` copied the live DB page-by-page via SQLite's backup API
- **Observed time**: ~205 seconds for ~4,100 pages
- **Timeout**: The restore wrapper had a 180s timeout, causing the command to abort before completion

---

## 3. Fix Implementation

### 3.1 Code Change

`backup_restore()` in `bins/ferrumctl/src/backup.rs` was updated:

- **Before**: Pre-restore snapshot used `copy_db_snapshot()` which internally called `run_to_completion(5, 250ms)` (SQLite backup API)
- **After**: Pre-restore snapshot uses `std::fs::copy` after the exclusive lock check passes. `copy_db_snapshot()` is retained for backup creation only.

### 3.2 Local Validation

| Check | Command | Result |
|-------|---------|--------|
| Unit tests | `cargo test --package ferrumctl backup` | 16/16 passed |
| Compile check | `cargo check --package ferrumctl` | Passed |
| Release build | `cargo build --bin ferrumctl --release` | Passed |

### 3.3 Test Coverage

Existing tests validate:
- Backup creation (`copy_db_snapshot` path)
- Backup verify (integrity check)
- Restore dry-run (exclusive lock check)
- The `std::fs::copy` path for pre-restore snapshot is exercised during restore integration tests

---

## 4. Target Deploy

| Step | Result |
|------|--------|
| Fixed binary path (build) | `target/release/ferrumctl` |
| Staging copy on target | `/tmp/ferrumctl.restore-fix` |
| Old binary backup | `/opt/ferrumgate/ferrumctl.backup-restore-timeout-20260515T073819Z` |
| Fixed binary installed | `/opt/ferrumgate/ferrumctl` (9,955,584 bytes) |
| Install method | `cp /tmp/ferrumctl.restore-fix /opt/ferrumgate/ferrumctl` |

---

## 5. Fixed T3b Execution

### 5.1 Run Identification

| Field | Value |
|-------|-------|
| Run ID | `20260515T074001Z` |
| Log file | `/tmp/ferrum-g36-t3b-restore-drill-fixed-20260515T074001Z.log` |
| Summary file | `/tmp/ferrum-g36-t3b-restore-drill-fixed-20260515T074001Z.json` |

### 5.2 Preflight

| Check | Result |
|-------|--------|
| Service status | Active |
| `/v1/readyz` | HTTP 200 |
| Latest backup | `/var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db` (16,060,416 bytes, mtime `2026-05-15T06:50:07Z`) |
| Backup verify | Exit 0; `Database integrity check passed` and `OK` |
| Dry-run restore | Exit 0 |
| Rate-limit config | `per_second=2`, `burst=50` |

### 5.3 Pre-Restore Copy

| Field | Value |
|-------|-------|
| Pre-restore copy path | `/var/lib/ferrumgate/data/ferrumgate.db.pre_restore_t3b_fixed_20260515T074001Z` |
| Pre-restore copy size | 16,060,416 bytes |

### 5.4 Service Stop

| Check | Result |
|-------|--------|
| Stop command exit code | 0 |
| Service stopped | Confirmed |

### 5.5 Actual Restore (SUCCESS)

| Field | Value |
|-------|-------|
| Command | `sudo /opt/ferrumgate/ferrumctl backup restore --db-path /var/lib/ferrumgate/data/ferrumgate.db --from /var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db --confirm` |
| Exit code | 0 |
| Elapsed time | **0.463 seconds** |
| Output includes | `Pre-restore snapshot saved`, `Database integrity check passed`, `Database restored successfully`, `Restore complete` |

### 5.6 Post-Restore Verification

| Check | Result | Evidence |
|-------|--------|----------|
| Live DB verify | **OK** | `ferrumctl backup verify --db-path /var/lib/ferrumgate/data/ferrumgate.db` exit 0; output: `Database integrity check passed` and `OK` |
| Service start | Exit 0 | `systemctl start ferrumgate.service` |
| `/v1/readyz` | HTTP 200 | Target-local curl |
| `/v1/readyz/deep` | HTTP 200 | Target-local curl |
| Service status | Active | `systemctl is-active ferrumgate.service` |
| Rate-limit config | `per_second=2`, `burst=50` | `/v1/metrics` gauges |
| Firewall | `118.69.4.63/32` | GCP firewall rule check |
| Sentinel marker | `T3B_FIXED_RESTORE_DRILL_SUCCESS` | Recorded in log/summary |

---

## 6. Impact Assessment

| Criterion | Status | Rationale |
|-----------|--------|-----------|
| Root-cause fix | **VALIDATED** | `std::fs::copy` reduces pre-restore snapshot from ~205s to <1s |
| Local tests | **PASS** | 16/16 backup tests passed; release build passed |
| Target deploy | **PASS** | Fixed binary installed; old binary backed up |
| Fixed preflight | **PASS** | All pre-checks healthy before restore |
| Fixed T3b restore | **PASS** | Restore completed in 0.463s; live DB verify OK; service healthy |
| T3b accepted | **YES** | Restore completed successfully within RTO |
| G3.6 full accepted | **YES — FOR P5b ENGINEERING REVIEW ONLY** | A1–A6 now met with real evidence |

---

## 7. Conservative Verdict

The fixed T3b destructive restore-to-production drill **completed successfully** in 0.463 seconds. Post-restore verification confirmed the live DB integrity, service health, and operational readiness.

**G3.6 is FULLY ACCEPTED for P5b engineering review only.**

Current status:

- `T3b restore drill (fixed)`: **PASS** (0.463s; live DB verify OK; service healthy).
- `G3.6 full accepted`: **YES — FOR P5b ENGINEERING REVIEW ONLY**.

**Explicit non-claims** (still valid):
- This does **NOT** make FerrumGate production-ready.
- This does **NOT** declare the pilot environment pilot-ready.
- This does **NOT** validate HA/multi-node behavior.
- This does **NOT** authorize PostgreSQL production deployment.
- P5b–P5e implementation requires engineering go-ahead and operator path decision in addition to G3.6.

---

## 8. Next Steps

| # | Action | Owner |
|---|--------|-------|
| N1 | Proceed to P5b engineering review with G3.6 evidence | Engineering |
| N2 | Operator path decision (doc 113) if not already decided | Operator |
| N3 | P5b conservative-default implementation with post-deploy monitoring | Engineering + Operator |
| N4 | P5c–P5e completion before production deployment assessment | Engineering |

---

## 9. Cross-References

| Document | Purpose |
|----------|---------|
| [`106-g3-6-pilot-metrics-evidence-packet.md`](../106-g3-6-pilot-metrics-evidence-packet.md) | G3.6 evidence packet and acceptance assessment |
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan and acceptance checklist |
| [`2026-05-15-g36-t3b-restore-drill-timeout-evidence.md`](./2026-05-15-g36-t3b-restore-drill-timeout-evidence.md) | Prior T3b timeout attempt evidence (historical) |
| [`2026-05-14-g36-a3-spike-confirmatory-evidence.md`](./2026-05-14-g36-a3-spike-confirmatory-evidence.md) | A3/spike confirmatory evidence |
| [`2026-05-14-g36-p0-p1-full-rerun-evidence.md`](./2026-05-14-g36-p0-p1-full-rerun-evidence.md) | P0+P1 full-duration rerun evidence |

---

## 10. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-15 | Fixed T3b success evidence artifact created. Records root-cause fix (`std::fs::copy` for pre-restore snapshot), target deploy, successful restore in 0.463s, and G3.6 full acceptance for P5b engineering review only. | Engineering |

---

*Artifact created: 2026-05-15. No secrets, no token values. G3.6 FULL ACCEPTANCE for P5b engineering review only. No production-ready claim. No pilot-ready claim. No HA/multi-node claim. No PostgreSQL production deployment claim.*
