# Artifact: 2026-05-14 G3.6 Rerun Evidence (Commit 7bcb025)

> **Type**: Evidence artifact (execution results, not a readiness claim)
> **Date**: 2026-05-14
> **Scope**: G3.6 real workload generator rerun with `trusted_context` fix and adapter-mix exercise
> **Status**: **EXECUTED / NOT ACCEPTED**. G3.6 full accepted remains **NO**.
> **Output directory (VM)**: `/tmp/ferrum-g36-workload-7bcb025-rerun`
> **Associated commit**: `7bcb025` (`fix(stress): prepare G3.6 rerun evidence`)
> **Local artifacts**: `workload_results.json`, `readyz_probe_log.json` copied from VM output dir

---

## Summary

This artifact documents the G3.6 rerun executed at commit `7bcb025`.
The `trusted_context` normalization fix resolved all HTTP 422 errors.
All five adapters in the mix were exercised and returned HTTP 200 responses.
However, the rate limiter rejected a large share of requests at target and spike
loads, preventing G3.6 full acceptance per doc 116 §12.2 (>5% HTTP 429 at target
load disqualifies the run).

---

## Run Metadata

| Field | Value |
|-------|-------|
| Total requests | 3,340 |
| HTTP 200 (OK) | 1,132 |
| HTTP 429 (Too Many Requests) | 2,208 |
| HTTP 422 (Unprocessable Entity) | **0** |
| HTTP 2xx rate (overall) | ~33.9% |
| Output directory (VM) | `/tmp/ferrum-g36-workload-7bcb025-rerun` |
| Associated commit | `7bcb025` |
| G3.6 full accepted | **NO** |

---

## Latency Summary

| Percentile | Latency (ms) |
|------------|-------------:|
| p50 | 1.67 |
| p95 | 2.70 |
| p99 | 3.8066 |
| min | 1.14 |
| max | 66.34 |

> **Note**: Latencies remain low because many requests were rejected at the
> rate-limit layer before adapter execution. Low latency does **not** indicate
> that all requests were fully processed.

---

## Phase Breakdown

### Baseline / Cooldown

| Phase | Duration | Rate (rps) | Requests | Status |
|-------|----------|-----------|----------|--------|
| Baseline | 600 s | 0.0 | 0 | Idle |
| Cooldown | 600 s | 0.0 | 0 | Idle |

No workload requests were sent during baseline or cooldown.

### Low Load

| Metric | Value |
|--------|-------|
| Duration | 600 s |
| Rate | 0.1 rps |
| Total requests | 60 |
| HTTP 200 | 60 |
| HTTP 429 | 0 |
| 2xx rate | 100% |

#### Per-Adapter Distribution (Low)

| Adapter | HTTP 200 | HTTP 429 |
|---------|----------|----------|
| fs | 12 | 0 |
| git | 13 | 0 |
| http | 9 | 0 |
| maildraft | 12 | 0 |
| sqlite | 14 | 0 |

All adapters returned HTTP 200 at low load.

### Target Load

| Metric | Value |
|--------|-------|
| Duration | 1,800 s |
| Rate | 1.0 rps |
| Total requests | 1,799 |
| HTTP 200 | 922 |
| HTTP 429 | 877 |
| 2xx rate | ~51.3% |
| 429 rate | ~48.7% |

#### Per-Adapter Distribution (Target)

| Adapter | HTTP 200 | HTTP 429 | 2xx Rate |
|---------|----------|----------|----------|
| fs | 188 | 176 | ~51.6% |
| git | 178 | 174 | ~50.6% |
| http | 201 | 183 | ~52.3% |
| maildraft | 167 | 169 | ~49.7% |
| sqlite | 188 | 175 | ~51.8% |

The rate limiter rejected approximately half of all requests at target load.
This exceeds the doc 116 §12.2 threshold (>5% HTTP 429 at target load).

### Spike Load

| Metric | Value |
|--------|-------|
| Duration | 300 s |
| Rate | 5.0 rps |
| Total requests | 1,481 |
| HTTP 200 | 150 |
| HTTP 429 | 1,331 |
| 2xx rate | ~10.1% |
| 429 rate | ~89.9% |

#### Per-Adapter Distribution (Spike)

