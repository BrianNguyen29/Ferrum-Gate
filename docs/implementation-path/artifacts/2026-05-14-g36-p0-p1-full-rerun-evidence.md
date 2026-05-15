# Artifact: 2026-05-14 G3.6 P0+P1 Full-Duration Rerun Evidence

> **Type**: Full-duration target-host G3.6 rerun evidence artifact
> **Date**: 2026-05-14
> **Scope**: P0 monitoring route exemption + P1 `SmartIpKeyExtractor` / deterministic `x-real-ip` generator rerun under D1b test-window policy
> **Status**: **ENGINEERING WORKLOAD GATE PASSED; G3.6 CONDITIONAL ACCEPTANCE (delegated) FOR P5b PLANNING INPUT ONLY**. A5 fresh backup verify OK; A6 delegated signoff recorded. Full acceptance, pilot-ready, and production-ready NOT claimed.
> **Policy in effect during workload window**: D1b (`rate_limit_per_second=5`, `burst=100`) for test window only; post-run state reverted to default `2` / `50`.

---

## 1. Executive Summary

This artifact records the first successful full-duration target-host rerun after both rate-limit blocker mitigations were deployed:

- **P0**: monitoring endpoints (`/v1/metrics`, `/v1/readyz`, `/v1/readyz/deep`) are outside the workload `GovernorLayer`.
- **P1/C1**: production rate limiting uses `SmartIpKeyExtractor`, and the workload generator sends deterministic per-adapter `x-real-ip` headers.

| Aspect | Result |
|--------|--------|
| P1 short diagnostic | **PASS** — 298/298 HTTP 200, 0 HTTP 429 |
| Full rerun wrapper / generator completion | **PASS** — sentinel `exit_code=0`, `generator_exit_code=0`, `revert_exit_code=0` |
| Full rerun target-phase HTTP 429 rate | **PASS** — 0/1,792 target requests were HTTP 429 |
| All adapters exercised at target | **PASS** — FS, Git, HTTP, Maildraft, SQLite all returned HTTP 200 |
| Config drift log | **PASS** — no `config_drift_log.jsonl` produced |
| Post-run cleanup | **PASS** — no wrapper/generator processes; service active; config reverted to `2` / `50` |
| SSH firewall | **RESTORED** — `118.69.4.63/32` |
| A5 fresh backup verify | **PASS** — `ferrumctl backup verify` OK on target host |
| G3.6 conditional accepted | **YES — delegated** | Operator authority delegated to assistant on 2026-05-14 for P5b planning input only |
| G3.6 full accepted | **NO** | Full restore-to-production and removal of A-criteria caveats remain pending |
| Pilot-ready | **NO** |
| Production-ready | **NO** |

The previous D1b full rerun failed with ≈80.4% target-phase HTTP 429. After P0+P1, the full target-focused workload completed with **1,852 total HTTP 200 responses and 0 HTTP 429 responses**.

---

## 2. P1 Diagnostic Gate

Before the full-duration rerun, a short P1 diagnostic was executed on the target host.

| Parameter | Value |
|-----------|-------|
| Output directory | `/tmp/ferrum-g36-p1-diagnostic-20260514` |
| Rate limit | `5` / `100` |
| Phases | baseline 30s @ 0 rps, target 300s @ 1 rps, cooldown 30s @ 0 rps |
| Wrapper exit | `0` |
| Sentinel | `COMPLETE.status` |

### 2.1 Diagnostic Sentinel

```json
{
  "timestamp": "2026-05-14T15:34:56Z",
  "stage": "generator_completed",
  "exit_code": 0,
  "generator_exit_code": 0,
  "revert_exit_code": 0,
  "reason": "Generator exited successfully; Revert succeeded",
  "output_dir": "/tmp/ferrum-g36-p1-diagnostic-20260514"
}
```

### 2.2 Diagnostic Workload Results

| Phase | Requests | HTTP 200 | HTTP 429 | Notes |
|-------|----------|----------|----------|-------|
| Baseline | 0 | 0 | 0 | Idle |
| Target | 298 | 298 | 0 | **PASS** |
| Cooldown | 0 | 0 | 0 | Idle |

Target-phase adapter distribution:

| Adapter | HTTP 200 |
|---------|----------|
| FS | 57 |
| Git | 60 |
| HTTP | 59 |
| Maildraft | 68 |
| SQLite | 54 |

Diagnostic artifact checks:

| Check | Result |
|-------|--------|
| `config_drift_log.jsonl` | Absent |
| `wrapper_stdout.log` long hex64 token pattern | False |
| `generator_stdout.log` long hex64 token pattern | False |
| Post-run metrics endpoint | HTTP 200 |
| Post-run service | Active |

---

## 3. Full-Duration Rerun Configuration

