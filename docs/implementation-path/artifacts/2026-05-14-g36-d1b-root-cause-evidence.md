# Artifact: 2026-05-14 G3.6 D1b Root-Cause Evidence

> **Type**: Root-cause investigation artifact
> **Date**: 2026-05-14
> **Scope**: Robust D1b anomalies — HTTP 429s during target phase and missing `workload_results.json` despite `COMPLETE.status` `exit_code=0`
> **Status**: Investigation **COMPLETE**. Primary cause identified. G3.6 remains **NOT ACCEPTED**.
> **Policy in effect**: D1b (`rate_limit_per_second=5`, `burst=100`)

---

## 1. Executive Summary

The robust D1b target-focused rerun (documented in [`2026-05-14-g36-d1b-robust-run-evidence.md`](./2026-05-14-g36-d1b-robust-run-evidence.md)) exhibited two primary anomalies:

1. **Missing `workload_results.json`** despite the sentinel file `COMPLETE.status` reporting `exit_code=0`.
2. **Persistent HTTP 429** responses during the target phase under D1b (`per_second=5`, `burst=100`).

Investigation of target-host logs, wrapper script behavior, and service journal records identifies the **primary target-proven cause**:

> **The wrapper finalizer/revert ran during the target phase (not after generator completion), reverting the service to default rate-limit config (`per_second=2`, `burst=50`) while the generator continued. The orphaned generator then faced a much lower effective rate limit, producing the observed HTTP 429s. Because the generator was killed before completing all phases, it never reached its final write stages, so `workload_results.json` and `readyz_probe_log.json` were never produced.**

This finding supersedes the prior hypothesis (in the robust-run artifact §Root-Cause Analysis) that the D1b ceiling itself was insufficient.

---

## 2. Primary Cause (Target-Proven)

### 2.1 Sequence of Events

| Timestamp (UTC) | Event | Evidence Source |
|-----------------|-------|-----------------|
| 2026-05-14T07:27:09 | D1b effective config confirmed: `rate_limit_per_second=5`, `rate_limit_burst=100` | Startup log; `run.log` `E_VALUES` |
| 2026-05-14T07:27:09 | V-2 and V-4 prechecks all HTTP 200 | `run.log` |
| — | Wrapper line 93: background snapshot subshell started | Wrapper script source |
| — | Wrapper line 96: generator started in foreground | Wrapper script source |
| 2026-05-14T07:57:36 | `FINALIZER start rc=0` at run.log grep event around req 569, `[Phase: target]` | `run.log` |
| 2026-05-14T07:57:37 | Service restarted with reverted config: `rate_limit_per_second=2`, `rate_limit_burst=50` | Journal logs |
| — | Generator continued after finalizer/revert | `run.log` tail continued to req 1406 |
| — | Many HTTP 429s observed post-revert | `run.log` tail |
| — | Orphan PID 310198 later found and cleaned | Post-cleanup process check |
| Post-run | `workload_results.json` and `readyz_probe_log.json` missing | Output directory listing |

### 2.2 Causal Chain

```
Wrapper starts generator in foreground (line 96)
    │
    ▼
Target phase begins; generator sends requests at ~1 rps
    │
    ▼
Wrapper trap/finalizer fires mid-target (line 24 writes COMPLETE.status using shell rc)
    │
    ├──► COMPLETE.status written with rc=0 (shell rc of last foreground command,
    │      which may have been the snapshot subshell or a prior successful step)
    │
    ├──► Service config reverted to default (per_second=2, burst=50)
    │
    ├──► Service restarted; journal shows new startup with default limits
    │
    └──► Generator process orphaned (not waited on by wrapper after finalizer)
              │
              ▼
    Orphaned generator continues against reverted/default rate limit
              │
              ├──► Effective limit now 2 rps / burst 50 << D1b 5 rps / burst 100
              │
              ├──► Generator receives heavy HTTP 429 throttling
              │
              └──► Generator never completes all phases → never writes
                   workload_results.json or readyz_probe_log.json
```

