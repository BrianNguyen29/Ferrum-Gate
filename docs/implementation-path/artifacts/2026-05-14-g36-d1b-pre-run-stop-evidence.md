# Artifact: 2026-05-14 G3.6 D1b Pre-Run Verification Failure Evidence

> **Type**: Evidence artifact (execution results, not a readiness claim)
> **Date**: 2026-05-14
> **Scope**: G3.6 D1b pre-run verification attempt and intentional STOP
> **Status**: **STOPPED / NOT EXECUTED**. G3.6 full accepted remains **NO**.
> **Associated commit**: `7bcb025`
> **Policy in effect at start**: D1b (`rate_limit_per_second=5`, `burst=100`)

---

## Summary

This artifact documents the D1b pre-run verification attempt executed on
2026-05-14. The D1b policy raised `rate_limit_per_second` to **5** and `burst`
to **100**. Service was active and readyz returned HTTP 200. However, the
mandatory pre-run verification checks V-2 (readyz burst probe) and V-4 (metrics
burst probe) failed. The rate limiter continued to enforce long waits
inconsistent with the D1b configuration. The operator invoked the STOP rule:
workload was **not started**, and the configuration was reverted.

---

## Run Metadata

| Field | Value |
|-------|-------|
| Policy | D1b (`FERRUMD_RATE_LIMIT_PER_SECOND=5`, `FERRUMD_RATE_LIMIT_BURST=100`) |
| Service status at start | Active |
| ReadyZ at start | HTTP 200 |
| Workload started | **NO** |
| Outcome | **STOPPED per verification rule** |
| G3.6 full accepted | **NO** |

---

## Pre-Run Verification Results

### V-1: Config Applied

| Check | Expected | Actual | Status |
|---|---|---|---|
| `FERRUMD_RATE_LIMIT_PER_SECOND` | 5 | 5 | **PASS** |
| `FERRUMD_RATE_LIMIT_BURST` | 100 | 100 | **PASS** |

V-1 passed: environment variables matched D1b specification.

### V-2: ReadyZ Burst Probe

| Check | Expected | Actual | Status |
|---|---|---|---|
| Response body does NOT contain "Wait for ~1s" | No ~1s wait | Sample: "Wait for 0s" | Ambiguous |
| Status distribution | Predominantly 200 | `{'200': 86, '429': 94}` | **FAIL** |

- 86 HTTP 200 responses
- 94 HTTP 429 responses
- Sample body: "Wait for 0s"

> **Interpretation**: The readyz endpoint returned a mix of 200 and 429 under
> burst probe. While some samples showed "Wait for 0s", the 429 rate (~52.2%)
> indicates the rate limiter was still aggressively throttling. This is
> inconsistent with a `rate_limit_per_second=5` / `burst=100` configuration
> that should easily absorb diagnostic probes.

### V-3: Service Active and ReadyZ 200

| Check | Expected | Actual | Status |
|---|---|---|---|
| Service active | Yes | Active | **PASS** |
| ReadyZ HTTP 200 | Yes | HTTP 200 | **PASS** |

V-3 passed.

### V-4: Metrics Burst Probe

| Check | Expected | Actual | Status |
|---|---|---|---|
| Response body does NOT contain "Wait for ~1s" | No ~1s wait | Sample: "Wait for 4s" | **FAIL** |
| Status distribution | Predominantly 200 | `{'429': 178, '200': 2}` | **FAIL** |

- 2 HTTP 200 responses
- 178 HTTP 429 responses
- Sample body: "Wait for 4s"

> **Interpretation**: The metrics endpoint was almost entirely blocked by the
> rate limiter. A wait of **4s** is far above the D1b acceptance threshold of
> **~0.2s** and the STOP threshold of **>0.3s**. This confirms the rate-limit
> configuration was **not effectively applied** to the metrics endpoint, or
> another rate-limit layer is overriding the D1b settings.

---

## STOP Decision

Per doc 116 §14.2 Mandatory Pre-Run Verification:

> **STOP** if wait is ~1s or >0.3s.

V-4 produced a sample wait of **4s**, which exceeds the STOP threshold by an
order of magnitude. V-2 produced 94 HTTP 429s out of 180 probes (~52.2% 429
rate). Both checks indicate the rate limiter is not behaving as configured.

