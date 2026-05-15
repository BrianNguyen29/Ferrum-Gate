# Artifact: 2026-05-14 G3.6 D1b Full-Duration Acceptance Rerun Evidence

> **Type**: Full-duration acceptance rerun evidence artifact
> **Date**: 2026-05-14
> **Scope**: Full G3.6 acceptance rerun under D1b policy (baseline→low→target→cooldown) using repository-owned scripts on target host
> **Status**: **NOT ACCEPTED** — target-phase HTTP 429 rate ≈80.4% exceeds ≤5% threshold.
> **Policy in effect**: D1b (`rate_limit_per_second=5`, `burst=100`) for test window only.

---

## 1. Executive Summary

This artifact documents the **full-duration** G3.6 acceptance rerun executed on 2026-05-14 under D1b policy using the sanitized, repository-owned wrapper and generator scripts.

| Aspect | Result |
|--------|--------|
| Wrapper / generator completion | **Completed** — sentinel `exit_code=0`, `generator_exit_code=0`, `revert_exit_code=0` |
| Target-phase HTTP 429 rate | **≈80.4%** (1,448 / 1,801 requests) — **FAILS** acceptance threshold (≤5%) |
| G3.6 full accepted | **NO** |
| G3.6 conditionally accepted | **NO** |
| Pilot-ready | **NO** |
| Production-ready | **NO** |

The run **completed without wrapper anomalies** (no orphan processes, no mid-run config revert, no missing artifacts). However, the target phase produced an unacceptable rate of HTTP 429 responses, meaning the effective rate-limit ceiling under D1b is still insufficient for sustained 1 rps workload plus diagnostic probes.

---

## 2. Run Configuration

| Parameter | Value |
|-----------|-------|
| Server URL | `http://127.0.0.1:19080` (target host) |
| Rate limit per second | 5 |
| Rate limit burst | 100 |
| Phases | baseline 600s @ 0 rps, low 600s @ 0.1 rps, target 1800s @ 1.0 rps, cooldown 600s @ 0 rps |
| Output directory | `/tmp/ferrum-g36-d1b-full-acceptance-20260514` |
| Wrapper exit | `2026-05-14T13:29:18Z` |

---

## 3. Sentinel

```json
{
  "timestamp": "2026-05-14T13:29:18Z",
  "stage": "generator_completed",
  "exit_code": 0,
  "generator_exit_code": 0,
  "revert_exit_code": 0,
  "generator_pid": <pid>,
  "reason": "Generator exited successfully; Revert succeeded",
  "output_dir": "/tmp/ferrum-g36-d1b-full-acceptance-20260514"
}
```

> **Note**: Unlike the earlier robust-run anomaly, the wrapper **did not** trigger a mid-run revert. The generator ran to completion and the revert command executed **after** generator exit.

---

## 4. Artifact Inventory

All expected artifacts were present in the output directory:

| Artifact | Present | Size / Notes |
|----------|---------|--------------|
| `workload_results.json` | Yes | 967,596 bytes |
| `workload_results.md` | Yes | Human-readable results |
| `workload_plan.json` | Yes | Plan generated before execution |
| `workload_plan.md` | Yes | Human-readable plan |
| `checkpoint_phase_000.json` | Yes | Baseline phase checkpoint |
| `checkpoint_phase_001.json` | Yes | Low phase checkpoint |
| `checkpoint_phase_002.json` | Yes | Target phase checkpoint |
| `checkpoint_phase_003.json` | Yes | Cooldown phase checkpoint |
| `readyz_probe_log.json` | Yes | ReadyZ probe log |
| `readyz_probe_log.md` | Yes | Human-readable probe log |
| `metrics_prerun.txt` | Yes | Pre-run `/v1/metrics` scrape |
| `config_drift_log.jsonl` | Yes | Background drift probe log |
| `wrapper_stdout.log` | Yes | **Sanitized** — no full token |
| `wrapper_stderr.log` | Yes | Wrapper stderr |
| `generator_stdout.log` | Yes | Generator stdout |
| `generator_stderr.log` | Yes | Generator stderr |
| `RUN_SUMMARY.txt` | Yes | Human-readable run summary |
| `sentinel/COMPLETE.status` | Yes | Truthful sentinel |

---

## 5. Workload Results

### 5.1 Overall Summary

| Metric | Value |
|--------|-------|
| Total requests | 1,861 |
| HTTP 200 | 413 |
| HTTP 429 | 1,448 |
| Latency p50 | 1.6 ms |
| Latency p95 | 2.67 ms |
| Latency p99 | 3.926 ms |
| Latency max | 27.89 ms |