| Parameter | Value |
|-----------|-------|
| Server URL | `http://127.0.0.1:19080` (target host) |
| Rate limit per second | 5 |
| Rate limit burst | 100 |
| Phases | baseline 600s @ 0 rps, low 600s @ 0.1 rps, target 1800s @ 1.0 rps, cooldown 600s @ 0 rps |
| Output directory | `/tmp/ferrum-g36-p1-full-acceptance-20260514` |
| Wrapper start | `2026-05-14T15:38:18Z` |
| Wrapper finish | `2026-05-14T16:39:03Z` |

---

## 4. Full-Duration Sentinel

```json
{
  "timestamp": "2026-05-14T16:39:03Z",
  "stage": "generator_completed",
  "exit_code": 0,
  "generator_exit_code": 0,
  "revert_exit_code": 0,
  "reason": "Generator exited successfully; Revert succeeded",
  "output_dir": "/tmp/ferrum-g36-p1-full-acceptance-20260514"
}
```

The wrapper exited cleanly and then reverted the test-window rate-limit configuration. No orphan wrapper/generator processes were present after the run.

---

## 5. Full-Duration Workload Results

### 5.1 Overall Summary

| Metric | Value |
|--------|-------|
| Total requests | 1,852 |
| HTTP 200 | 1,852 |
| HTTP 429 | 0 |
| Aborted | False |
| Latency p50 | 2.22 ms |
| Latency p95 | 3.01 ms |
| Latency p99 | 8.2626 ms |
| Latency max | 30.07 ms |

### 5.2 Per-Phase Breakdown

| Phase | Duration | Requests | HTTP 200 | HTTP 429 | Notes |
|-------|----------|----------|----------|----------|-------|
| Baseline | 600 s | 0 | 0 | 0 | Idle |
| Low | 600 s | 60 | 60 | 0 | Warm-up adapter mix passed |
| Target | 1,800 s | 1,792 | 1,792 | 0 | **Acceptance workload gate passed** |
| Cooldown | 600 s | 0 | 0 | 0 | Idle |

> Target-phase HTTP 429 threshold is ≤5%. Actual: 0 / 1,792 = **0%**.

### 5.3 Per-Adapter Breakdown

Low phase:

| Adapter | HTTP 200 |
|---------|----------|
| FS | 10 |
| Git | 11 |
| HTTP | 8 |
| Maildraft | 14 |
| SQLite | 17 |

Target phase:

| Adapter | HTTP 200 |
|---------|----------|
| FS | 351 |
| Git | 391 |
| HTTP | 330 |
| Maildraft | 370 |
| SQLite | 350 |

All configured adapter paths returned HTTP 200 during the target phase.

---

## 6. Supporting Evidence

### 6.1 Artifact Inventory

| Artifact | Present | Notes |
|----------|---------|-------|
| `workload_results.json` | Yes | Full generator output |
| `workload_results.md` | Yes | Human-readable generator output |
| `workload_plan.json` | Yes | Workload plan |
| `workload_plan.md` | Yes | Human-readable plan |
| `checkpoint_phase_000.json` | Yes | Baseline checkpoint |
| `checkpoint_phase_001.json` | Yes | Low checkpoint |
| `checkpoint_phase_002.json` | Yes | Target checkpoint |
| `checkpoint_phase_003.json` | Yes | Cooldown checkpoint |
| `readyz_probe_log.json` | Yes | File present; parsed probe array was empty |
| `readyz_probe_log.md` | Yes | Human-readable readyz probe log |
| `metrics_prerun.txt` | Yes | Pre-run metrics scrape |
| `config_drift_log.jsonl` | No | Absence means no drift events were recorded |
| `wrapper_stdout.log` | Yes | Sanitized |
| `wrapper_stderr.log` | Yes | Empty |
| `generator_stdout.log` | Yes | Sanitized |
| `generator_stderr.log` | Yes | Empty |
| `RUN_SUMMARY.txt` | Yes | Human-readable summary |
| `sentinel/COMPLETE.status` | Yes | Truthful completion sentinel |

### 6.2 Readyz Evidence Caveat

`readyz_probe_log.json` exists, but the parsed probe array contained `0` entries in the summary script. The generator stdout tail showed five post-run readyz probes, all HTTP 200:

```text
Probe 1/5: HTTP 200
Probe 2/5: HTTP 200
Probe 3/5: HTTP 200
Probe 4/5: HTTP 200
Probe 5/5: HTTP 200
```

This is sufficient as a post-run health observation, but **not** sufficient by itself to satisfy the full A3 combined-observation readyz/deep criterion without operator review of the probe artifact semantics.

### 6.3 Secret Handling

Target log scans for this run found no long hex64 token pattern in:

- `wrapper_stdout.log`
- `generator_stdout.log`
- `RUN_SUMMARY.txt`
- `workload_results.md`

Token values are not recorded in this artifact.

---

## 7. Post-Run State Verification