**Decision**: STOP. Do not start workload generator.

---

## Post-STOP Recovery

| Step | Action | Result |
|------|--------|--------|
| 1 | Reverted env config from D1b to pre-test state | Success |
| 2 | Service restarted | Active |
| 3 | ReadyZ probe post-revert | HTTP 200 |
| 4 | SSH firewall | Restored to `118.69.4.63/32` |

---

## Root-Cause Analysis

| Hypothesis | Evidence | Likelihood |
|---|---|---|
| D1b env vars not propagated to the active rate-limit layer | Config vars set, but metrics/readyz still throttled heavily | High |
| A separate per-endpoint or per-IP rate limit overrides D1b | readyz and metrics both throttled despite different paths | High |
| GCRA parameters produce long waits independent of configured per-second rate | V-4 showed 4s wait; V-2 showed 0s but with 52% 429 rate | Medium |
| Service restart was insufficient to reload rate-limit config | Config may require full process restart or different reload signal | Medium |

**Key finding**: Setting env vars is **not sufficient evidence** that the
effective rate-limit configuration has changed. The D1, D1b, and prior runs all
suffered from the same gap: **we cannot observe the effective rate-limit
configuration from outside the process.**

---

## Remediation Implemented

To close the observability gap, the following code changes were implemented:

1. **`/v1/metrics` exposes effective rate-limit gauges**:
   - `ferrumgate_rate_limit_per_second` — effective sustained rate limit
   - `ferrumgate_rate_limit_burst` — effective burst size

2. **Startup log includes effective rate-limit config**:
   - `ferrumd` startup `tracing::info!` now logs `rate_limit_per_second` and
     `rate_limit_burst` from the resolved `ServerConfig`.

These changes provide **deterministic, outside-the-process evidence** of the
effective rate-limit configuration. Future verification must read these values
from `/v1/metrics` or startup logs **before** trusting that a config change has
 taken effect.

---

## New Verification Rule

**No G3.6 workload rerun may proceed until the following deterministic
evidence confirms the effective rate-limit configuration:**

| # | Evidence Source | Required Reading | Pass Criteria | Stop Criteria |
|---|---|---|---|---|
| E-1 | `/v1/metrics` | `ferrumgate_rate_limit_per_second` gauge | Matches intended policy value | **STOP** — value does not match policy |
| E-2 | `/v1/metrics` | `ferrumgate_rate_limit_burst` gauge | Matches intended policy value | **STOP** — value does not match policy |
| E-3 | Startup log or runtime config query | `rate_limit_per_second` and `rate_limit_burst` fields | Matches intended policy value | **STOP** — value does not match policy |

> **Rule**: If E-1, E-2, or E-3 does not match the intended policy, the
> configuration change has not taken effect. Do not proceed to V-2/V-4 burst
> probes. Investigate config propagation and retry.

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- D1b env vars were set correctly (5/100).
- Pre-run verification detected that the effective rate-limit configuration did
  not match the intended policy.
- STOP rule was correctly invoked; workload was not started.
- Config was safely reverted post-STOP.
- Code changes now provide deterministic observability of effective rate-limit
  config via `/v1/metrics` and startup logs.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| D1b policy effective | **NO** | Effective rate limit did not match configured values. |
| G3.6 full accepted | **NO** | No workload executed. |
| G3.6 conditionally accepted | **NO** | No new acceptance evidence produced. |
| Production-ready | **NO** | No production-ready claim is made. |
| Pilot-ready | **NO** | Drills and workload evidence are prerequisites, not sufficient conditions. |

---

## Next Actions

| Action | Owner | Notes |
|--------|-------|-------|
| Deploy build with rate-limit metrics/startup log | Engineering | Provides deterministic evidence of effective config. |
| Re-attempt D1b after deployment | Engineering / Operator | Read `ferrumgate_rate_limit_per_second` and `ferrumgate_rate_limit_burst` from `/v1/metrics` before burst probes. |
| If metrics still show old values, investigate config propagation | Engineering | May require process restart, config file change, or layer-specific override. |
| Do not rerun until E-1 and E-2 pass | Operator | No exceptions. |

---

*Artifact created: 2026-05-14. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