### 5.2 Per-Phase Breakdown

| Phase | Duration | Requests | HTTP 200 | HTTP 429 | Notes |
|-------|----------|----------|----------|----------|-------|
| Baseline | 600 s | 0 | 0 | 0 | Idle — no requests issued |
| Low | 600 s | 60 | 60 | 0 | 100% success; warm-up confirmed |
| Target | 1,800 s | 1,801 | 353 | 1,448 | **≈80.4% 429 — acceptance failure** |
| Cooldown | 600 s | 0 | 0 | 0 | Idle — no requests issued |

> **Acceptance criterion**: Target phase must achieve ≤5% HTTP 429. Actual rate ≈80.4% → **FAIL**.

### 5.3 Per-Adapter Breakdown (Target Phase Only)

| Adapter | HTTP 200 | HTTP 429 | Total | 200 Rate | Notes |
|---------|----------|----------|-------|----------|-------|
| FS | 72 | 284 | 356 | 20.2% | |
| Git | 67 | 282 | 349 | 19.2% | |
| HTTP | 75 | 289 | 364 | 20.6% | |
| Maildraft | 72 | 296 | 368 | 19.6% | |
| SQLite | 67 | 297 | 364 | 18.4% | |
| **Total** | **353** | **1,448** | **1,801** | **19.6%** | |

> All adapters were exercised, but every adapter experienced heavy throttling. No adapter-specific exemption from the shared rate-limit bucket is in effect.

---

## 6. Supporting Evidence

### 6.1 `readyz_probe_log.json`

The `readyz_probe_log.json` artifact is present. However, the parsed summary indicates **0 probes** were recorded. Do **not** use this artifact to claim readyz success or failure. The absence of probes may indicate that the probe logic did not fire during this run configuration, or that the log format differed from the parser expectation.

### 6.2 `config_drift_log.jsonl`

The background drift probe log shows repeated `metrics_probe_failed` events with HTTP 429 during the target phase. Eventually the log records `drift_cleared`. This means:

- The monitoring endpoint (`/v1/metrics`) itself was **also throttled** during the target phase.
- The drift probe could not reliably read rate-limit gauges while the workload was active.
- This is a **diagnostic finding / secondary blocker**, not a pass.

| Observation | Interpretation |
|-------------|----------------|
| `metrics_probe_failed` HTTP 429 | Drift probe could not scrape metrics due to shared rate-limit bucket |
| `drift_cleared` | Probe recovered after workload pressure dropped |

> **Conclusion**: The shared rate-limit bucket applies to **both** workload requests and diagnostic probes, compounding observability gaps under load.

---

## 7. Post-Run State Verification

| Check | Result | Evidence |
|-------|--------|----------|
| Active wrapper / generator processes | **None** | Process listing after run |
| `ferrumgate.service` status | **Active** | `systemctl status` |
| D1b env keys in `/etc/ferrumgate/env` | **Absent** | File inspection |
| Effective rate-limit config | `per_second=2`, `burst=50` (pre-test defaults) | `/v1/metrics` gauges post-revert |
| Firewall | Restored to `118.69.4.63/32` | Firewall rule check |

---

## 8. Secret Handling and Token Process-List Exposure Remediation

### 8.1 Incident

During earlier execution stages, the **old wrapper** passed the bearer token to the generator via `--bearer-token <token>` on the command line (argv). Process-list polling (`ps`, `/proc/*/cmdline`) therefore **exposed the token** while the generator was running.

| Aspect | Detail |
|--------|--------|
| **Cause** | Old wrapper used `--bearer-token` CLI argument |
| **Impact** | Token visible in target process listings |
| **Immediate action** | Target bearer token **rotated** again; `ferrumgate.service` restarted and confirmed active |

### 8.2 Fix

The wrapper was updated to pass the token **exclusively** via the `FERRUM_BEARER_TOKEN` environment variable. The `--bearer-token` argument was removed from the generator invocation.

| Check | Result |
|-------|--------|
| `wrapper_stdout.log` contains long hex64 token pattern | **False** |
| `wrapper_stdout.log` contains `--bearer-token` with redacted value | **True** (`<REDACTED>`) |
| Generator receives token via env var | **Confirmed** |
| Updated wrapper deployed to target | **Yes** |
| Updated wrapper validated on target | **Yes** (rehearsal + full run) |

