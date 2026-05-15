# Artifact: 2026-05-14 G3.6 D1b Rehearsal Evidence

> **Type**: Rehearsal evidence artifact
> **Date**: 2026-05-14
> **Scope**: Target-side D1b rehearsal using repository-owned scripts (`run_g36_d1b_rehearsal_target.sh`, `run_g36_workload_wrapper.sh`, `run_real_workload_generator.py`)
> **Status**: Rehearsal **completed successfully (sanitized)**. G3.6 remains **NOT ACCEPTED**.
> **Policy in effect**: D1b (`rate_limit_per_second=5`, `burst=100`) for test window only.

---

## 1. Executive Summary

This artifact documents the target-side D1b rehearsal conducted on 2026-05-14 using the repository-owned control scripts. The rehearsal validated:

1. **C1 (wrapper)**: Truthful sentinel semantics, no-orphan guarantee, deferred config revert.
2. **C2 (checkpoints)**: Incremental per-phase checkpoint JSON writes.
3. **C3 (drift detection)**: Mid-run config-drift probe capability.

The rehearsal was **not** a G3.6 acceptance run. It used a **short phase set** (baseline 5s, target 20s, cooldown 5s) to validate script behavior end-to-end before any full-duration acceptance attempt.

### 1.1 Token Logging Incident and Remediation

The **first** rehearsal attempt succeeded functionally but exposed a security issue: `wrapper_stdout.log` contained the full bearer token because the wrapper logged the generator command array including `--bearer-token <token>`.

| Aspect | Detail |
|--------|--------|
| **Cause** | `log "Generator command: ${GENERATOR_CMD[*]}"` in `run_g36_workload_wrapper.sh` line 318 |
| **Impact** | Full bearer token written to `wrapper_stdout.log` on target host |
| **Immediate action** | Target artifact sanitized; target bearer token **rotated**; `ferrumgate.service` restarted and confirmed active |
| **Fix** | Wrapper modified to build `GENERATOR_CMD_LOG` array with `<REDACTED>` substitution before logging |
| **Verification** | `HAS_REDACTED_ARG=True`, `HAS_LONG_HEX_TOKEN=False` on sanitized rerun output |

---

## 2. Rehearsal Sequence

### 2.1 Script Versions

| Script | Version / Commit | Notes |
|--------|-----------------|-------|
| `run_g36_d1b_rehearsal_target.sh` | Repository version (post-C1/C2/C3 implementation) | Target-side rehearsal orchestrator |
| `run_g36_workload_wrapper.sh` | Repository version (post-token-logging fix) | Sanitized command logging |
| `run_real_workload_generator.py` | Repository version (post-checkpoint/drift additions) | Incremental checkpoints + drift detection |

### 2.2 Rehearsal Configuration

| Parameter | Value |
|-----------|-------|
| Server URL | `http://127.0.0.1:19080` |
| Rate limit per second | 5 |
| Rate limit burst | 100 |
| Phases | baseline 5s @ 0 rps, target 20s @ 1 rps, cooldown 5s @ 0 rps |
| Output directory | `/tmp/ferrum-g36-d1b-rehearsal-20260514_100456` |

### 2.3 Execution Steps

1. **Backup**: `/etc/ferrumgate/env` copied to timestamped backup file.
2. **Apply D1b**: Removed existing `FERRUMD_RATE_LIMIT_PER_SECOND`/`FERRUMD_RATE_LIMIT_BURST` lines; appended `5`/`100`; restarted `ferrumgate.service`.
3. **Pre-run E-checks**: `/v1/metrics` confirmed `ferrumgate_rate_limit_per_second=5`, `ferrumgate_rate_limit_burst=100`.
4. **Generator execution**: Workload generator ran all three phases.
5. **Post-generator revert**: Backup restored to `/etc/ferrumgate/env`; service restarted.
6. **Sentinel**: `COMPLETE.status` written with `exit_code=0`.

---

## 3. Rehearsal Results

### 3.1 Sentinel

```json
{
  "timestamp": "2026-05-14T10:06:14Z",
  "stage": "generator_completed",
  "exit_code": 0,
  "generator_exit_code": 0,
  "revert_exit_code": 0,
  "generator_pid": <pid>,
  "reason": "Generator exited successfully; Revert succeeded",
  "output_dir": "/tmp/ferrum-g36-d1b-rehearsal-20260514_100456"
}
```

### 3.2 Artifact Inventory

All expected artifacts were present in the output directory:

