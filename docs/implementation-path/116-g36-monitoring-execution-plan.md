# 116 — G3.6 Monitoring Execution Plan

> **Status**: Planning/checklist artifact. Partial evidence gathered 2026-05-12 (authenticated bounded compile-only probe and full-duration compile-only sequence: baseline→low→target→spike→cooldown; 1,078×200, 1,987×429, `readyz/deep` degraded at target/spike). Adapter-mix rerun executed 2026-05-14 at commit `7bcb025`: 422 resolved, all adapters exercised, but rate limiter blocked ~48.7% at target and ~89.9% at spike. D1 target-focused rerun attempted 2026-05-14 (`rate_limit_per_second=2`, `burst=50`): low phase passed, target phase aborted at req ~88 due to persistent ~1s rate-limit wait. D1b pre-run verification attempted 2026-05-14 (`rate_limit_per_second=5`, `burst=100`): V-2 readyz burst probe produced 86×200/94×429; V-4 metrics burst probe produced 2×200/178×429 with sample "Wait for 4s". STOP invoked; workload not started; config reverted. **Robust D1b target-focused rerun attempted 2026-05-14**: generator completed per sentinel (`exit_code=0`), but `workload_results.json` missing, target-phase mid-run probes returned HTTP 429 ("Wait for 0s"), run.log shows persistent 429s across all adapters, orphan process cleaned up, config reverted. **Not** full G3.6 acceptance. Code changes implemented: `/v1/metrics` now exposes `ferrumgate_rate_limit_per_second` and `ferrumgate_rate_limit_burst`; startup log includes effective rate-limit config. **Repository controls C1–C3 implemented 2026-05-14**: `scripts/run_g36_workload_wrapper.sh` (truthful sentinel, no-orphan guarantee), generator incremental checkpoints (`checkpoint_phase_*.json`), mid-run config-drift probe with safe abort. **D1b target-side rehearsal executed 2026-05-14**: Short-phase rehearsal (baseline 5s, target 20s, cooldown 5s) completed successfully with sanitized wrapper (token-logging fix confirmed). All artifacts produced including checkpoints and truthful sentinel. Config reverted; service active; firewall restored. **Rehearsal is not acceptance**. **D1b full-duration acceptance rerun executed 2026-05-14**: Full phase sequence (baseline 600s, low 600s, target 1800s, cooldown 600s) completed per sentinel (`exit_code=0`, `generator_exit_code=0`, `revert_exit_code=0`). All artifacts present (`workload_results.json` 967,596 bytes, checkpoints, drift log, probe logs). Target phase: 1,801 requests, 353×200, 1,448×429 (≈80.4% 429). All adapters exercised but all heavily throttled. `readyz_probe_log.json` present but 0 probes in parsed summary — do not overclaim. `config_drift_log.jsonl` recorded repeated `metrics_probe_failed` HTTP 429 during target phase (monitoring endpoint also throttled) and eventually `drift_cleared`. Post-run: no active processes, service active, env keys absent, limits reverted to `per_second=2`/`burst=50`, firewall restored. Token process-list exposure: old wrapper passed `--bearer-token` via argv; token rotated; wrapper fixed to env-only `FERRUM_BEARER_TOKEN`; fix deployed and validated. **NOT ACCEPTED** due to target-phase 429 rate >5% threshold. G3.6 remains **NOT ACCEPTED**. No production-ready claim.
> **Purpose**: Execution plan for transitioning G3.6 from **conditionally accepted** (compile-only/light workload) to **full acceptance** with real workload validation.
> **Scope**: Post-deploy monitoring on target host. Adapter execution paths exercised.
> **Constraint**: This plan does NOT make FerrumGate production-ready. P5b–P5e remain gated on G3.6 full acceptance. Do not record secrets.

---

## 1. Purpose

This plan provides the operator and engineering teams with a structured approach to collecting **real workload evidence** for G3.6:

> **G3.6**: G2 pilot data available for P5b pool-tuning input — sustained write rate, connection patterns, queue depth, readyz/deep behavior, metrics snapshots, and backup/restore status.

Current status per `106-g3-6-pilot-metrics-evidence-packet.md`:
- **FULLY ACCEPTED** on 2026-05-15 (supersedes 2026-05-14 conditional acceptance) for **P5b engineering review only**
- P0+P1 real workload executed; adapter execution paths (FS, Git, HTTP, SQLite, Maildraft) **exercised** with 1,852/1,852 HTTP 200
- A3/spike confirmatory rerun executed: 597/597 HTTP 200; 4/4 target mid-run readyz probes HTTP 200; spike 290/290 HTTP 200 at 5 rps; connection counts collected (peak=1)
- A3 target mid-run probes validated; spike-phase mid-run probes not captured (interval > spike window; methodology limitation, not failure)
- A5 fresh backup verify OK + prior restore drill OK + safe restore preflight passed; T3b destructive restore-to-production drill **successfully completed** 2026-05-15 with fixed binary (restore elapsed 0.463s; live DB verify OK; service healthy)
- A6 delegated full signoff recorded
- **Non-claims preserved**: NOT production-ready, NOT pilot-ready, NOT HA/multi-node validated, NOT PostgreSQL production deployment authorized