> **Token value is not recorded in this artifact.** The exposed token has been rotated; do not reuse it.

---

## 9. Impact Assessment

| Criterion | Status | Rationale |
|-----------|--------|-----------|
| G3.6 full accepted | **NO** | Target-phase 429 rate ≈80.4% far exceeds ≤5% threshold. |
| G3.6 conditionally accepted | **NO** | No sustained write-rate evidence at acceptable success rate. |
| Pilot-ready | **NO** | Rate limiter blocks representative workload. |
| Production-ready | **NO** | No production-ready claim is made. |
| D1b policy validated | **NO** | D1b ceiling (5 rps / burst 100) is insufficient for sustained 1 rps + probes under observed key-extractor behavior. |
| Controls C1–C3 | **PASSED** | Wrapper, checkpoints, and drift probe all behaved correctly. |
| Token logging / argv exposure | **FIXED** | Wrapper sanitized; env-only token passing confirmed. |

---

## 10. Root Cause (Observed)

The **primary blocker** is the **shared rate-limit bucket** combined with the `PeerIpKeyExtractor` behavior on the target host:

1. All traffic from the same source IP (generator + drift probe + any other requests) shares a single rate-limit quota.
2. At 1 rps sustained for 30 minutes, plus intermittent drift probes, the bucket drains and the vast majority of requests are throttled.
3. The D1b ceiling (`per_second=5`, `burst=100`) **does not** provide enough effective headroom under this extractor configuration.

**Contributing factor**: The drift probe's own metrics scrapes are counted against the same bucket, creating a feedback loop where the probe cannot reliably verify config during high load.

---

## 11. Recommendations

### 11.1 Before Next G3.6 Acceptance Rerun

| # | Recommendation | Priority | Rationale |
|---|----------------|----------|-----------|
| R1 | **Exempt authenticated `/v1/metrics` and `/v1/readyz` from the workload rate limiter**, or place them in a separate bucket | **P0** | Probes must be observable during workload execution. Current shared bucket breaks drift detection. |
| R2 | **Raise the rate-limit policy** (e.g., `per_second≥10`, `burst≥200`) **or** switch to a per-principal / per-endpoint extractor | **P0** | 5 rps ceiling is insufficient for 1 rps workload + probes under `PeerIpKeyExtractor`. |
| R3 | **Adjust acceptance criteria only with explicit operator decision** | **P1** | Lowering the target load or raising the 429 threshold requires signoff, not engineering unilateral action. |
| R4 | **Ensure env-only wrapper is used for every future rerun** | **P0** | Token argv exposure is eliminated; maintain the fix. |
| R5 | **Investigate `SmartIpKeyExtractor` vs `PeerIpKeyExtractor` asymmetry** between test and production | **P1** | Production uses default `PeerIpKeyExtractor`; test helper uses `SmartIpKeyExtractor`. Aligning them may change observed behavior. |

### 11.2 Next Steps

1. Engineering evaluates whether to exempt monitoring endpoints from rate limiting or create a dedicated bucket.
2. Operator decides whether to raise the production/test rate-limit ceiling or adjust the acceptance load criteria.
3. Once the above decisions are implemented and deployed, schedule another full G3.6 acceptance rerun.
4. Continue to treat any rate-limit change as a **test-window exception**; revert to conservative defaults after the run.

---

## 12. Cross-References

| Document | Purpose |
|----------|---------|
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan; updated with full rerun verdict |
| [`2026-05-14-g36-d1b-rehearsal-evidence.md`](./2026-05-14-g36-d1b-rehearsal-evidence.md) | D1b rehearsal evidence (sanitized wrapper, short phases) |
| [`2026-05-14-g36-d1b-robust-run-evidence.md`](./2026-05-14-g36-d1b-robust-run-evidence.md) | Original robust-run evidence (pre-wrapper-fix, INCOMPLETE) |
| [`2026-05-14-g36-d1b-root-cause-evidence.md`](./2026-05-14-g36-d1b-root-cause-evidence.md) | Root-cause investigation (wrapper finalizer bug, orphan generator) |

---

## 13. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-14 | Full rerun evidence artifact created. Documents completed but NOT ACCEPTED full-duration run, 80.4% target-phase 429 rate, token process-list exposure remediation, env-only wrapper fix, and next-blocker recommendations. | Engineering |

---

*Artifact created: 2026-05-14. Full-duration acceptance rerun evidence. No secrets, no token values, no production-ready claim, no pilot-ready claim. G3.6 remains NOT ACCEPTED.*