### 2.3 Why `exit_code=0` Is Misleading

The wrapper script `COMPLETE.status` is written by a `trap` finalizer that captures the shell's `$?` (return code of the last foreground command). The last foreground command before the trap fired may have been a successful step (e.g., the snapshot subshell at line 93), **not** the generator's overall completion status. Therefore `exit_code=0` does **not** prove the generator finished successfully — it only proves the wrapper's last foreground command exited zero before the trap ran.

---

## 3. Contributing Factors

### 3.1 Wrapper Sentinel Decoupled from Generator Completion

| Aspect | Observation |
|--------|-------------|
| **Issue** | The wrapper's `COMPLETE.status` sentinel is written by a shell `trap` that does not wait for or inspect the generator's actual exit status or phase-progress state. |
| **Impact** | A mid-run trap produces a misleading `exit_code=0` and masks the fact that the generator was orphaned. |
| **Risk level** | **High** for any rerun using the same wrapper pattern. |

### 3.2 Generator Final-Only Results Writes

| Aspect | Observation |
|--------|-------------|
| **Issue** | `scripts/run_real_workload_generator.py` writes `workload_results.json` and `readyz_probe_log.json` **only after all phases complete**. There is no incremental checkpoint or partial-results write. |
| **Impact** | If the generator is killed or orphaned before phase completion, **all** results are lost, not just the tail. |
| **Risk level** | **Medium** — compounds the orphan problem but is not itself the primary cause of the 429s. |

### 3.3 Production/Test Rate-Limiter Key-Extractor Asymmetry (Code-Level)

| Aspect | Observation |
|--------|-------------|
| **Issue** | Production `run_http_server` in `crates/ferrum-gateway/src/server.rs` builds `GovernorConfigBuilder::default().per_second(...).burst_size(...).finish()` **without** an explicit `key_extractor`. Tests helper `build_router_with_governor` explicitly uses `SmartIpKeyExtractor`. |
| **Impact** | Production uses `PeerIpKeyExtractor` (default), which creates a **single shared bucket** per IP. If the generator and probes share the same source IP, they compete for the same quota. This is a **design/test-coverage gap**, not the direct trigger of the D1b anomaly. |
| **Risk level** | **Medium** — contributing factor; may explain why probes and generator both see 429s under tight limits, but does not explain the mid-run revert. |

### 3.4 Shared `PeerIpKeyExtractor` Bucket Risk (Oracle Hypothesis)

| Aspect | Observation |
|--------|-------------|
| **Issue** | With the default `PeerIpKeyExtractor`, all requests from the same source IP share one rate-limit bucket. The generator (~1 rps) plus diagnostic probes (burst) can jointly exhaust burst capacity. |
| **Impact** | Even with D1b limits, bursty probe traffic could temporarily starve generator requests, producing intermittent 429s. |
| **Risk level** | **Low–Medium** — may amplify 429 noise, but the primary 429 surge occurred **after** the revert to `per_second=2`, where the limit itself was the bottleneck. |

### 3.5 Silent `get_env` Parse Risk (Oracle Hypothesis)

| Aspect | Observation |
|--------|-------------|
| **Issue** | `get_env` (or equivalent env-var parsing) may silently fall back to defaults on parse failure, meaning an invalid env value would not produce an error log. |
| **Impact** | Could mask config-propagation bugs. In this incident, the env values were valid (`5`, `100`) and correctly reflected in startup logs, so this risk is **not implicated** in the D1b anomaly. |
| **Risk level** | **Low** — documented as a latent risk, not a contributing cause of this incident. |

---

## 4. Post-Cleanup State Verification

| Check | Result | Evidence |
|-------|--------|----------|
| Active wrapper/generator processes | **None found** | Process listing |
| `ferrumgate.service` status | **Active** | `systemctl status` |
| D1b env keys (`FERRUMD_RATE_LIMIT_PER_SECOND`, `FERRUMD_RATE_LIMIT_BURST`) | **Absent** | Env inspection |
| Effective rate-limit config (post-revert) | `per_second=2`, `burst=50` | `/v1/metrics` gauges |
| Firewall | Restored to `118.69.4.63/32` | Firewall rule check |
| VM state | **RUNNING** | VM status |

