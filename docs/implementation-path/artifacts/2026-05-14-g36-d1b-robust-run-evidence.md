# Artifact: 2026-05-14 G3.6 D1b Robust Target Rerun Evidence

> **Type**: Evidence artifact (execution results, not a readiness claim)
> **Date**: 2026-05-14
> **Scope**: G3.6 D1b robust target-focused rerun under delegated operator policy
> **Status**: **INCOMPLETE / NOT ACCEPTED**. G3.6 full accepted remains **NO**.
> **Associated commit**: `7bcb025` (same as prior rerun)
> **Policy in effect**: D1b (`rate_limit_per_second=5`, `burst=100`)

---

## Summary

This artifact documents the robust D1b target-focused rerun attempted on
2026-05-14. The workload generator executed through the planned phase sequence
(baseline → low → target → cooldown). The sentinel file (`COMPLETE.status`)
reports `exit_code=0`, but the primary structured output (`workload_results.json`)
is missing. Mid-run probes and the run log show that the target phase suffered
persistent HTTP 429 throttling across all adapters. Post-run cleanup identified
and removed an orphan workload process, reverted the D1b configuration, restored
the SSH firewall, and returned the host to its pre-test state. This run does
**not** satisfy the G3.6 acceptance criteria.

---

## Run Metadata

| Field | Value |
|-------|-------|
| Policy | D1b (`FERRUMD_RATE_LIMIT_PER_SECOND=5`, `FERRUMD_RATE_LIMIT_BURST=100`) |
| Remote output directory | `/tmp/ferrum-g36-workload-d1b-target-rerun-robust` |
| Sentinel file | `COMPLETE.status` present, `exit_code=0`, `finished_at=2026-05-14T07:57:40Z` |
| `workload_results.json` | **MISSING** |
| Service status at end (post-revert) | Active |
| `readyz/deep` post-revert (unauthenticated) | HTTP 401 (auth restored, not disabled) |
| VM state | RUNNING |
| SSH firewall post-cleanup | `118.69.4.63/32` only |
| Outcome | **INCOMPLETE / NOT ACCEPTED** |
| G3.6 full accepted | **NO** |

---

## Phase Execution

| Phase | Rate | Duration | Known Result |
|-------|------|----------|--------------|
| Baseline | 0 rps | 600 s | No anomalies reported |
| Low | 0.1 rps | 600 s | Passed (historical pattern; no explicit log retained) |
| Target | 1 rps | 1,800 s | Completed per sentinel, but **persistent HTTP 429** observed |
| Cooldown | 0 rps | 600 s | No anomalies reported |

> **Note**: Because `workload_results.json` is missing, per-phase request counts,
> adapter-specific 2xx rates, and sustained-write-rate statistics are
> **unavailable**. The sentinel alone is insufficient for acceptance.

---

## Mid-Run Probe Results

Probes were captured during the target phase.

| Probe | File | Sample Body | Interpretation |
|-------|------|-------------|----------------|
| `readyz` mid-run | `readyz_target_mid.json` | `Too Many Requests! Wait for 0s` | 429 returned; wait time nominal but still throttled |
| `metrics` mid-run | `metrics_target_mid.txt` | `Too Many Requests! Wait for 0s` | 429 returned; metrics endpoint blocked |

> **Interpretation**: Both probes returned HTTP 429 during the target phase.
> A wait of `0s` is below the explicit STOP threshold of `>0.3s`, but the
> **presence** of 429s at the 1 rps generator rate under D1b (5 rps / burst=100)
> indicates the rate limiter was still actively throttling. The D1b
> configuration did not provide enough headroom to eliminate 429s during
> sustained target load.

---

## Target-Phase HTTP 429 Sample (run.log tail)

Excerpts from `run.log` show repeated adapter-level HTTP 429 responses during
the target phase:

| Request | Adapter | HTTP Status |
|---------|---------|-------------|
| 1387 | fs | 429 |
| 1389 | fs | 429 |
| 1391 | http | 429 |
| 1393 | maildraft | 429 |
| 1396 | git | 429 |
| 1397 | maildraft | 429 |
| 1399 | sqlite | 429 |
| 1401 | fs | 429 |
| 1403 | maildraft | 429 |
| 1405 | git | 429 |

> **Observation**: All five adapters (FS, Git, HTTP, SQLite, Maildraft) were
> affected by HTTP 429 throttling during the target phase. The run did **not**
> achieve the >95% HTTP 200 requirement for target-load acceptance.

---

## Orphan Process & Cleanup

| Step | Detail |
|------|--------|
| Orphan PID detected | `310198` |
| Status after cleanup | PID no longer existed |
| Action taken | Orphan process cleaned up as part of post-run hygiene |

---

## Post-Run Recovery