| Artifact | Present | Notes |
|----------|---------|-------|
| `workload_results.json` | Yes | Full results with per-phase summary |
| `workload_results.md` | Yes | Human-readable results |
| `workload_plan.json` | Yes | Plan generated before execution |
| `workload_plan.md` | Yes | Human-readable plan |
| `checkpoint_phase_000.json` | Yes | Baseline phase checkpoint |
| `checkpoint_phase_001.json` | Yes | Target phase checkpoint |
| `checkpoint_phase_002.json` | Yes | Cooldown phase checkpoint |
| `readyz_probe_log.json` | Yes | Post-workload readyz/deep probes |
| `readyz_probe_log.md` | Yes | Human-readable probe log |
| `metrics_prerun.txt` | Yes | Pre-run /v1/metrics scrape |
| `wrapper_stdout.log` | Yes | **Sanitized** — no full token |
| `wrapper_stderr.log` | Yes | Wrapper stderr |
| `generator_stdout.log` | Yes | Generator stdout |
| `generator_stderr.log` | Yes | Generator stderr |
| `RUN_SUMMARY.txt` | Yes | Human-readable run summary |
| `sentinel/COMPLETE.status` | Yes | Truthful sentinel |

### 3.3 Workload Summary

| Metric | Value |
|--------|-------|
| `aborted` | `false` |
| `abort_reason` | (empty) |
| Baseline requests | 0 |
| Target requests | 20 |
| Target HTTP 200 | 20 |
| Target HTTP 429 | 0 |
| Cooldown requests | 0 |

> **Note**: The 20 requests at 1 rps for 20s is consistent with the short rehearsal phase definition. This is **not** representative of sustained target-load validation.

### 3.4 Token Sanitization Verification

| Check | Result |
|-------|--------|
| `wrapper_stdout.log` contains `--bearer-token <REDACTED>` | Yes (`HAS_REDACTED_ARG=True`) |
| `wrapper_stdout.log` contains long hex token pattern | No (`HAS_LONG_HEX_TOKEN=False`) |
| `generator_stdout.log` contains token leakage | No (generator uses env var, not CLI arg) |
| `RUN_SUMMARY.txt` contains token | No |

---

## 4. Post-Rehearsal State Verification

| Check | Result | Evidence |
|-------|--------|----------|
| Active wrapper/rehearsal/generator processes | **None** | Process listing |
| `ferrumgate.service` status | **Active** | `systemctl status` |
| D1b env keys in `/etc/ferrumgate/env` | **Absent** | File inspection |
| Effective rate-limit config | `per_second=2`, `burst=50` (pre-test defaults) | `/v1/metrics` gauges |
| Firewall | Restored to `118.69.4.63/32` | Firewall rule check |

---

## 5. Impact Assessment

| Criterion | Status | Rationale |
|-----------|--------|-----------|
| G3.6 full accepted | **NO** | Rehearsal used short phases (5s/20s/5s), not the required 30-minute target phase. No sustained write-rate evidence. |
| G3.6 conditionally accepted | **NO** | Rehearsal is not an acceptance run; no adapter-exercised metrics or queue-depth data produced. |
| Pilot-ready | **NO** | Short rehearsal only; full G3.6 acceptance criteria A1–A6 remain unmet. |
| Production-ready | **NO** | No production-ready claim is made. |
| D1b policy validated | **PARTIAL** | Controls C1–C3 behaved correctly under real target execution. D1b effectiveness under sustained load remains unproven. |
| Token logging issue | **FIXED** | Wrapper sanitized and verified on target. |

---

## 6. Recommendations

### 6.1 Before G3.6 Acceptance Rerun

| # | Recommendation | Priority | Rationale |
|---|----------------|----------|-----------|
| R1 | Use the **same** sanitized wrapper + rehearsal script for the acceptance rerun | **P0** | Token logging fix is confirmed working on target. |
| R2 | Run **full-duration phases** (baseline 600s, low 600s, target 1800s, cooldown 600s) | **P0** | Acceptance requires sustained target-load evidence. |
| R3 | Confirm pre-run E-checks pass before starting generator | **P0** | Already validated during rehearsal; maintain for acceptance. |
| R4 | Verify checkpoint files are written after each phase | **P1** | Validated during rehearsal (3 checkpoint files produced). |
| R5 | Monitor for config drift during target phase | **P1** | Drift detection is active; no drift occurred during short rehearsal. |

### 6.2 Next Steps

1. Schedule full G3.6 acceptance rerun using D1b policy with full-duration phases.
2. Ensure operator is available for signoff if acceptance criteria A1–A6 are met.
3. Continue to treat D1b as a **test-window exception** only; revert to default limits after acceptance run.

---

## 7. Cross-References

| Document | Purpose |
|----------|---------|
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan; updated with rehearsal result |
| [`2026-05-14-g36-d1b-root-cause-evidence.md`](./2026-05-14-g36-d1b-root-cause-evidence.md) | D1b root-cause investigation (wrapper bug, orphan generator) |
| [`2026-05-14-g36-d1b-robust-run-evidence.md`](./2026-05-14-g36-d1b-robust-run-evidence.md) | Original robust-run evidence (pre-fix) |

---

## 8. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-14 | Rehearsal evidence artifact created. Documents sanitized successful rehearsal, token-logging incident + remediation, and conservative non-acceptance status. | Engineering |

---

*Artifact created: 2026-05-14. Rehearsal evidence only. No secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