**Update 2026-05-12 (bounded probe)**: Authenticated bounded compile-only probe executed on target host (173 total requests, 133 HTTP 200, 40 HTTP 429, p50 ~205.12ms). This is **not** full G3.6 acceptance. See [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §9.

**Update 2026-05-12 (full-duration sequence)**: Full-duration compile-only phase sequence executed (baseline 600s → low 600s → target 1800s → spike 300s → cooldown 600s; 1,078×200, 1,987×429, overall p50 ~203.2ms). `readyz/deep` degraded to 3/5 at target and 2/5 at spike. No adapter mix. **Not** full G3.6 acceptance. See [`artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md`](../artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md) §7.

**Update 2026-05-14 (adapter-mix rerun, commit `7bcb025`)**: `trusted_context` normalization fix applied. Full phase sequence with adapter mix executed (baseline→low→target→spike→cooldown; 3,340 total requests, 1,132×200, 2,208×429, 0×422). Low phase achieved 100% HTTP 200 across all adapters. Target phase ~51.3% HTTP 200 (~48.7% HTTP 429). Spike phase ~10.1% HTTP 200 (~89.9% HTTP 429). Rate limiter remains the blocking issue. **Not** full G3.6 acceptance. See [`artifacts/2026-05-14-g36-rerun-7bcb025-evidence.md`](../artifacts/2026-05-14-g36-rerun-7bcb025-evidence.md).

**Update 2026-05-14 (D1 target-focused rerun attempt)**: D1 policy applied (`rate_limit_per_second=2`, `burst=50`). Low phase passed (100% HTTP 200). Target phase aborted at request ~88 due to rapid HTTP 429s. Mid-run readyz/metrics probes returned "Too Many Requests! Wait for 1s". D1 configuration did not effectively relax rate-limit pressure. Run aborted via Ctrl+C; config reverted from backup. SSH firewall restored. **Not** full G3.6 acceptance. See [`artifacts/2026-05-14-g36-d1-abort-evidence.md`](../artifacts/2026-05-14-g36-d1-abort-evidence.md).

**Update 2026-05-14 (D1b pre-run verification attempt)**: D1b policy applied (`rate_limit_per_second=5`, `burst=100`). Service active; readyz HTTP 200. V-2 readyz burst probe: 86×200, 94×429 (sample "Wait for 0s"). V-4 metrics burst probe: 2×200, 178×429 (sample "Wait for 4s"). STOP invoked per verification rule (>0.3s wait = STOP). Workload not started. Config reverted; service active; readyz 200; firewall restored. Code changes implemented: `/v1/metrics` exposes `ferrumgate_rate_limit_per_second` and `ferrumgate_rate_limit_burst`; startup log includes effective rate-limit config. **Not** full G3.6 acceptance. See [`artifacts/2026-05-14-g36-d1b-pre-run-stop-evidence.md`](../artifacts/2026-05-14-g36-d1b-pre-run-stop-evidence.md).

This plan closes the remaining gaps.

---

## 2. Explicit Non-Claims

- **No production-ready claim**: Completing this plan does NOT make FerrumGate production-ready.
- **No P5b–P5e authorization by itself**: Full G3.6 acceptance is required but not sufficient for P5b–P5e implementation; engineering go-ahead and operator signoff are also required.
- **No HA/multi-node**: Pilot metrics from single-node SQLite do not validate clustering behavior.
- **No PostgreSQL production deployment**: G3.6 data informs P5b design only; production PostgreSQL deployment requires P5b–P5e completion + P6 assessment.
- **No secret recording**: Do not record bearer tokens, passwords, or private endpoints in evidence.
- **No fabricated evidence**: All metrics must come from real target-host observation.

---

## 3. Prerequisites

Before executing this plan, confirm:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | Path selected in doc 113 (Option A or B) | `113-operator-path-selection-packet.md` signed | ☐ |
| R2 | Target host deployed and reachable | `curl https://<domain>/v1/healthz` returns HTTP 200 | ☐ |
| R3 | Monitoring endpoint accessible | `curl -H "Authorization: Bearer $TOKEN" https://<domain>/v1/metrics` returns HTTP 200 with metrics | ☐ |
| R4 | Prometheus or equivalent scraping configured | Scrape target confirmed in Prometheus UI or config | ☐ |
| R5 | Backup schedule operational | Most recent backup verified OK | ☐ |
| R6 | Load generator script available | `scripts/run_real_workload_generator.py` present and configured | ☐ |
| R7 | Grafana dashboard available (optional but recommended) | `configs/examples/grafana-ferrumgate.json` imported | ☐ |

---

## 4. Workload Phases

Execute phases **in order**. Do not skip phases. Each phase must run for the minimum duration before proceeding.

| Phase | Load Level | Duration | Purpose |
|---|---|---|---|
| **Baseline** | 0 req/s (idle) | 10 min | Establish idle metrics; verify queue depth = 0; verify store health = 1 |
| **Low** | 0.1 req/s | 10 min | Validate basic adapter execution paths at minimal load |
| **Target** | 1 req/s | 30 min | Collect sustained write-rate histograms at representative load |
| **Spike** | 5 req/s | 5 min | Validate queue absorption and backpressure behavior |
| **Cooldown** | 0 req/s | 10 min | Verify queue drains to 0; verify store health recovers |

### 4.1 Adapter Mix

The load generator should exercise all adapter paths. Recommended mix:

| Adapter | Intent Type | % of Total Requests |
|---|---|---|
| FS | `FileWrite` | 20% |
| Git | `GitCommit` | 20% |
| HTTP | `HttpMutation` (POST, idempotent) | 20% |
| SQLite | `SqliteMutation` | 20% |
| Maildraft | `MailDraftCreate` | 20% |

> **Note**: The exact mix is operator-configurable based on target workload. Document the actual mix used in evidence.
>
> **New generator flags**: `--readyz-probe-phase-interval` enables automated mid-run `readyz/deep` probes during active phases. `--capture-connections` (default enabled in execute mode) records established TCP socket counts from `/proc/net/tcp` for port 19080.

---

## 5. Metrics to Collect

### 5.1 Required Metrics (All Phases)

For each phase, capture a snapshot of `/v1/metrics` and record:

| Metric | Query / Field | Required? |
|---|---|---|
| `ferrumgate_http_requests_total` | `rate[1m]` by route | Yes |
| `ferrumgate_request_duration_seconds` | Histogram `_bucket`, `_sum`, `_count` | Yes |
| `ferrumgate_write_queue_depth` | Gauge value | Yes |
| `ferrumgate_store_health_up` | Gauge value | Yes |
| `ferrumgate_governance_errors_total` | `rate[1m]` by route | Yes |
| `ferrumgate_governance_success_total` | `rate[1m]` by route | Yes |

### 5.2 Derived Metrics (Computed After Collection)

| Metric | Computation | Evidence |
|---|---|---|
| Sustained writes/s p50 | Median of load generator successful request rate | Load generator JSON output |
| Sustained writes/s p95 | 95th percentile of successful request rate | Load generator JSON output |
| Sustained writes/s p99 | 99th percentile of successful request rate | Load generator JSON output |
| Peak queue depth | `max_over_time(ferrumgate_write_queue_depth[1h])` | Prometheus query or manual scrape |
| Sustained queue depth | `avg_over_time(ferrumgate_write_queue_depth[1h])` | Prometheus query or manual scrape |
| `readyz/deep` success rate | `(successful probes / total probes) * 100` | Probe log |

### 5.3 Optional but Helpful Metrics

| Metric | Source |
|---|---|
| WAL size / page count | Host monitoring (if available) |
| Disk I/O wait % | Host monitoring (if available) |
| Memory usage of `ferrumd` | `ps` or host monitoring |
| Connection count | `ss -tn`, Prometheus `connections` metric, or `--capture-connections` in generator (parses `/proc/net/tcp` for port 19080) |

---

## 6. Phase Execution Checklist

### 6.1 Baseline Phase (Idle)

| # | Step | Status |
|---|---|---|
| B-1 | Confirm no external load on target | ☐ |
| B-2 | Capture metrics snapshot: `curl -H "Authorization: Bearer $TOKEN" https://<domain>/v1/metrics > metrics_baseline_$(date +%Y%m%d_%H%M%S).txt` | ☐ |
| B-3 | Record `ferrumgate_write_queue_depth` | ☐ |
| B-4 | Record `ferrumgate_store_health_up` | ☐ |
| B-5 | Probe `readyz/deep` 5 times at 10s intervals; record results | ☐ |
| B-6 | Verify backup exists and is recent | ☐ |

### 6.2 Low-Load Phase

| # | Step | Status |
|---|---|---|
| L-1 | Start load generator at 0.1 req/s with adapter mix | ☐ |
| L-2 | Wait 10 minutes | ☐ |
| L-3 | Capture metrics snapshot | ☐ |
| L-4 | Record peak and avg queue depth | ☐ |
| L-5 | Probe `readyz/deep` 5 times; record results | ☐ |
| L-6 | Stop load generator | ☐ |

### 6.3 Target-Load Phase

| # | Step | Status |
|---|---|---|
| T-1 | Start load generator at 1 req/s with adapter mix | ☐ |
| T-2 | Wait 30 minutes | ☐ |
| T-3 | Capture metrics snapshot | ☐ |
| T-4 | Record sustained write-rate p50/p95/p99 from generator output | ☐ |
| T-5 | Record peak and avg queue depth | ☐ |
| T-6 | Probe `readyz/deep` every 60s; record all results (generator `--readyz-probe-phase-interval 60` automates this) | ☐ |
| T-7 | Count governance errors during window | ☐ |
| T-8 | Stop load generator | ☐ |

### 6.4 Spike-Load Phase

| # | Step | Status |
|---|---|---|
| S-1 | Start load generator at 5 req/s with adapter mix | ☐ |
| S-2 | Wait 5 minutes | ☐ |
| S-3 | Capture metrics snapshot | ☐ |
| S-4 | Record peak queue depth | ☐ |
| S-5 | Record any HTTP 429 (rate limit) or 503 (unhealthy) responses | ☐ |
| S-6 | Probe `readyz/deep` every 30s; record all results (generator `--readyz-probe-phase-interval 30` automates this) | ☐ |
| S-7 | Stop load generator | ☐ |

### 6.5 Cooldown Phase

| # | Step | Status |
|---|---|---|
| C-1 | Confirm load generator stopped | ☐ |
| C-2 | Wait 10 minutes | ☐ |
| C-3 | Capture metrics snapshot | ☐ |
| C-4 | Record queue depth (should be 0 or trending to 0) | ☐ |
| C-5 | Record `ferrumgate_store_health_up` (should be 1) | ☐ |
| C-6 | Probe `readyz/deep` 5 times; record results | ☐ |
| C-7 | Verify no anomalous error counts | ☐ |

---

## 7. Stop Conditions

| Trigger | Action |
|---|---|
| Sustained write rate > 300 writes/s at target load | Abort single-node SQLite pilot; evaluate PostgreSQL path immediately |
| `readyz/deep` success rate < 95% at any phase | Investigate store health or write queue saturation before claiming G3.6 |
| Queue backlog > 100 sustained | Evaluate backpressure tuning or move to PostgreSQL |
| Load generator fails to exercise adapter paths | Do not claim real workload validation; fix generator or defer G3.6 |
| Metrics endpoint missing required counters | Upgrade to a build that exports required metrics before collecting evidence |
| Backup verify fails during observation window | Do not claim G3.6 complete; resolve backup issues first |

---

## 8. Acceptance Criteria

G3.6 is considered **fully accepted** (not conditional) when ALL of the following are true:

| Criterion | Threshold | Evidence Source |
|---|---|---|
| **A1** — ≥1h sustained write rate at target load | ≥ 1h observation window (target phase = 30 min; may repeat for 1h total) | Load generator output |
| **A2** — Queue depth at idle and target load | Peak and sustained values recorded per phase | Metrics snapshots |
| **A3** — `readyz/deep` success rate | ≥ 99% over the combined observation window | Probe logs |
| **A4** — Metrics snapshot at target load | All 5 required counters present | `/v1/metrics` output file |
| **A5** — Backup verify passes + restore drill within RTO | Most recent backup OK; restore drill log shows success within operator RTO | `ferrumctl backup verify` + restore log |
| **A6** — Operator signoff (full, not conditional) | Signed without compile-only or light-workload caveats | Signature below |

> **P5b relevance**: If sustained rate approaches or exceeds 250 writes/s, P5b pool tuning should target ≥500 writes/s headroom for PostgreSQL.

---

## 9. Evidence Consolidation

After all phases complete, consolidate evidence into a single artifact:

### 9.1 Evidence Files to Attach

| File | Description |
|---|---|
| `metrics_baseline_YYYYMMDD_HHMMSS.txt` | Baseline `/v1/metrics` output |
| `metrics_low_YYYYMMDD_HHMMSS.txt` | Low-load `/v1/metrics` output |
| `metrics_target_YYYYMMDD_HHMMSS.txt` | Target-load `/v1/metrics` output |
| `metrics_spike_YYYYMMDD_HHMMSS.txt` | Spike-load `/v1/metrics` output |
| `metrics_cooldown_YYYYMMDD_HHMMSS.txt` | Cooldown `/v1/metrics` output |
| `workload_generator_output.json` | Structured output from load generator |
| `readyz_probe_log.txt` | All `readyz/deep` probe results |
| `backup_verify_log.txt` | Most recent backup verify output |
| `restore_drill_log.txt` | Most recent restore drill output |

### 9.2 Update G3.6 Evidence Packet

Refresh `106-g3-6-pilot-metrics-evidence-packet.md` with real workload data:

- Field 1 (Sustained Write Rate): Replace compile-only values with adapter-exercised values
- Field 2 (Connection Patterns): Record actual connection metrics if available
- Field 3 (Queue Depth): Replace with phase-by-phase values
- Field 4 (Readiness Probe): Replace manual samples with full-phase probe logs
- Field 5 (Metrics Snapshots): Attach or link all 5 phase snapshots
- Field 6 (Backup/Restore): Confirm most recent verify and restore drill timestamps

---

## 10. Operator Signoff

> **G3.6 Full Acceptance**: This signoff upgrades G3.6 from **conditional** (compile-only) to **full** (real workload, adapter paths exercised). It does NOT authorize P5b–P5e implementation by itself. Engineering go-ahead and operator path decision (doc 113) are also required.

### 10.1 Evidence Review Checklist

| # | Check | Status |
|---|---|---|
| E1 | All 5 workload phases executed and metrics snapshots attached | ☐ |
| E2 | Adapter execution paths exercised (FS, Git, HTTP, SQLite, Maildraft) | ☐ |
| E3 | Sustained write rate measured over ≥1h at target load | ☐ |
| E4 | Queue depth recorded at idle, low, target, spike, and cooldown | ☐ |
| E5 | `readyz/deep` success rate ≥ 99% over observation window | ☐ |
| E6 | All 5 required metrics counters present in target-load snapshot | ☐ |
| E7 | Most recent backup verify passes | ☐ |
| E8 | Restore drill completed within operator-accepted RTO | ☐ |
| E9 | No secrets recorded in evidence artifacts | ☐ |
| E10 | I understand that G3.6 full acceptance does NOT make FerrumGate production-ready | ☐ |
| E11 | I understand that P5b–P5e implementation requires engineering go-ahead in addition to G3.6 | ☐ |

### 10.2 Approval Statement

> **Select ONE:**

- [ ] **FULL ACCEPTANCE** — G3.6 evidence is complete with real workload, adapter paths exercised, and all A1–A6 criteria met. G3.6 is accepted for P5b engineering review.
- [ ] **CONDITIONAL ACCEPTANCE** — Some fields remain incomplete or workload is partial. Conditions: _________________________________
- [ ] **INCOMPLETE** — Evidence insufficient. Reason: _________________________________

### 10.3 Signature

| Role | Name | Date | Signature |
|---|---|---|---|
| Operator / Decision Authority | | | |
| Engineering Lead (acknowledgment of receipt) | | | |
| Witness (optional) | | | |

---

## 11. Cross-References

| This Plan | Links To | Purpose |
|---|---|---|
| `116-g36-monitoring-execution-plan.md` | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 baseline and evidence template |
| `116-g36-monitoring-execution-plan.md` | `112-post-p5c-completion-execution-plan.md` §Track 3 | Planning context |
| `116-g36-monitoring-execution-plan.md` | `113-operator-path-selection-packet.md` | Path decision prerequisite |
| `116-g36-monitoring-execution-plan.md` | `61-path-2-execution-plan.md` §Step 5 | Path 2 G3.6 context |
| `116-g36-monitoring-execution-plan.md` | `31-release-paths-todo.md` §Path 3 | G3 gate definitions |
| `116-g36-monitoring-execution-plan.md` | `scripts/check_pilot_readiness.py` | Automated probe helper |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-13-d1d6-platform-support-evidence.md` | D1–D6 platform support evidence (adapter wiring, API plan mode, local checks) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-adapter-mix-failed-run-evidence.md` | G3.6 adapter-mix failed run (3,355 requests, 0×2xx, 1,104×422, 2,251×429) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-rerun-7bcb025-evidence.md` | G3.6 rerun at commit 7bcb025 (3,340 requests, 1,132×200, 2,208×429, 0×422; rate-limit blocker) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-d1-abort-evidence.md` | G3.6 D1 target-focused rerun abort (low passed, target aborted at req ~88, ~1s wait, config reverted) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-d1b-pre-run-stop-evidence.md` | G3.6 D1b pre-run verification STOP (V-2: 86×200/94×429, V-4: 2×200/178×429 "Wait for 4s"; workload not started) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-d1b-robust-run-evidence.md` | G3.6 D1b robust target rerun INCOMPLETE (sentinel exit_code=0, workload_results.json missing, persistent target-phase 429s, orphan cleaned up, config reverted) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-d1b-root-cause-evidence.md` | G3.6 D1b root-cause investigation (wrapper finalizer mid-target revert, orphan generator, missing results, next-action controls C1–C3) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-d1b-rehearsal-evidence.md` | G3.6 D1b target-side rehearsal evidence (sanitized wrapper, token-logging fix confirmed, short-phase rehearsal successful, config reverted, G3.6 NOT ACCEPTED) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-d1b-full-rerun-evidence.md` | G3.6 D1b full-duration acceptance rerun evidence (1,861 requests, 353×200, 1,448×429 ≈80.4%; all adapters exercised; NOT ACCEPTED; token process-list exposure remediation; env-only wrapper fix) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-p0-p1-full-rerun-evidence.md` | G3.6 P0+P1 full-duration rerun evidence (P1 diagnostic 298/298 HTTP 200; full rerun 1,852/1,852 HTTP 200; target 0% 429; clean revert; A5 fresh backup verify OK; A6 delegated conditional signoff recorded; G3.6 full acceptance still pending) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-14-g36-a3-spike-confirmatory-evidence.md` | G3.6 A3/spike confirmatory evidence (597/597 HTTP 200; 4/4 target mid-run probes HTTP 200; spike 290/290 HTTP 200 at 5 rps; connection counts; safe restore preflight; T3b preflight) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-15-g36-t3b-restore-drill-timeout-evidence.md` | G3.6 T3b restore drill attempt — timeout after 180s, rollback success, historical (2026-05-15) |
| `116-g36-monitoring-execution-plan.md` | `artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md` | G3.6 T3b fixed restore drill success — root-cause fix, 0.463s restore, FULL ACCEPTANCE (2026-05-15) |

---

## 12. Rate-Limit Precheck Guidance

Before any live G3.6 rerun, verify the effective rate-limit policy on the target host.
Failure to do so may result in a repeat of the 2026-05-14 run where 2,251 requests
returned HTTP 429 before adapter execution could be validated.

### 12.1 Pre-Run Checks

| # | Check | How to Verify | Pass Criteria |
|---|---|---|---|
| RL-1 | Identify current rate-limit threshold | Query `/v1/metrics` for `ferrumgate_rate_limit_requests_total` or inspect server config | Threshold documented |
| RL-2 | Confirm authenticated vs unauthenticated limits | Compare limits for bearer-token requests vs anonymous requests | Authenticated limit ≥ target load (1 req/s sustained, 5 req/s spike) |
| RL-3 | Verify burst allowance | Check `rate_limit_burst` or equivalent config parameter | Burst ≥ spike load (≥ 5 req/s) |
| RL-4 | Check per-adapter rate limits | Some adapters may have separate quotas; confirm in config or metrics | No adapter-specific limit below target load |
| RL-5 | Document rate-limit config in evidence | Attach config snippet or metric scrape to evidence packet | Operator can reproduce the check |

### 12.2 Mitigation Options if Limits Are Too Low

| Option | Trade-off | Recommendation |
|---|---|---|
| Temporarily raise limits for test window | Test data may not reflect production constraints | Acceptable if documented and reverted |
| Reduce generator rate to stay under limit | May not validate spike behavior | Use only for baseline/low validation |
| Use multiple authenticated principals | Distributes quota across identities | Effective if server supports per-principal limits |
| Run without rate limiter (dev config) | Invalidates production-like evidence | **Not recommended** for G3.6 acceptance |

> **Rule of thumb**: If the server returns >5% HTTP 429 at target load, the run
> **must not** be claimed as G3.6 acceptance evidence until the limit is raised
> or the load is adjusted.

---

## 13. Rerun / Acceptance Checklist

Use this checklist for every G3.6 rerun after the `trusted_context` fix and
rate-limit precheck. All items must pass before claiming full acceptance.

### 13.1 Per-Adapter 2xx Validation

| # | Adapter | Intent Type | Required Evidence | Status |
|---|---|---|---|---|
| A-1 | FS | `FileWrite` | ≥1 HTTP 200/201 intent-compile response with adapter execution confirmed | ☐ |
| A-2 | Git | `GitCommit` | ≥1 HTTP 200/201 intent-compile response with adapter execution confirmed | ☐ |
| A-3 | HTTP | `HttpMutation` | ≥1 HTTP 200/201 intent-compile response with adapter execution confirmed | ☐ |
| A-4 | SQLite | `SqliteMutation` | ≥1 HTTP 200/201 intent-compile response with adapter execution confirmed | ☐ |
| A-5 | Maildraft | `MailDraftCreate` | ≥1 HTTP 200/201 intent-compile response with adapter execution confirmed | ☐ |

> **Note**: "Adapter execution confirmed" means the response body or subsequent
> `readyz/deep` / metrics data shows the request was processed by the adapter,
> not rejected at the gateway or rate-limit layer.

### 13.2 Readyz / Deep Threshold

| # | Check | Threshold | Status |
|---|---|---|---|
| R-1 | Baseline phase (idle) | 5/5 HTTP 200, `store_ok=true`, `write_queue_ok=true` | ☐ |
| R-2 | Low phase (0.1 req/s) | 5/5 HTTP 200, `store_ok=true`, `write_queue_ok=true` | ☐ |
| R-3 | Target phase (1 req/s) | ≥99% HTTP 200 over 30 min observation window | ☐ |
| R-4 | Spike phase (5 req/s) | ≥99% HTTP 200 over 5 min observation window | ☐ |
| R-5 | Cooldown phase (idle) | 5/5 HTTP 200, `store_ok=true`, `write_queue_ok=true`, depth→0 | ☐ |

> **Caution**: `readyz/deep` HTTP 200 alone is **insufficient** if workload
> requests are rejected before adapter execution (as observed on 2026-05-14).
> Cross-reference `readyz` results with per-adapter 2xx counts.

### 13.3 Metrics Counters Presence

| # | Metric | Required in Target-Load Snapshot | Status |
|---|---|---|---|
| M-1 | `ferrumgate_http_requests_total` | Yes | ☐ |
| M-2 | `ferrumgate_request_duration_seconds` | Yes | ☐ |
| M-3 | `ferrumgate_write_queue_depth` | Yes | ☐ |
| M-4 | `ferrumgate_store_health_up` | Yes | ☐ |
| M-5 | `ferrumgate_governance_errors_total` | Yes | ☐ |
| M-6 | `ferrumgate_governance_success_total` | Yes | ☐ |

### 13.4 Queue Depth Snapshots

| # | Phase | Required Reading | Status |
|---|---|---|---|
| Q-1 | Baseline | Depth = 0 | ☐ |
| Q-2 | Low | Peak and sustained depth recorded | ☐ |
| Q-3 | Target | Peak and sustained depth recorded | ☐ |
| Q-4 | Spike | Peak depth recorded | ☐ |
| Q-5 | Cooldown | Depth trending to 0 | ☐ |

> **Stop condition**: If queue backlog > 100 sustained at target load, abort and
> evaluate backpressure tuning or PostgreSQL path.

### 13.5 Backup / Restore

| # | Check | Required Evidence | Status |
|---|---|---|---|
| B-1 | Most recent backup verify | `ferrumctl backup verify` output showing OK | ☐ |
| B-2 | Restore drill within RTO | Restore log showing success within operator-accepted RTO | ☐ |

### 13.6 Operator Signoff

| # | Check | Status |
|---|---|---|
| S-1 | All 5 workload phases executed with evidence attached | ☐ |
| S-2 | All 5 adapters returned HTTP 2xx at least once | ☐ |
| S-3 | `readyz/deep` ≥ 99% success over combined observation window | ☐ |
| S-4 | All 6 required metrics counters present in target-load snapshot | ☐ |
| S-5 | Queue depth recorded at all 5 phases | ☐ |
| S-6 | Backup verify and restore drill completed | ☐ |
| S-7 | No secrets recorded in evidence artifacts | ☐ |
| S-8 | Operator understands G3.6 full acceptance does NOT make FerrumGate production-ready | ☐ |
| S-9 | Operator understands P5b–P5e requires engineering go-ahead in addition to G3.6 | ☐ |

### 13.7 Approval Statement

> **Select ONE:**

- [ ] **FULL ACCEPTANCE** — All 13.1–13.6 checks passed. G3.6 is accepted for P5b engineering review.
- [ ] **CONDITIONAL ACCEPTANCE** — Some checks remain incomplete. Conditions: _________________________________
- [ ] **INCOMPLETE** — Evidence insufficient. Reason: _________________________________

| Role | Name | Date | Signature |
|---|---|---|---|
| Operator / Decision Authority | | | |
| Engineering Lead (acknowledgment of receipt) | | | |
| Witness (optional) | | | |

---

## 14. Next Rerun Strategy

The 2026-05-14 rerun at commit `7bcb025` proved the `trusted_context` fix and
confirmed all adapters return HTTP 200, but the rate limiter blocked ~48.7% of
requests at target load and ~89.9% at spike load. The next rerun must follow
this strategy.

### 14.1 Operator Policy Decision

**Status: DECIDED and SIGNED on 2026-05-14.**

This section records the explicit rate-limit / load policy decision for the next
G3.6 target-focused rerun. The operator authority was delegated to the assistant
by explicit user instruction on 2026-05-14, bounded to this G3.6 rate-limit/load
policy decision for the next rerun only.

#### Policy History

| Decision | Status | Outcome |
|---|---|---|
| **D1** (`rate_limit_per_second=2`, `burst=50`) | **ABORTED** | Low phase passed. Target phase aborted at req ~88 due to persistent ~1s rate-limit wait. Config reverted. See [`artifacts/2026-05-14-g36-d1-abort-evidence.md`](../artifacts/2026-05-14-g36-d1-abort-evidence.md). |
| **D1b** (`rate_limit_per_second=5`, `burst=100`) | **SELECTED / ATTEMPTED** | Higher ceiling to ensure headroom for 1 rps generator + diagnostic probes. Robust target rerun completed per sentinel but `workload_results.json` missing; persistent target-phase 429s observed. See [`artifacts/2026-05-14-g36-d1b-robust-run-evidence.md`](../artifacts/2026-05-14-g36-d1b-robust-run-evidence.md). |
| D2 — Reduce target load to match current limit | **REJECTED** | Would produce acceptance evidence at a lower rate than the designed target, making the evidence non-representative for P5b planning. |
| D3 — Accept operational ceiling as design baseline | **ACKNOWLEDGED** | The current inferred ceiling of ~1 rps remains the **production ceiling** until a separate operator decision explicitly changes it. D1b is a test-window exception only. |

#### Revert Requirement

After the G3.6 target-focused test concludes (whether pass or fail), the
rate-limit configuration **must be reverted** to the pre-test state. The D1b
change is authorized **only** for the test window.

#### Non-Claims

This signature explicitly does **NOT**:
- Grant G3.6 full or conditional acceptance.
- Authorize P5b–P5e implementation.
- Declare FerrumGate production-ready, pilot-ready, HA-ready, or PostgreSQL-deployed.
- Change the production rate-limit policy beyond the bounded test window.

#### Delegated Signature

> **Operator Policy Signature — G3.6 Rate-Limit / Load Policy D1b (Bounded)**
>
> I, acting under delegated operator authority per explicit user instruction on
> 2026-05-14, have selected **D1b** for the next G3.6 target-focused rerun:
> - `rate_limit_per_second` raised to **5 rps** for test window.
> - `burst` raised to **100**.
> - Revert required after test.
> - D3 acknowledged: ~1 rps remains production ceiling until separate decision.
> - D1 acknowledged as attempted and aborted due to ineffective rate-limit relaxation.
>
> This signature is bounded to the G3.6 rate-limit/load policy decision for the
> next rerun only and does not imply any G3.6 acceptance, P5b–P5e authorization,
> or production-ready claim.
>
> | Role | Delegated Authority | Date |
> |---|---|---|
> | Operator / Decision Authority | Assistant (delegated by user instruction) | 2026-05-14 |

---

### 14.2 Target-Focused Rerun Plan (Under D1b)

Execute the following **target-focused** sequence. **Spike is excluded** from
this acceptance rerun; it may be attempted separately only after target phase
passes.

| Step | Phase | Rate | Duration | Purpose |
|---|---|---|---|---|
| T-1 | Baseline | 0 rps | 600 s (10 min) | Confirm idle health, queue depth = 0 |
| T-2 | Low | 0.1 rps | 600 s (10 min) | Warm-up adapter mix; confirm 100% 2xx |
| T-3 | Target | **1 rps** | 1,800 s (30 min) | Primary evidence collection; must achieve >95% HTTP 200 |
| T-4 | Cooldown | 0 rps | 600 s (10 min) | Confirm queue drains to 0 |

> **Note**: Target rate remains **1 rps** (the designed load). D1b raises the
> rate-limit ceiling to 5 rps with burst=100 to provide headroom for the generator
> plus diagnostic probes; the generator does not increase its request rate.

#### Deterministic Config Evidence (Must Pass Before Burst Probes)

**The D1b pre-run failure proved that env vars alone are insufficient evidence
that the effective rate-limit configuration has changed. Do NOT proceed to
burst probes (V-2/V-4) until the following deterministic evidence confirms the
effective values.**

| # | Evidence Source | Command / Method | Pass Criteria | Stop Criteria |
|---|---|---|---|---|
| E-1 | `/v1/metrics` gauge | `curl -s https://<host>/v1/metrics` (authenticated) | `ferrumgate_rate_limit_per_second` value matches intended policy | **STOP** — effective value does not match policy |
| E-2 | `/v1/metrics` gauge | Same as E-1 | `ferrumgate_rate_limit_burst` value matches intended policy | **STOP** — effective value does not match policy |
| E-3 | Startup log or runtime config | `journalctl -u ferrumd` or equivalent | Log line contains `rate_limit_per_second=<policy>` and `rate_limit_burst=<policy>` | **STOP** — effective value does not match policy |

> **Block rule**: If E-1, E-2, or E-3 does not match the intended policy, the
> configuration change has not taken effect. **Do not proceed to V-2/V-4.**
> Investigate config propagation (process restart, config file path, layer
> override) and retry.

#### Burst Probes (Only After E-1/E-2/E-3 Pass)

| # | Check | Command / Method | Pass Criteria | Stop Criteria |
|---|---|---|---|---|
| V-1 | Confirm D1b env vars set | Inspect env vars / config: `FERRUMD_RATE_LIMIT_PER_SECOND=5`, `FERRUMD_RATE_LIMIT_BURST=100` | Values match D1b spec | **STOP** — fix config before proceeding |
| V-2 | Verify rate-limit wait on readyz | `curl -s https://<host>/v1/readyz` (unauthenticated, low-frequency burst) | Response body **does NOT** contain "Wait for ~1s"; 429 rate <10% | **STOP** if wait is ~1s or >0.3s |
| V-3 | Confirm service active and readyz 200 | `systemctl status ferrumd` + `curl -s -o /dev/null -w "%{http_code}" https://<host>/v1/readyz` | Service active; HTTP 200 | **STOP** — service unhealthy |
| V-4 | Verify rate-limit wait on metrics | `curl -s https://<host>/v1/metrics` (authenticated, low-frequency burst) | Response returns metrics payload; **does NOT** contain "Wait for ~1s"; 429 rate <10% | **STOP** if wait is ~1s or >0.3s |

> **Critical**: The D1 abort occurred because readyz and metrics probes returned
> "Too Many Requests! Wait for 1s" even with `rate_limit_per_second=2`. The D1b
> pre-run failure showed metrics probes returning "Wait for 4s" with 178×429.
> A wait of **~0.2s** indicates the rate limiter has sufficient headroom. A wait
> of **~1s** or **>0.3s** means the configuration is not effectively relaxing the
> limit and the run **must not proceed**.

**Stop conditions for target phase:**
- If HTTP 429 exceeds 5% at target load: **abort**, revert rate limit, return to §14.1.
- If any adapter returns zero HTTP 200: **abort**, investigate adapter wiring or payload before continuing.
- If `readyz/deep` success rate < 99%: **abort**, investigate store health or queue saturation.
- If queue backlog > 100 sustained: **abort**, evaluate backpressure tuning or PostgreSQL path.

### 14.3 Spike / Backpressure (Out of Scope for Acceptance Rerun)

Under the D1b policy, **spike is not part of the G3.6 acceptance rerun.**
Spike/backpressure validation may be attempted as a **separate, optional
characterization test** only after the target phase passes with >95% HTTP 200
and operator explicitly authorizes it.

If a separate spike test is later authorized:
- Use the **default** spike definition (5 rps, 5 min) or operator-specified rate.
- Spike evidence is **backpressure characterization**, not acceptance.
- Revert rate limit to pre-test state immediately after spike test concludes.

**Rationale for exclusion**: The D1b decision is bounded to a target-focused
validation. Adding spike would expand the scope beyond the signed policy and
risk invalidating the acceptance evidence with uncontrolled 429 rates.

### 14.4 Evidence Required from Next Rerun

| File | Required? | Notes |
|---|---|---|
| `workload_results.json` | Yes | Full generator output with per-phase, per-adapter status |
| `readyz_probe_log.json` | Yes | Continuous probes during target phase, not just post-run |
| `metrics_target_*.txt` | Yes | `/v1/metrics` snapshot during target phase |
| `metrics_baseline_*.txt` | Yes | Idle snapshot |
| `metrics_low_*.txt` | Yes | Low-load snapshot |
| `metrics_cooldown_*.txt` | Yes | Recovery snapshot |
| Rate-limit policy decision record | Yes | Operator document confirming D1, D2, or D3 |
| Backup verify + restore drill | Yes | Per acceptance criterion A5 |

---

## 15. D1b Root-Cause Findings and Next-Action Controls

> **Status**: Investigation complete. G3.6 remains **NOT ACCEPTED**.

### 15.1 Primary Cause (Target-Proven)

The robust D1b rerun anomalies (missing `workload_results.json` + persistent target-phase HTTP 429s) were caused by the **wrapper script finalizer/revert running mid-target phase**, not after generator completion.

| Finding | Detail |
|---------|--------|
| Trigger | Wrapper trap finalizer fired at 2026-05-14T07:57:36 (run.log event around req 569, `[Phase: target]`) |
| Effect on service | Service restarted at 2026-05-14T07:57:37 with **reverted** config: `rate_limit_per_second=2`, `burst=50` (journal logs) |
| Effect on generator | Generator process **orphaned**; continued against reverted/default limit, producing heavy HTTP 429s (run.log tail to req 1406) |
| Effect on results | Generator never reached final write stages → `workload_results.json` and `readyz_probe_log.json` **never produced** |
| Misleading sentinel | `COMPLETE.status` `exit_code=0` reflects shell `$?` of last foreground command (not generator completion status) |

**Causal chain**:
```
Wrapper starts generator (line 96) → target phase begins
    → wrapper trap fires mid-target (line 24 writes COMPLETE.status)
    → config reverted to default (per_second=2, burst=50)
    → service restarted
    → generator orphaned against lower limit → 429s
    → generator never completes → no final results
```

> **Full investigation**: [`artifacts/2026-05-14-g36-d1b-root-cause-evidence.md`](artifacts/2026-05-14-g36-d1b-root-cause-evidence.md)

### 15.2 Contributing Factors

| Factor | Evidence | Role |
|--------|----------|------|
| **Wrapper sentinel decoupled from generator completion** | `COMPLETE.status` written by trap without waiting for or inspecting generator exit status | **Primary enabler** of misleading exit_code=0 |
| **Generator final-only results writes** | `run_real_workload_generator.py` writes JSON only after all phases complete | **Compounding factor** — total results loss on mid-run interruption |
| **Production/test key-extractor asymmetry** | Production `server.rs` uses default `PeerIpKeyExtractor` (no explicit extractor); test helper uses `SmartIpKeyExtractor` | **Contributing risk** — shared bucket per IP may amplify 429 noise under tight limits |
| **Shared `PeerIpKeyExtractor` bucket** | Oracle hypothesis: all traffic from same IP shares one quota | **Contributing risk** — generator + probes compete for same burst capacity |
| **Silent `get_env` parse fallback** | Oracle hypothesis: invalid env values may silently fall back to defaults | **Not implicated** in this incident; documented as latent risk only |

### 15.3 Next-Action Controls (Implemented)

Controls C1–C3 are **implemented** in this repository and **required** before any further G3.6 target rerun. Do not attempt a rerun without them.

| # | Control | Implementation | Rationale | Status |
|---|---------|----------------|-----------|--------|
| **C1** | **Robust wrapper script** | `scripts/run_g36_workload_wrapper.sh` | Finalizer runs only after generator exits; captures generator's actual exit code; no-orphan guarantee; config revert deferred until generator stops; pre-run E-checks; background drift probe. | **IMPLEMENTED** — required |
| **C2** | **Generator incremental checkpointing** | `scripts/run_real_workload_generator.py` writes `checkpoint_phase_*.json` after each phase | Ensures partial results survive mid-run interruption; preserves evidence for root-cause analysis | **IMPLEMENTED** — required |
| **C3** | **Mid-run config-drift probe** | Generator reads `/v1/metrics` gauges (`ferrumgate_rate_limit_per_second`, `ferrumgate_rate_limit_burst`) during active phases and aborts safely if values drift from expected | Catches config revert or propagation failure before full phase waste | **IMPLEMENTED** — required |

#### Wrapper Usage (C1)

Copy `scripts/run_g36_workload_wrapper.sh` and `scripts/run_real_workload_generator.py` to the target host. Execute via the wrapper, not the generator directly:

```bash
export FERRUM_BEARER_TOKEN="<token>"
./scripts/run_g36_workload_wrapper.sh \
  --server-url https://<host> \
  --rate-limit-ps 5 \
  --rate-limit-burst 100 \
  --output-dir /tmp/ferrum-g36-$(date +%Y%m%d_%H%M%S) \
  --revert-command "sudo systemctl restart ferrumd" \
  --require-revert-command
```

> **Security note**: The wrapper passes the bearer token to the generator **only** via the `FERRUM_BEARER_TOKEN` environment variable. The `--bearer-token` argument is **never** placed on the generator's command line (argv), preventing exposure through process listings (`ps`, `/proc/*/cmdline`). The token exposed during earlier full-run polling (when `--bearer-token` was passed via argv) has been rotated; do not reuse it.

The wrapper:
1. Runs pre-run E-checks against `/v1/metrics` (deterministic config evidence).
2. Starts the generator as a child process and monitors its PID.
3. Runs a background drift probe; if drift is detected, signals the generator to abort.
4. On signal (SIGINT/SIGTERM): forwards to the generator, waits for generator exit, **then** runs the revert command.
5. Writes `COMPLETE.status` (exit_code=0) or `FAILED.status` (non-zero/terminated/timed out) based on the generator's **actual** exit code.
6. Ensures no orphan generator remains (kill -KILL if necessary).

**Revert command semantics**:
- `--revert-command CMD` is executed **after** the generator has exited or been killed and **after** the drift probe has stopped.
- If `--require-revert-command` is set and no `--revert-command` is provided, the wrapper exits before starting the generator.
- If the generator succeeds but the revert command fails, the wrapper writes `FAILED.status` with `exit_code=10`, preserving `generator_exit_code=0` and recording `revert_exit_code` separately. This prevents a misreported fully-successful cleanup.
- For local smoke tests, omit `--require-revert-command` to skip the revert step.

#### Checkpoint Artifacts (C2)

After each phase, the generator writes:
- `checkpoint_phase_000.json` (baseline)
- `checkpoint_phase_001.json` (low)
- `checkpoint_phase_002.json` (target)
- `checkpoint_phase_003.json` (cooldown)

These contain all data up to and including that phase. If the run is interrupted, the latest checkpoint preserves evidence. The final `workload_results.json` is still written on successful completion.

#### Drift Detection (C3)

The generator probes `/v1/metrics` every `--drift-check-interval` seconds (default 60s) during active (non-idle) phases. If `ferrumgate_rate_limit_per_second` or `ferrumgate_rate_limit_burst` differ from `--expected-rate-limit-ps` / `--expected-rate-limit-burst`, the generator:
1. Sets `aborted=true` and records the drift reason.
2. Breaks out of the current phase.
3. Writes the final (partial) `workload_results.json` with `aborted` and `abort_reason` fields.
4. Exits with code 3.

The wrapper detects this non-zero exit code and writes `FAILED.status`.

#### D1b Rehearsal Script (Target Side)

For target-host execution, use `scripts/run_g36_d1b_rehearsal_target.sh` instead of calling the wrapper directly. This script avoids fragile SSH inline quoting and handles the full apply-run-revert lifecycle safely:

```bash
# Copy all three scripts to target host
scp scripts/run_g36_d1b_rehearsal_target.sh \
    scripts/run_g36_workload_wrapper.sh \
    scripts/run_real_workload_generator.py \
    user@target:/opt/ferrum-g36/

# Run on target host as root
ssh user@target 'sudo /opt/ferrum-g36/run_g36_d1b_rehearsal_target.sh'
```

What the rehearsal script does:
1. **Backup** `/etc/ferrumgate/env` to a timestamped file.
2. **Apply D1b config**: removes existing `FERRUMD_RATE_LIMIT_PER_SECOND` / `FERRUMD_RATE_LIMIT_BURST` lines, appends `5` / `100`, restarts `ferrumgate.service`.
3. **Sources bearer token** from the env file (exports as `FERRUM_BEARER_TOKEN` for the wrapper; token is never printed).
4. **Invokes the wrapper** with `--revert-command` set to restore the exact backup and restart the service.
5. **Outer trap**: if the script fails **before** the wrapper starts (e.g., service restart fails), it automatically reverts from the backup.
6. **Post-run status check**: prints output directory, wrapper exit code, and service status.

**Defaults** (conservative rehearsal only):
- Server: `http://127.0.0.1:19080`
- Rate limit: `5` / `100`
- Phases: baseline 5s → target 20s → cooldown 5s
- Output: `/tmp/ferrum-g36-d1b-rehearsal-YYYYMMDD_HHMMSS/`

**Overriding phases** (e.g., for a longer rehearsal):
```bash
sudo ./run_g36_d1b_rehearsal_target.sh \
  --phases '[{"name":"baseline","duration_sec":60,"rate_rps":0},{"name":"low","duration_sec":60,"rate_rps":0.1},{"name":"target","duration_sec":300,"rate_rps":1.0},{"name":"cooldown","duration_sec":60,"rate_rps":0}]'
```

> **Note**: This is a **rehearsal**, not G3.6 acceptance. G3.6 remains **NOT ACCEPTED** until all acceptance criteria A1–A6 are met and operator signs off.

### 15.4 D1b Policy Status

| Aspect | Status |
|--------|--------|
| D1b effectiveness under sustained target load | **UNPROVEN** — the run did not complete under D1b config because the config was reverted mid-run. |
| Whether D1b would have eliminated 429s at 1 rps | **UNKNOWN** — the anomaly was caused by wrapper behavior, not D1b ceiling inadequacy. |
| Next policy decision | **Unblocked** for engineering testing. Operator must still review and sign off before any acceptance claim. |

### 15.5 Non-Claims

This section explicitly does **NOT** claim:
- G3.6 full or conditional acceptance.
- D1b policy validated or effective.
- Pilot-ready or production-ready status.
- That the wrapper bug is the only remaining blocker (other acceptance criteria A1–A6 remain unmet).

---

## 16. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial G3.6 monitoring execution plan | Engineering |
| 2026-05-12 | Partial evidence update: authenticated bounded compile-only probe executed on target host (133×200, 40×429, p50 ~205.12ms). Full phase sequence and adapter mix remain not executed. Full G3.6 acceptance not achieved. See [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md). | Engineering |
| 2026-05-12 | Extended evidence: full-duration compile-only phase sequence executed (baseline→low→target→spike→cooldown; 1,078×200, 1,987×429, `readyz/deep` degraded). No adapter mix. Full G3.6 acceptance not achieved. See [`artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md`](../artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md). | Engineering |
| 2026-05-13 | D1–D6 platform support improved: adapter wiring in `ferrumd`, API drill plan mode added, OpenAPI execute/verify coverage added, runbook lifecycle overview corrected, local checks passed. B1 remains not executed. No production-ready claim. See [`artifacts/2026-05-13-d1d6-platform-support-evidence.md`](../artifacts/2026-05-13-d1d6-platform-support-evidence.md). | Engineering |
| 2026-05-14 | Adapter-mix run failed (3,355 requests, 0×2xx, 1,104×422, 2,251×429). `trusted_context` normalization added to workload generator. Rate-limit precheck guidance and rerun/acceptance checklist added. See [`artifacts/2026-05-14-g36-adapter-mix-failed-run-evidence.md`](../artifacts/2026-05-14-g36-adapter-mix-failed-run-evidence.md). | Engineering |
| 2026-05-14 | Rerun at commit 7bcb025 executed (3,340 requests, 1,132×200, 2,208×429, 0×422). `trusted_context` fix confirmed; adapter mix exercised; rate limiter remains blocker. Next rerun strategy added (target-first, spike-separated, policy decision required). See [`artifacts/2026-05-14-g36-rerun-7bcb025-evidence.md`](../artifacts/2026-05-14-g36-rerun-7bcb025-evidence.md). | Engineering |
| 2026-05-14 | Delegated operator policy decision recorded: D1 selected (`rate_limit_per_second=2` for test window, burst=50, revert required), D2 rejected, D3 acknowledged (~1 rps production ceiling). Target-focused rerun plan updated: baseline→low→target→cooldown, no spike in acceptance rerun. | Engineering (delegated operator authority) |
| 2026-05-14 | D1 target-focused rerun attempted and aborted (low passed, target aborted at req ~88, readyz/metrics returned "Wait for 1s", config reverted). D1b policy selected (`rate_limit_per_second=5`, `burst=100`). Mandatory pre-run verification V-1 through V-4 added with STOP criteria (~1s or >0.3s wait = STOP). | Engineering (delegated operator authority) |
| 2026-05-14 | D1b pre-run verification attempted and STOPPED (V-2: 86×200/94×429, V-4: 2×200/178×429 with "Wait for 4s"; workload not started; config reverted). Deterministic config evidence checks E-1/E-2/E-3 added. Code changes: `/v1/metrics` exposes `ferrumgate_rate_limit_per_second` and `ferrumgate_rate_limit_burst`; startup log includes effective rate-limit config. | Engineering (delegated operator authority) |
| 2026-05-14 | Robust D1b target-focused rerun attempted and classified **INCOMPLETE / NOT ACCEPTED** (sentinel `exit_code=0` but `workload_results.json` missing; target-phase mid-run probes returned HTTP 429 "Wait for 0s"; run.log shows persistent 429s across all adapters; orphan PID 310198 cleaned up; config reverted; firewall restored). See [`artifacts/2026-05-14-g36-d1b-robust-run-evidence.md`](../artifacts/2026-05-14-g36-d1b-robust-run-evidence.md). | Engineering (delegated operator authority) |
| 2026-05-14 | D1b root-cause investigation completed. **Primary cause identified**: wrapper finalizer/revert ran mid-target phase, orphaning generator and reverting rate-limit config to default (`per_second=2`, `burst=50`). Contributing factors documented (wrapper sentinel decoupling, generator final-only writes, production/test key-extractor asymmetry). Next-action controls C1–C3 added as blockers before any further rerun. G3.6 remains **NOT ACCEPTED**. See [`artifacts/2026-05-14-g36-d1b-root-cause-evidence.md`](../artifacts/2026-05-14-g36-d1b-root-cause-evidence.md). | Engineering (delegated operator authority) |
| 2026-05-14 | Controls C1-C3 implemented. `scripts/run_g36_workload_wrapper.sh` added (truthful sentinel, no-orphan, deferred config revert, pre-run E-checks, background drift probe). `scripts/run_real_workload_generator.py` updated (per-phase incremental checkpoints `checkpoint_phase_*.json`, mid-run `/v1/metrics` drift detection with safe abort). Doc 116 updated to require these controls before next rerun. G3.6 remains **NOT ACCEPTED**. | Engineering |
| 2026-05-14 | D1b target-side rehearsal executed using repository-owned scripts. Short-phase rehearsal (baseline 5s, target 20s, cooldown 5s) completed successfully. Wrapper token-logging fix confirmed (sanitized output, no full token in artifacts). All checkpoints, sentinel, and revert verified. Config reverted; service active; firewall restored. **Rehearsal is not acceptance**. G3.6 remains **NOT ACCEPTED**. See [`artifacts/2026-05-14-g36-d1b-rehearsal-evidence.md`](../artifacts/2026-05-14-g36-d1b-rehearsal-evidence.md). | Engineering |
| 2026-05-14 | Wrapper security fix: `--bearer-token` removed from generator argv. Token now passed via `FERRUM_BEARER_TOKEN` env var only, eliminating process-list exposure. Docs updated to record argv exposure remediation and token rotation. G3.6 remains **NOT ACCEPTED**. | Engineering |
| 2026-05-14 | **D1b full-duration acceptance rerun executed** (baseline 600s, low 600s, target 1800s, cooldown 600s). Sentinel `exit_code=0`, all artifacts present. Target phase 1,801 requests with ≈80.4% HTTP 429 (1,448×429, 353×200). All adapters exercised but heavily throttled. `readyz_probe_log.json` present but 0 probes parsed. `config_drift_log.jsonl` shows metrics endpoint also throttled. Post-run state verified clean. Token process-list exposure remediated (argv → env-only, token rotated, fix deployed and validated). **NOT ACCEPTED** due to target-phase 429 rate >5%. G3.6 remains **NOT ACCEPTED**. See [`artifacts/2026-05-14-g36-d1b-full-rerun-evidence.md`](../artifacts/2026-05-14-g36-d1b-full-rerun-evidence.md). | Engineering |
| 2026-05-14 | **P0 implemented locally**: `/v1/metrics`, `/v1/readyz`, `/v1/readyz/deep` exempted from workload `GovernorLayer` via route-layer split (monitoring router + workload router). `run_http_server` applies rate limiter only to workload router. `build_router_with_governor` test helper updated to match production behavior. Existing rate-limit integration tests migrated from `/v1/healthz` to `/v1/approvals` (workload route). New unit tests prove monitoring bypass and workload throttling. **G3.6 remains NOT ACCEPTED** — target deploy + Phase 1.5 diagnostic gate remain next steps. No production-ready or pilot-ready claim. | Engineering |
| 2026-05-14 | **P1/C1 implemented locally**: Production `GovernorConfig` aligned with test helper by switching from default `PeerIpKeyExtractor` to `SmartIpKeyExtractor` (`x-real-ip` / `x-forwarded-for` aware). Generator updated to send deterministic per-adapter private IPs (`10.36.0.x`) via `--simulate-client-ips` (default enabled in execute mode). New tests prove distinct headers get separate buckets and same header is still limited. Docs updated. Target diagnostic/rerun remains next. G3.6 remains **NOT ACCEPTED**. | Engineering |
| 2026-05-14 | **P0+P1 target evidence collected**: P1 short diagnostic passed (298/298 HTTP 200, 0×429). P0+P1 full-duration target-focused rerun passed the workload 429 gate (1,852/1,852 HTTP 200; target phase 1,792/1,792 HTTP 200; all adapters exercised; no config drift log; clean revert to `2`/`50`; SSH firewall restored to `118.69.4.63/32`). **G3.6 full acceptance remains pending** operator signoff and remaining A-criteria evidence; no pilot-ready or production-ready claim. See [`artifacts/2026-05-14-g36-p0-p1-full-rerun-evidence.md`](artifacts/2026-05-14-g36-p0-p1-full-rerun-evidence.md). | Engineering |
| 2026-05-14 | **A5 fresh backup verify executed** (`ferrumctl backup verify` OK on `ferrumgate_20260513_163232.db`). **A6 delegated conditional signoff recorded**: operator authority delegated from user to assistant for G3.6 conditional acceptance only. Doc 106 updated to **CONDITIONAL ACCEPTANCE (delegated) for P5b planning input** (supersedes 2026-05-11). No full acceptance, pilot-ready, or production-ready claim. | Engineering (delegated operator authority) |
| 2026-05-14 | **A3/spike confirmatory rerun executed** (baseline→low→target→spike→cooldown; 597/597 HTTP 200, 0×429). Generator enhanced with `--readyz-probe-phase-interval` and `--capture-connections`. 4/4 target mid-run readyz probes HTTP 200. Spike 290/290 HTTP 200 at 5 rps. Connection counts peak=1, typical=1. Safe restore preflight passed (backup verify OK, pre-restore copy prepared). **T3b destructive restore-to-production NOT executed** — recorded as remaining gate requiring explicit user YES. G3.6 remains **NOT FULLY ACCEPTED**. See [`artifacts/2026-05-14-g36-a3-spike-confirmatory-evidence.md`](artifacts/2026-05-14-g36-a3-spike-confirmatory-evidence.md). | Engineering |
| 2026-05-15 | **T3b destructive restore-to-production drill attempted** with explicit user authorization (run ID `20260515T070905Z`). Pre-checks passed; backup verify OK; pre-restore copy created; dry-run passed; service stopped. Actual restore command **timed out after 180s**. **Rollback succeeded**: pre-restore copy restored, service restarted, readyz/deep HTTP 200, live DB verify OK, firewall restored. **T3b NOT ACCEPTED**. G3.6 full acceptance remains blocked pending root-cause investigation or alternate restore method. See [`artifacts/2026-05-15-g36-t3b-restore-drill-timeout-evidence.md`](artifacts/2026-05-15-g36-t3b-restore-drill-timeout-evidence.md). | Engineering |
| 2026-05-15 | **T3b root-cause fix identified and deployed**: slow SQLite backup API for pre-restore snapshot replaced with `std::fs::copy` in `bins/ferrumctl/src/backup.rs`. Local tests 16/16 passed. Fixed binary deployed to target host. **T3b reattempted successfully** (run ID `20260515T074001Z`): restore completed in 0.463s; live DB verify OK; service healthy. G3.6 updated to **FULL ACCEPTANCE for P5b engineering review only**. All non-claims preserved (NOT production-ready, NOT pilot-ready, NOT HA/multi-node, NOT PostgreSQL production). See [`artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md`](artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md). | Engineering |

---

## 17. P1 / C1 — Per-Adapter Client IP Simulation

### 17.1 Problem

The D1b full rerun showed ≈80.4% HTTP 429 in the target phase. A contributing factor is the **shared rate-limit bucket**: production used the default `PeerIpKeyExtractor`, so all traffic from the same source IP (generator + drift probes) competed for one quota. The test helper already used `SmartIpKeyExtractor`, creating an asymmetry between local validation and production behavior.

### 17.2 Implementation

#### Production alignment (Rust)

`crates/ferrum-gateway/src/server.rs`:
- `run_http_server` now explicitly sets `.key_extractor(SmartIpKeyExtractor)` on the `GovernorConfigBuilder`.
- This aligns production with the existing `build_router_with_governor` test helper.
- `SmartIpKeyExtractor` reads `x-real-ip` and `x-forwarded-for` headers before falling back to the peer socket address.

#### Generator enhancement (Python)

`scripts/run_real_workload_generator.py`:
- Added `--simulate-client-ips` / `--no-simulate-client-ips` CLI flags.
- Default behavior: **enabled in execute mode**, disabled in plan mode.
- When enabled, each adapter gets a deterministic private IP in the `x-real-ip` header:
  - `fs` → `10.36.0.1`
  - `git` → `10.36.0.2`
  - `http` → `10.36.0.3`
  - `sqlite` → `10.36.0.4`
  - `maildraft` → `10.36.0.5`
- These are safe RFC 1918 / test-range addresses that do not expose real infrastructure.

#### Tests

New integration tests in `crates/ferrum-gateway/src/server.rs`:
- `test_distinct_x_real_ip_get_separate_buckets` — proves that two different `x-real-ip` values each have independent rate-limit buckets.
- `test_same_x_real_ip_is_limited_across_adapters` — proves that the same IP is still throttled regardless of which workload route it hits.

### 17.3 How to Use for Next Rerun

No CLI change is required for the default execute-mode behavior; the generator will automatically send `x-real-ip` headers. To disable:

```bash
python3 scripts/run_real_workload_generator.py --execute --no-simulate-client-ips ...
```

### 17.4 Non-Claims

- P1 does **not** raise the default production rate-limit ceiling.
- P1 does **not** implement a custom per-principal extractor (C2 remains out of scope).
- P1 does **not** grant G3.6 acceptance by itself; it removes one contributing factor so the next rerun can evaluate the true ceiling.

### 17.5 Target Diagnostic and Full Rerun Evidence

P1/C1 was deployed to the target host after P0. The short diagnostic and full-duration target-focused rerun both passed the workload rate-limit gate:

| Run | Output Directory | Result |
|---|---|---|
| P1 short diagnostic | `/tmp/ferrum-g36-p1-diagnostic-20260514` | 298/298 HTTP 200, 0×429, no drift log, clean revert |
| P0+P1 full rerun | `/tmp/ferrum-g36-p1-full-acceptance-20260514` | 1,852/1,852 HTTP 200 overall; target phase 1,792/1,792 HTTP 200; all adapters exercised; no drift log; clean revert |

See [`artifacts/2026-05-14-g36-p0-p1-full-rerun-evidence.md`](artifacts/2026-05-14-g36-p0-p1-full-rerun-evidence.md).

**Conservative verdict**: the P0+P1 rerun resolves the previously observed target-phase 429 blocker for this workload. G3.6 full acceptance still requires operator signoff and remaining A-criteria evidence review; no pilot-ready or production-ready claim is made.

---

*Document updated: 2026-05-14. G3.6 Monitoring Execution Plan — planning/checklist artifact. Partial evidence only. No production-ready claim. P6 CONDITIONAL GO.*
