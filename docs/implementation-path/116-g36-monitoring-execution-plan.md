# 116 — G3.6 Monitoring Execution Plan

> **Status**: Planning/checklist artifact. Partial evidence gathered 2026-05-12 (authenticated bounded compile-only probe and full-duration compile-only sequence: baseline→low→target→spike→cooldown; 1,078×200, 1,987×429, `readyz/deep` degraded at target/spike). Full G3.6 acceptance not achieved. No production-ready claim.  
> **Purpose**: Execution plan for transitioning G3.6 from **conditionally accepted** (compile-only/light workload) to **full acceptance** with real workload validation.  
> **Scope**: Post-deploy monitoring on target host. Adapter execution paths exercised.  
> **Constraint**: This plan does NOT make FerrumGate production-ready. P5b–P5e remain gated on G3.6 full acceptance. Do not record secrets.

---

## 1. Purpose

This plan provides the operator and engineering teams with a structured approach to collecting **real workload evidence** for G3.6:

> **G3.6**: G2 pilot data available for P5b pool-tuning input — sustained write rate, connection patterns, queue depth, readyz/deep behavior, metrics snapshots, and backup/restore status.

Current status per `106-g3-6-pilot-metrics-evidence-packet.md`:
- **Conditionally accepted** on 2026-05-11 for initial P5b planning only
- Compile-only workload; adapter execution paths (FS, Git, HTTP, SQLite, Maildraft) **unexercised**
- No low/target/spike/cooldown metrics sequence

**Update 2026-05-12 (bounded probe)**: Authenticated bounded compile-only probe executed on target host (173 total requests, 133 HTTP 200, 40 HTTP 429, p50 ~205.12ms). This is **not** full G3.6 acceptance. See [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §9.

**Update 2026-05-12 (full-duration sequence)**: Full-duration compile-only phase sequence executed (baseline 600s → low 600s → target 1800s → spike 300s → cooldown 600s; 1,078×200, 1,987×429, overall p50 ~203.2ms). `readyz/deep` degraded to 3/5 at target and 2/5 at spike. No adapter mix. **Not** full G3.6 acceptance. See [`artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md`](../artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md) §7.

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
| Connection count | `ss -tn` or Prometheus `connections` metric (if exposed) |

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
| T-6 | Probe `readyz/deep` every 60s; record all results | ☐ |
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
| S-6 | Probe `readyz/deep` every 30s; record all results | ☐ |
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

---

## 12. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial G3.6 monitoring execution plan | Engineering |
| 2026-05-12 | Partial evidence update: authenticated bounded compile-only probe executed on target host (133×200, 40×429, p50 ~205.12ms). Full phase sequence and adapter mix remain not executed. Full G3.6 acceptance not achieved. See [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md). | Engineering |
| 2026-05-12 | Extended evidence: full-duration compile-only phase sequence executed (baseline→low→target→spike→cooldown; 1,078×200, 1,987×429, `readyz/deep` degraded). No adapter mix. Full G3.6 acceptance not achieved. See [`artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md`](../artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md). | Engineering |
| 2026-05-13 | D1–D6 platform support improved: adapter wiring in `ferrumd`, API drill plan mode added, OpenAPI execute/verify coverage added, runbook lifecycle overview corrected, local checks passed. B1 remains not executed. No production-ready claim. See [`artifacts/2026-05-13-d1d6-platform-support-evidence.md`](../artifacts/2026-05-13-d1d6-platform-support-evidence.md). | Engineering |

---

*Document updated: 2026-05-13. G3.6 Monitoring Execution Plan — planning/checklist artifact. Partial evidence only. No production-ready claim. P6 CONDITIONAL GO.*
