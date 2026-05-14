# Artifact: 2026-05-14 G3.6 D1 Abort Evidence

> **Type**: Evidence artifact (execution results, not a readiness claim)
> **Date**: 2026-05-14
> **Scope**: G3.6 D1 target-focused rerun attempt and intentional abort
> **Status**: **ABORTED**. G3.6 full accepted remains **NO**.
> **Associated commit**: `7bcb025`
> **Policy in effect at start**: D1 (`rate_limit_per_second=2`, `burst=50`)

---

## Summary

This artifact documents the D1 target-focused rerun attempt executed on
2026-05-14. The D1 policy raised `rate_limit_per_second` to **2** and `burst`
to **50**. The low phase completed successfully with 100% HTTP 200. However,
once the target phase began, the rate limiter rapidly produced HTTP 429 errors.
ReadyZ and metrics endpoint probes during the target phase returned
"Too Many Requests! Wait for 1s", indicating the D1 configuration did not
achieve the intended rate-limit relaxation. The run was intentionally aborted
via Ctrl+C. Configuration was reverted to the pre-test state.

---

## Run Metadata

| Field | Value |
|-------|-------|
| Policy | D1 (`FERRUMD_RATE_LIMIT_PER_SECOND=2`, `FERRUMD_RATE_LIMIT_BURST=50`) |
| Service status at start | Active |
| Estimated total requests (plan) | 1,860 |
| Run outcome | **Intentionally aborted** |
| Abort trigger | Ctrl+C by operator |
| G3.6 full accepted | **NO** |

---

## Phase Execution

### Baseline

| Phase | Duration | Rate | Requests | Notes |
|-------|----------|------|----------|-------|
| Baseline | 600 s | 0 rps | 0 | Idle; no workload requests |

### Low Phase

| Phase | Duration | Rate | Status |
|-------|----------|------|--------|
| Low | 600 s | 0.1 rps | **All visible HTTP 200** |

The low phase completed successfully with no observed HTTP 429 errors.

### Target Phase (Aborted)

| Phase | Duration | Rate | Status |
|-------|----------|------|--------|
| Target | ~<1,800 s (aborted mid-phase) | 1 rps | **Aborted after req ~88** |

- Target phase began at designed 1 rps.
- By request ~88, **many HTTP 429 errors** were observed.
- The rate limiter was rejecting requests aggressively despite the D1
  `rate_limit_per_second=2` setting.

### Cooldown

No cooldown phase was reached; the run was aborted during target.

---

## Mid-Run Diagnostic Probes

During the target phase, `readyz` and `/v1/metrics` probes were attempted.
Both returned:

> **"Too Many Requests! Wait for 1s"**

This indicates the rate limiter was throttling **all** traffic to the server,
including diagnostic and health probes. The observed wait time of **~1s** is
inconsistent with the D1 expectation of reduced rate-limit pressure.

### Interpretation

The D1 configuration (`rate_limit_per_second=2`, `burst=50`) did **not**
effectively raise the operational rate-limit ceiling. Possible explanations:
- The rate-limit configuration may not have been fully propagated or applied.
- Another rate-limit layer (e.g., per-IP, per-endpoint, or middleware) may be
  enforcing a stricter limit independently of the configured per-second rate.
- The GCRA (Generic Cell Rate Algorithm) parameters may produce a ~1s wait
  regardless of the `rate_limit_per_second` value at this configuration level.

---

## Post-Abort Recovery

### Configuration Revert

| Step | Action | Result |
|------|--------|--------|
| 1 | Reverted config from `/etc/ferrumgate/env.g36-d1-backup-20260514040250` | Success |
| 2 | Service restarted | Active |
| 3 | ReadyZ probe post-revert | HTTP 200 |
| 4 | SSH firewall | Restored to `118.69.4.63/32` |

The system returned to a known-good state after abort.

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- D1 policy was applied and tested.
- Low phase (0.1 rps) succeeded with 100% HTTP 200.
- Target phase at 1 rps produced rapid 429s under D1 config.
- Diagnostic probes confirmed rate-limit throttling with ~1s wait.
- Abort and revert were executed safely.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| D1 policy effective | **NO** | Rate-limit throttling persisted with ~1s wait despite D1 settings. |
| G3.6 full accepted | **NO** | Run was aborted before completion. |
| G3.6 conditionally accepted | **NO** | No new acceptance evidence produced. |
| Production-ready | **NO** | No production-ready claim is made. |
| Pilot-ready | **NO** | Drills and workload evidence are prerequisites, not sufficient conditions. |

---

## Next Actions

| Action | Owner | Notes |
|--------|-------|-------|
| Select D1b policy with higher rate-limit parameters | Operator | See doc 116 §14.1: `rate_limit_per_second=5`, `burst=100`. |
| Add mandatory pre-run verification (V-2/V-4) | Engineering / Operator | Must confirm 429 wait ~0.2s before starting workload. If wait is ~1s or >0.3s, STOP. |
| Re-run target-focused sequence under D1b | Engineering / Operator | baseline→low→target→cooldown, no spike. |
| Revert config after test | Engineering / Operator | Required for all test-window policy changes. |

---

*Artifact created: 2026-05-14. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