| Step | Action | Result |
|------|--------|--------|
| 1 | Reverted `/etc/ferrumgate/env` from backup | Success |
| Backup link | `/etc/ferrumgate/env.g36-d1b3-backup-latest` → `/etc/ferrumgate/env.g36-d1b3-backup-20260514072709` | Confirmed |
| 2 | `ferrumgate.service` restarted | Active |
| 3 | Verified absence of rate-limit env vars post-revert | `FERRUMD_RATE_LIMIT_PER_SECOND` and `FERRUMD_RATE_LIMIT_BURST` absent | Confirmed |
| 4 | Unauthenticated `readyz/deep` probe post-revert | HTTP 401 | Auth restored (not disabled public health) |
| 5 | SSH firewall | Restored to `118.69.4.63/32` |
| 6 | VM state | Remains RUNNING |

> **SSH / Network Management API note**: A troubleshooting step attempted to
> enable the Network Management API. The service-enable command was initiated,
> but subsequent troubleshooting encountered a `SERVICE_DISABLED`
> propagation/permission message. This is noted for completeness; it does not
> affect the G3.6 acceptance conclusion.

---

## Root-Cause Analysis (Target Phase 429s)

> **Update 2026-05-14**: The investigation in
> [`2026-05-14-g36-d1b-root-cause-evidence.md`](./2026-05-14-g36-d1b-root-cause-evidence.md)
> has identified the **primary cause**. The hypotheses below are preserved for
> historical context; the "unresolved" conclusion is superseded.

| Hypothesis | Evidence | Likelihood |
|---|---|---|
| D1b effective rate limit still insufficient for 1 rps sustained + diagnostic probes | Mid-run probes and run.log show 429s despite `per_second=5`, `burst=100` | **Superseded** — 429s occurred **after** revert to `per_second=2` |
| Missing `workload_results.json` prevents quantitative confirmation | File absent from output directory | Confirmed — generator orphaned before final writes |
| Orphan process may have interfered with generator output or metrics | PID found and cleaned up; causal link now established | **Confirmed** — orphan was the generator continuing against reverted limits |
| Config propagation or layer-specific limit overrides D1b | Unauthenticated probes also 429; auth restored post-revert | **Superseded** — no layer override; config was explicitly reverted mid-run |

**Superseding finding** (from root-cause artifact): The wrapper finalizer/revert
ran **during** the target phase (not after generator completion), reverting the
service to default rate-limit config (`per_second=2`, `burst=50`) while the
generator continued as an orphan. The orphaned generator then faced the much
lower default limit, producing the observed HTTP 429s. Because the generator
never completed all phases, it never reached its final write stages, so
`workload_results.json` and `readyz_probe_log.json` were never produced.

The `exit_code=0` in `COMPLETE.status` is misleading: the wrapper trap captures
the shell return code of the last foreground command (not the generator's
overall completion), so `0` does not prove the generator finished successfully.

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- A target-focused workload generator run was attempted under D1b.
- The sentinel reports `exit_code=0` with a completion timestamp.
- Mid-run probes and logs confirm **persistent HTTP 429** during target phase.
- All five adapters experienced 429 throttling.
- Post-run cleanup and config revert were successful.
- Auth and firewall were restored to pre-test state.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| D1b policy eliminated rate-limit throttling at 1 rps | **NO** | 429s persisted across all adapters. |
| Target phase achieved >95% HTTP 200 | **NO** | `workload_results.json` missing; log shows heavy 429. |
| G3.6 full accepted | **NO** | Acceptance criteria A1–A6 not met. |
| G3.6 conditionally accepted | **NO** | No new adapter-exercised metrics or queue-depth data produced. |
| Production-ready | **NO** | No production-ready claim is made. |
| Pilot-ready | **NO** | Missing structured output and unresolved rate-limit blocker. |

---

## Next Options

| Option | Description | Trade-off | Recommendation |
|--------|-------------|-----------|----------------|
| **O1** — Raise rate-limit ceiling further (e.g., `per_second=10`, `burst=200`) | Test whether a higher ceiling eliminates 429s | May mask production constraints; requires revert | **Caution** — only if operator explicitly authorizes |
| **O2** — Investigate effective rate-limit layer | Use `ferrumgate_rate_limit_per_second`/`burst` metrics and startup logs to confirm effective values before next run | Requires engineering time; may reveal config propagation bug | **Superseded** by root-cause finding: effective limit was reverted mid-run, not misconfigured |
| **O3** — Accept operational ceiling as design baseline (D3) | Document that current effective ceiling is <1 rps for sustained adapter mix | Produces non-representative P5b data | **Not recommended** for G3.6 acceptance |
| **O4** — Defer G3.6 full acceptance | Acknowledge that rate-limit resolution is a prerequisite | Blocks P5b–P5e until resolved | **Acceptable** if operator chooses to wait |
| **O5** — Fix wrapper script and retry (NEW) | Address the wrapper finalizer bug (R1 in root-cause artifact) before any automated rerun | Requires engineering time; blocks automated reruns until fixed | **Recommended** — primary next action |

> **No further G3.6 target reruns should be attempted until the wrapper
> finalizer bug is fixed (R1) and the fix is tested.**
>
> **See also**: [`2026-05-14-g36-d1b-root-cause-evidence.md`](./2026-05-14-g36-d1b-root-cause-evidence.md)
> for the complete investigation, causal chain, and detailed recommendations.

---

*Artifact created: 2026-05-14. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
