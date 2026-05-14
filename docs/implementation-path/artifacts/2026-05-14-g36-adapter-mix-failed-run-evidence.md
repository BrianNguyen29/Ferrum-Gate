# Artifact: 2026-05-14 G3.6 Adapter-Mix Failed/Non-Accepted Run Evidence

> **Type**: Evidence artifact (execution results, not a readiness claim)
> **Date**: 2026-05-14
> **Scope**: G3.6 real workload generator adapter-mix execution attempt
> **Status**: **FAILED / NOT ACCEPTED**. G3.6 full accepted remains **NO**.
> **Output directory**: `/tmp/ferrum-g36-workload-3fd8e30`
> **Associated commit**: `3fd8e30`

---

## Summary

This artifact documents the latest G3.6 adapter-mix workload execution attempt.
The run completed the full phase sequence, but the vast majority of requests were
rejected with HTTP 422 or HTTP 429. No adapter execution paths were meaningfully
validated. This run **does not** satisfy G3.6 acceptance criteria.

---

## Run Metadata

| Field | Value |
|-------|-------|
| Total requests | 3,355 |
| HTTP 422 (Unprocessable Entity) | 1,104 |
| HTTP 429 (Too Many Requests) | 2,251 |
| HTTP 2xx | 0 |
| Output directory | `/tmp/ferrum-g36-workload-3fd8e30` |
| Associated commit | `3fd8e30` |
| G3.6 full accepted | **NO** |

---

## Latency Summary

| Percentile | Latency (ms) |
|------------|-------------:|
| p50 | 1.64 |
| p95 | 2.31 |
| p99 | 2.9892 |
| min | 1.17 |
| max | 53.59 |

> **Note**: Latencies are very low because requests were rejected early (before
> adapter execution). Low latency does **not** indicate healthy adapter performance.

---

## Readyz / Deep Probe Results

| Probe | Status | Details |
|-------|--------|---------|
| 5 / 5 | HTTP 200 | `store_ok`: true, `write_queue_ok`: true, `depth`: 0, `threshold`: 100 |

> **Caution**: `readyz/deep` returned HTTP 200 for all probes, but this is
> **misleading** in the context of this failed run. The probes check server
> health at the infrastructure layer (store connectivity, queue depth), not
> whether workload requests are successfully processed. Since 3,355 requests
> were rejected before reaching adapter execution, `readyz/deep` HTTP 200 does
> **not** indicate a successful workload validation.

---

## Root-Cause Analysis (Preliminary)

| Symptom | Likely Cause | Evidence |
|---------|--------------|----------|
| HTTP 422 (1,104 requests) | Missing required `trusted_context` field in intent-compile payload | Oracle finding; payload templates in `run_real_workload_generator.py` did not include `trusted_context` at time of run |
| HTTP 429 (2,251 requests) | Rate limiter rejecting requests | High volume of 429s under target/spike load; consistent with configured rate limits |

### Trusted Context Gap

The workload generator templates (`ADAPTER_TEMPLATES`) did not include the
`trusted_context` field, which the server requires for intent compile requests.
This is analogous to the normalization already present in
`scripts/run_d1_d6_drills.py` (`_normalize_intent_compile_payload`), but was
missing from the G3.6 workload generator at execution time.

### Rate Limiter Interaction

Even after fixing the 422 root cause, the rate limiter is expected to throttle
requests at target (1 req/s) and spike (5 req/s) loads unless:
- The rate-limit configuration is relaxed for the test window, **or**
- The generator uses authenticated sessions with higher quotas, **or**
- The test is run at a rate below the limit threshold.

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- A workload generator run was executed with adapter-mix templates.
- The run produced measurable request counts, status distributions, and latency histograms.
- `readyz/deep` infrastructure probes passed at the time of observation.
- Specific failure modes (422, 429) were identified for remediation.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| G3.6 full accepted | **NO** | Run failed; 0 successful adapter executions. |
| G3.6 conditionally accepted | **NO** | This run does not upgrade or sustain any conditional acceptance; it is a failed attempt. |
| Adapter paths exercised | **NO** | 0 HTTP 2xx responses; no adapters executed. |
| Production-ready | **NO** | No production-ready claim is made. |
| Pilot-ready | **NO** | Drills and workload evidence are prerequisites, not sufficient conditions. |

---

## Remediation Taken (Post-Run)

1. **`trusted_context` normalization added**: `scripts/run_real_workload_generator.py`
   now applies `_normalize_intent_compile_payload` to every intent-compile
   payload, ensuring `trusted_context` is present in both plan output and live
   execution (mirrors `run_d1_d6_drills.py` behavior).

2. **Rate-limit precheck guidance added**: See
   `docs/implementation-path/116-g36-monitoring-execution-plan.md` §12 for
   pre-run rate-limit verification steps.

3. **Rerun checklist added**: See
   `docs/implementation-path/116-g36-monitoring-execution-plan.md` §13 for
   required per-adapter 2xx validation, readyz/deep threshold confirmation,
   metrics counter presence, queue depth snapshots, backup/restore status, and
   operator signoff.

---

## Next Actions

| Action | Owner | Notes |
|--------|-------|-------|
| Re-run workload generator with `trusted_context` fix | Engineering / Operator | Use latest `run_real_workload_generator.py`. Start in `--plan` mode to inspect payloads. |
| Verify rate-limit policy before live execution | Operator | Check `/v1/metrics` or server config for current rate-limit thresholds. |
| Confirm per-adapter 2xx responses in rerun | Engineering / Operator | Each adapter in the mix must return HTTP 200/201 for at least one request. |
| Validate readyz/deep ≥ 99% success during target phase | Engineering / Operator | Probe continuously, not just post-run. |
| Capture metrics snapshots and queue depth per phase | Engineering / Operator | Required for G3.6 acceptance per doc 116. |
| Confirm backup verify + restore drill within RTO | Operator | Required for A5 acceptance criterion. |
| Obtain operator signoff after all criteria met | Operator | Full acceptance, not conditional. |

---

*Artifact created: 2026-05-14. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