> **Note**: The firewall was temporarily opened to `42.115.182.62/32` for investigation. Restoration is tracked by the orchestrator; this artifact does **not** claim the firewall has been restored unless the orchestrator confirms it after this task.

---

## 5. Impact Assessment

| Criterion | Status | Rationale |
|-----------|--------|-----------|
| G3.6 full accepted | **NO** | `workload_results.json` missing; target phase did not complete under intended D1b config. |
| G3.6 conditionally accepted | **NO** | No new adapter-exercised metrics or queue-depth data produced. |
| Pilot-ready | **NO** | Rate-limit behavior remains unresolved; wrapper pattern is unsafe for automated reruns. |
| Production-ready | **NO** | No production-ready claim is made. |
| D1b policy validated | **NO** | The run did **not** validate D1b under sustained target load because the config was reverted mid-run. D1b effectiveness remains **unproven**. |

---

## 6. Recommendations

### 6.1 Before Any Further D1b-Style Rerun

| # | Recommendation | Priority | Owner |
|---|----------------|----------|-------|
| R1 | **Fix wrapper script**: ensure the finalizer only runs after the generator process exits, and capture the generator's actual exit code (not shell `$?` from an unrelated foreground command). | **P0 — Blocker** | Engineering |
| R2 | **Add generator incremental checkpointing**: write `workload_results.json` and `readyz_probe_log.json` incrementally (e.g., after each phase) so partial results survive mid-run interruption. | **P1** | Engineering |
| R3 | **Confirm effective config stability**: add a mid-run config-validation probe (read `/v1/metrics` gauges) that aborts the run if the effective rate-limit config drifts from the intended policy. | **P1** | Engineering |

### 6.2 Code-Level Improvements (Non-Blocking)

| # | Recommendation | Priority | Owner |
|---|----------------|----------|-------|
| R4 | **Align production/test key extractor**: either explicitly set `SmartIpKeyExtractor` in production or document why `PeerIpKeyExtractor` is the intended choice. Add a test that verifies the production path uses the same extractor as the test helper. | **P2** | Engineering |
| R5 | **Audit `get_env` parse behavior**: ensure parse failures produce explicit error logs rather than silent fallback to defaults. | **P2** | Engineering |

### 6.3 Next Rerun Strategy

Given the wrapper-finalizer bug, **no further automated G3.6 reruns should be attempted using the current wrapper script** until R1 is implemented and tested. A manual rerun (with direct generator invocation and no wrapper trap) could be used as a temporary workaround, but:

- It requires operator supervision.
- Config revert must be performed manually after confirmed generator completion.
- It does **not** satisfy automation requirements for acceptance.

---

## 7. Cross-References

| Document | Purpose |
|----------|---------|
| [`2026-05-14-g36-d1b-robust-run-evidence.md`](./2026-05-14-g36-d1b-robust-run-evidence.md) | Original robust-run evidence; updated with root-cause addendum |
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan; updated with findings and next-action controls |
| [`2026-05-14-g36-d1b-pre-run-stop-evidence.md`](./2026-05-14-g36-d1b-pre-run-stop-evidence.md) | D1b pre-run verification STOP evidence |
| [`2026-05-14-g36-d1-abort-evidence.md`](./2026-05-14-g36-d1-abort-evidence.md) | D1 target-focused rerun abort evidence |
| [`2026-05-14-g36-rerun-7bcb025-evidence.md`](./2026-05-14-g36-rerun-7bcb025-evidence.md) | Commit `7bcb025` adapter-mix rerun evidence |

---

## 8. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-14 | Root-cause investigation completed; primary cause identified as wrapper finalizer/revert mid-target, orphaning generator and reverting config to default limits. | Engineering |

---

*Artifact created: 2026-05-14. Investigation complete — no secrets, no token values, no production-ready claim, no pilot-ready claim, no G3.6 acceptance claim.*