| Check | Result | Evidence |
|-------|--------|----------|
| Active wrapper / generator processes | None | Target process listing after run |
| `ferrumgate.service` status | Active | `systemctl is-active ferrumgate.service` |
| Effective rate-limit config | `per_second=2`, `burst=50` | `/v1/metrics` post-revert gauges |
| `/v1/readyz` | HTTP 200 | Target-local curl |
| `/v1/readyz/deep` | HTTP 200 | Target-local curl |
| SSH firewall | `118.69.4.63/32` | GCP firewall rule check |

Post-run metrics gauge excerpt:

```text
ferrumgate_rate_limit_per_second 2
ferrumgate_rate_limit_burst 50
```

---

## 8. A5 Fresh Backup Verify

On 2026-05-14, a fresh backup verify was executed on the target host as part of the A5 acceptance criterion.

| Parameter | Value |
|-----------|-------|
| Command | `sudo /opt/ferrumgate/ferrumctl backup verify --db-path /var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db` |
| Output | `OK` |
| Detail | `Database integrity check passed: /var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db` |
| Prior restore drill | 2026-05-11 — restored to temp path, `ferrumctl backup verify` passed on restored copy, temp path removed |

**Caveat**: A5 is accepted conditionally. The fresh backup verify passed and the prior May 12 safe temp-copy restore drill integrity_check was ok, but full restore-to-production remains deferred.

---

## 9. Impact Assessment

| Criterion | Status | Rationale |
|-----------|--------|-----------|
| P0 monitoring exemption | **Validated by evidence** | No drift log; metrics/readyz not throttled during full rerun; post-run endpoints HTTP 200 |
| P1 SmartIp / deterministic client IP behavior | **Validated by evidence** | All target-phase workload requests returned HTTP 200 with per-adapter buckets |
| Target-phase 429 threshold | **PASS** | 0% 429 at target phase |
| Adapter exercise | **PASS** | All adapters had HTTP 200 in low and target phases |
| G3.6 conditional accepted | **YES — delegated** | Operator authority delegated to assistant on 2026-05-14 for conditional acceptance only; A1–A6 met with caveats |
| G3.6 full accepted | **NO** | Full acceptance requires full restore-to-production drill and removal of A-criteria caveats |
| Conditional single-node SQLite pilot ready | **NO** | G3.6 remains conditional only; no pilot-ready claim |
| Production-ready | **NO** | This is target-host workload evidence only |
| PostgreSQL production deployment | **NO** | No PostgreSQL production deployment is implied |
| HA/multi-node | **NO** | Single-node SQLite evidence only |

---

## 10. Conservative Verdict

The P0+P1 full rerun resolves the previously observed target-phase rate-limit blocker for this target-focused workload. Engineering can now treat the **workload 429 gate** as passed for this rerun.

A5 fresh backup verify was executed on the target host and returned OK. A6 delegated conditional signoff was recorded on 2026-05-14. However, this artifact does **not** by itself grant full G3.6 acceptance because full restore-to-production remains deferred and A3 was accepted as a post-run proxy only.

Current conservative status:

- `G3.6 workload 429 gate`: **PASS for this P0+P1 rerun**.
- `G3.6 conditional accepted`: **YES — delegated for P5b planning input only** (2026-05-14).
- `G3.6 full accepted`: **NO** — full restore-to-production and removal of A-criteria caveats remain pending.
- `Conditional single-node SQLite pilot ready`: **NO**.
- `Production-ready`: **NO**.

---

## 10. Cross-References

| Document | Purpose |
|----------|---------|
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan and acceptance checklist |
| [`2026-05-14-g36-d1b-full-rerun-evidence.md`](./2026-05-14-g36-d1b-full-rerun-evidence.md) | Prior failed D1b full rerun (≈80.4% target-phase 429) |
| [`2026-05-14-g36-d1b-rehearsal-evidence.md`](./2026-05-14-g36-d1b-rehearsal-evidence.md) | Repository-owned wrapper rehearsal evidence |

---

## 11. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-14 | P0+P1 diagnostic and full-duration rerun evidence artifact created. Records 298/298 diagnostic HTTP 200, 1,852/1,852 full-rerun HTTP 200, 0 target-phase 429, clean revert, restored firewall, and conservative non-acceptance pending operator signoff / remaining A-criteria evidence. | Engineering |
| 2026-05-14 | A5 fresh backup verify added (`ferrumctl backup verify` OK on `ferrumgate_20260513_163232.db`). Status updated to G3.6 CONDITIONAL ACCEPTANCE (delegated) for P5b planning input only. A6 delegated signoff recorded. Full acceptance, pilot-ready, and production-ready remain NOT claimed. | Engineering |

---

*Artifact updated: 2026-05-14. No secrets, no token values, no production-ready claim, no pilot-ready claim. G3.6 conditional acceptance (delegated) recorded for P5b planning input only. Full acceptance remains pending full restore-to-production and removal of A-criteria caveats.*