| Adapter | HTTP 200 | HTTP 429 | 2xx Rate |
|---------|----------|----------|----------|
| fs | 25 | 276 | ~8.3% |
| git | 30 | 247 | ~10.8% |
| http | 35 | 292 | ~10.7% |
| maildraft | 34 | 260 | ~11.6% |
| sqlite | 26 | 256 | ~9.2% |

The rate limiter rejected approximately 90% of requests at spike load.

---

## Readyz / Deep Probe Results (Post-Run)

| Probe | Status | Details |
|-------|--------|---------|
| 5 / 5 | HTTP 200 | No errors |

> **Caution**: As noted in prior artifacts, `readyz/deep` HTTP 200 measures
> infrastructure health (store, queue) and does **not** confirm that workload
> requests were successfully processed by adapters.

---

## Security & Post-Run Hygiene

### Secret Scan

A secret scan was run against the workload artifact files:

| File | Result |
|------|--------|
| `workload_results.json` | **NONE** |
| `readyz_probe_log.json` | **NONE** |
| `workload_results.md` | **NONE** |
| `readyz_probe_log.md` | **NONE** |

No bearer tokens, secrets, or sensitive literals were found.

### SSH Firewall

Post-run, the SSH firewall was restored to `118.69.4.63/32`.

---

## Root-Cause Analysis

| Symptom | Status | Analysis |
|---------|--------|----------|
| HTTP 422 | **RESOLVED** | `trusted_context` normalization fix eliminated all 422 errors (was 1,104 in prior run) |
| HTTP 429 | **BLOCKING** | Rate limiter continues to reject ~48.7% at target and ~89.9% at spike; this blocks G3.6 acceptance |
| Adapter mix exercise | **PARTIAL** | All 5 adapters returned HTTP 200, confirming mix templates and `trusted_context` are correct, but volume of successful adapter executions is too low for acceptance |

### Rate-Limit Assessment

The current rate-limit policy appears to enforce a threshold that is:
- **Below** 1 req/s sustained (target load)
- **Far below** 5 req/s burst (spike load)

For G3.6 acceptance, one of the following must occur before the next rerun:
1. **Raise rate limits** for the test principal(s) or globally, **or**
2. **Reduce target load** to stay under the limit (may not validate spike behavior), **or**
3. **Document the limit as the operational ceiling** and accept a reduced target for G3.6 evidence (requires operator decision).

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- The `trusted_context` fix eliminated HTTP 422 errors completely.
- All 5 adapters in the mix returned HTTP 200 at least once.
- Low-load phase (0.1 rps) achieved 100% HTTP 200.
- `readyz/deep` infrastructure probes remained healthy post-run.
- No secrets were found in artifact files.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| G3.6 full accepted | **NO** | ~48.7% HTTP 429 at target load exceeds the >5% disqualification threshold. |
| G3.6 conditionally accepted | **NO** | This run does not upgrade or sustain any conditional acceptance; it is a partial execution with a known blocker. |
| Adapter paths fully validated | **NO** | Adapters returned 2xx, but at insufficient volume and with heavy rate-limit interference. |
| Production-ready | **NO** | No production-ready claim is made. |
| Pilot-ready | **NO** | Drills and workload evidence are prerequisites, not sufficient conditions. |

---

## Next Actions

| Action | Owner | Notes |
|--------|-------|-------|
| ~~Decide rate-limit / load policy before next rerun~~ | ~~Operator~~ | **DONE** — See doc 116 §14.1: D1 selected (`rate_limit_per_second=2` for test window, burst=50, revert required), D2 rejected, D3 acknowledged (~1 rps production ceiling). |
| Re-run workload generator under D1 policy | Engineering / Operator | Target-focused sequence: baseline 600s → low 600s → target 1800s @ 1 rps → cooldown 600s. No spike in acceptance rerun. |
| Confirm per-adapter 2xx at target load with >95% success | Engineering / Operator | Required for G3.6 acceptance per doc 116 §13.1. |
| Revert rate limit after test | Engineering / Operator | D1 is test-window only; revert to pre-test state after run concludes. |
| Treat spike / backpressure as separate optional test | Engineering / Operator | Only after target phase passes; spike is not part of acceptance rerun. |

---

*Artifact created: 2026-05-14. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
