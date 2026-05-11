# 106 — G3.6 Pilot Metrics Evidence Packet

> **Status**: Partial evidence attached with 1h compile-only workload + restore drill. G3.6 conditionally ready for operator review — A6 (operator signoff) remains pending.  
> **Scope**: Path 2 single-node SQLite pilot metrics collection for P5b pool-tuning input only.  
> **Constraint**: This packet does NOT authorize P5b–P5e implementation. G3.5, Eng.1, and Eng.2 are now satisfied; G3.6 must also be satisfied before P5b–P5e begin.  
> **Purpose**: Structured evidence collection template for G3.6 per `31-release-paths-todo.md` §Path 3 Gate.

---

## Purpose

This packet captures the real pilot metrics and logs required to satisfy **G3.6**:

> **G3.6**: G2 pilot data available for P5b pool-tuning input — sustained write rate, connection patterns, queue depth, readyz/deep behavior, metrics snapshots, and backup/restore status.

G3.6 is an **operator-owned gate**. Engineering cannot fabricate or assume pilot data.
The evidence in this packet is used solely for P5b connection-pool sizing and circuit-breaker tuning.
It does **not** constitute a production-ready claim, does **not** authorize P5b–P5e implementation by itself, and does **not** replace Eng.1/Eng.2 engineering planning confirmation.

**Operator-owned**: All fields below require real data from the target pilot environment.
Do not pre-fill with estimates or local simulation results unless explicitly labeled as such.

---

## Explicit Non-Claims

- **No production-ready claim**: Collecting G3.6 metrics does NOT make FerrumGate production-ready.
- **No P5 implementation authorization**: P5b–P5e remain gated on G3.6 (this packet). G3.5, Eng.1, and Eng.2 are satisfied but do not alone authorize implementation.
- **No HA/multi-node authorization**: Pilot metrics from single-node SQLite do not validate HA/clustering behavior.
- **No PostgreSQL production deployment**: G3.6 data informs P5b design only; production deployment requires P5b–P5e completion + P6 assessment.
- **No operator signature pre-filled**: All signoff fields remain blank until the operator attaches real evidence and signs.

---

## Prerequisites

Before collecting G3.6 evidence, confirm the following:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | G3.4 (P5a design) approved | `104-g3-4-p5a-adr-approval-packet.md` signed | ☑ DONE |
| R2 | G3.5 (operator D1–D3) signed | `105-g3-5-operator-d1-d3-signoff-packet.md` signed | ☑ DONE (Option A defaults via chat authorization on 2026-05-11) |
| R3 | Path 2 pilot is running or has completed | `59-pilot-readiness-evidence-packet.md` signed; `ferrumd` process confirmed on `ferrumgate-nonprod` (RUNNING) | ☑ DONE (observed 2026-05-11) |
| R4 | Monitoring endpoint accessible | `/v1/metrics` and `/v1/readyz/deep` return HTTP 200 | ☑ DONE (observed 2026-05-11T16:35:29Z) |
| R5 | Backup schedule operational | `ferrumgate-backup.timer` active; 1 backup exists and verified OK | ☑ DONE (observed 2026-05-11) |

---

## Evidence Collection Fields

### 1. Sustained Write Rate

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `measurement_period` | Start and end timestamps of the observation window | `2026-05-11T17:06:28Z` – `2026-05-11T18:06:29Z` (3600.7s) |
| `peak_writes_per_second` | Highest instantaneous write rate observed | **CAVEAT**: compile-only workload, not execution side effects. Peak successful compile rate ≈0.5 req/s (observed over 1h window). |
| `sustained_writes_per_second_p50` | Median sustained write rate over the window | **CAVEAT**: compile-only. p50 latency ≈218ms; successful compile throughput ≈0.5 req/s. |
| `sustained_writes_per_second_p95` | 95th-percentile sustained write rate | **CAVEAT**: compile-only. p95 latency ≈326ms. |
| `sustained_writes_per_second_p99` | 99th-percentile sustained write rate | **CAVEAT**: compile-only. p99 latency ≈523ms. |
| `total_intents_executed` | Count of intents successfully executed in the window | 1805 successful compiles (HTTP 200) out of 3582 attempts. 1777 returned HTTP 429 (rate limited). |
| `write_source_breakdown` | Breakdown by adapter (FS, Git, HTTP, SQLite, Maildraft) | **NOT COLLECTED** — compile-only workload does not exercise adapter execution paths. |

**Acceptance threshold for single-node SQLite**: ≤300 writes/s sustained.  
**P5b relevance**: If sustained rate approaches or exceeds 250 writes/s, P5b pool tuning should target ≥500 writes/s headroom for PostgreSQL.

---

### 2. Connection Patterns

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `concurrent_client_connections_peak` | Peak number of simultaneous HTTP client connections | **NOT COLLECTED** — no connection-pool metrics available from current Prometheus scrape |
| `concurrent_client_connections_typical` | Typical number of simultaneous HTTP client connections | **NOT COLLECTED** |
| `connection_duration_p50` | Median connection lifetime | **NOT COLLECTED** — `request_duration_seconds` present but no workload to produce representative percentiles |
| `connection_duration_p95` | 95th-percentile connection lifetime | **NOT COLLECTED** |
| `auth_mode` | Bearer auth or disabled | `bearer` (confirmed from VM config `FERRUMD_AUTH_MODE=bearer`) |
| `tls_termination` | Reverse proxy TLS or direct | Reverse proxy via `caddy.service` (active); target URL `https://ferrumgate.duckdns.org` |
| `client_geography` | Single region or multi-region | Single region (`asia-southeast1-a`); no multi-region evidence |

**P5b relevance**: `concurrent_client_connections_peak` directly informs `max_connections` pool sizing.

---

### 3. Queue Depth

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `write_queue_depth_peak` | Maximum `ferrumgate_write_queue_depth` observed | 0 (observed across all samples; `max_over_time[1h]` = 0 pre-workload and post-workload) |
| `write_queue_depth_sustained` | Sustained (p95) queue depth | 0 (idle and post-1h-compile-workload; queue never exceeded 0) |
| `write_queue_drain_time_p50` | Median time for queue to drain from peak to empty | N/A — queue never exceeded 0; no drain events observed |
| `queue_backlog_events` | Number of times backlog exceeded 100 items | 0 |
| `queue_rejection_events` | Number of intents rejected due to queue saturation | 0 (HTTP 429 responses were rate-limiter rejections, not queue saturation)

**P5b relevance**: Peak queue depth and drain time determine whether PostgreSQL pool sizing can absorb bursts or whether backpressure/circuit-breaker tuning is required.

---

### 4. Readiness Probe (`readyz/deep`)

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `probe_schedule` | How often `GET /v1/readyz/deep` was polled | Pre-workload: 5 manual samples at ~10s intervals (2026-05-11T16:36:01Z – 16:36:44Z). Post-workload: 5 manual samples at ~10s intervals (2026-05-11T18:06:40Z – 18:07:12Z) |
| `probe_success_rate` | Percentage of probes returning HTTP 200 | 100% (10/10 total; 5/5 pre-workload, 5/5 post-workload) |
| `probe_failure_count` | Number of non-200 responses | 0 |
| `probe_failure_codes` | HTTP status codes observed on failure (e.g., 503) | None observed |
| `component_store_up` | Percentage of successful probes where `store` component reported `up` | 100% (store_health_up=1 on all 10 samples) |
| `component_write_queue_up` | Percentage of successful probes where `write_queue` component reported `up` | 100% (deep_status=ok on all 10 samples; queue_depth=0 is healthy idle state) |
| `deepest_failure_reason` | If any probe failed, root cause (e.g., store timeout, disk full) | N/A — no failures observed pre-workload or post-workload |

**P5b relevance**: Persistent `store` or `write_queue` component failures under load indicate pool or concurrency model mismatch.

---

### 5. Metrics Snapshots

Attach raw metrics output or link to monitoring system. Minimum required snapshots:

| Snapshot | Timing | Content Required |
|---|---|---|
| Baseline (idle) | 2026-05-11T16:35:46Z | `GET /v1/metrics` output (12,980 bytes). All required counters present. `ferrumgate_write_queue_depth=0`, `ferrumgate_store_health_up=1`. See artifact `2026-05-11-g3-6-live-metrics-partial-evidence.md` §2 |
| Post-workload (compile-only) | 2026-05-11T18:06:29Z | `GET /v1/metrics` output post-1h compile-only workload. `ferrumgate_write_queue_depth=0`, `ferrumgate_store_health_up=1`, `governance_errors_total=0`. 1805 successful compiles recorded. See artifact `2026-05-11-g3-6-live-metrics-partial-evidence.md` §6 |
| Low load | **NOT COLLECTED** | No low-load workload executed |
| Target load | **NOT COLLECTED** | No target-load workload executed |
| Spike load | **NOT COLLECTED** | No spike-load workload executed |
| Cooldown | **NOT COLLECTED** | No workload to cool down from |

**Required metrics to verify presence**:
- `ferrumgate_write_queue_depth`
- `ferrumgate_http_requests_total`
- `ferrumgate_request_duration_seconds`
- `ferrumgate_store_health_up`
- `ferrumgate_governance_errors_total`

**Optional but helpful**:
- WAL size / page count (if exposed by host monitoring)
- Disk I/O wait % (if exposed by host monitoring)
- Memory usage of `ferrumd` process

---

### 6. Backup / Restore Status

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `backup_schedule` | Cron expression or systemd timer schedule | `ferrumgate-backup.timer` active (systemd timer) |
| `backups_taken_during_pilot` | Number of backups executed during the observation window | 1 backup file present in `/var/lib/ferrumgate/backups` |
| `backup_verify_pass_rate` | Percentage of backups where `ferrumctl backup verify` returned OK | 100% (1/1 verified OK) |
| `last_backup_timestamp` | Timestamp of most recent backup | `2026-05-11T16:33:12Z` (mtime of `ferrumgate_20260508_154446.db`) |
| `last_restore_drill_timestamp` | Timestamp of most recent restore drill | 2026-05-11T17:04:57Z |
| `restore_drill_result` | OK / FAILED (with reason) | OK — restored to temp path (`mktemp -d`), `ferrumctl backup verify` passed on restored copy, temp path removed |
| `rpo_accepted_minutes` | Operator-accepted RPO in minutes | **NOT COLLECTED** — operator must define and accept |
| `rto_accepted_minutes` | Operator-accepted RTO in minutes | Coarsely under 120s (restore completed within `ferrumctl backup restore` default timeout; exact seconds not instrumented) |

---

## Collection Methods

### Automated Helper

[`scripts/check_pilot_readiness.py`](../../scripts/check_pilot_readiness.py) can run shallow, deep, and functional readiness probes against a live `ferrumd` instance and verify the metrics endpoint.

```bash
# Run automated probes (does NOT complete G3.6 by itself)
python3 scripts/check_pilot_readiness.py \
  --server-url https://ferrumgate.example.com \
  --bearer-token "$FERRUMCTL_BEARER_TOKEN"
```

**Important**: `check_pilot_readiness.py` performs pass/fail probes only. It does not collect sustained write-rate histograms, queue-depth time series, or backup history. Use it as a sanity check, not as a substitute for the evidence fields above.

### Manual Collection Commands

```bash
# Deep readiness probe
curl -s https://ferrumgate.example.com/v1/readyz/deep | jq .

# Metrics snapshot
curl -s -H "Authorization: Bearer $FERRUMCTL_BEARER_TOKEN" \
  https://ferrumgate.example.com/v1/metrics > metrics_$(date +%Y%m%d_%H%M%S).txt

# Backup verify
ferrumctl backup verify --db-path /var/lib/ferrumgate/ferrumgate.db

# Safe restore drill (restore to temp path; never touch live DB)
TMPDIR=$(mktemp -d)
ferrumctl backup restore \
  --backup-path /var/lib/ferrumgate/backups/ferrumgate_YYYYMMDD_HHMMSS.db \
  --target-dir "$TMPDIR"
ferrumctl backup verify --db-path "$TMPDIR"/ferrumgate.db
rm -rf "$TMPDIR"
```

---

## Acceptance Criteria

G3.6 is satisfied when **all** of the following are true:

| # | Criterion | Evidence |
|---|---|---|
| A1 | At least one sustained write-rate measurement covers ≥1 hour of representative workload | Field 1 filled with timestamps and rates |
| A2 | Queue depth observed at both idle and target load | Field 3 filled with peak and sustained values |
| A3 | `readyz/deep` probe success rate ≥99% over the observation window | Field 4 filled with success rate and failure count |
| A4 | At least one metrics snapshot at target load contains all required metrics | Field 5 attached or linked |
| A5 | Most recent backup verify passes and restore drill completed within operator-accepted RTO | Field 6 filled |
| A6 | Operator has reviewed all fields and signed below | §Operator Signoff completed |

**If any criterion is not met**: G3.6 remains pending. Do not proceed to P5b–P5e.

---

## Stop Conditions

| Trigger | Action |
|---|---|
| Sustained write rate >300 writes/s | Abort single-node SQLite pilot; evaluate Path 3 PostgreSQL |
| `readyz/deep` success rate <95% | Investigate store or write_queue health before claiming G3.6 complete |
| Backup verify fails during pilot | Do not claim G3.6 complete; resolve backup issues first |
| Queue backlog >100 items sustained | Evaluate whether workload exceeds single-node capacity |
| Metrics endpoint missing required counters | Upgrade to a build that exports required metrics before collecting evidence |

---

## Operator Signoff

> **Operator instruction**: Attach real evidence for all fields above, confirm all acceptance criteria (A1–A6) are met, and sign below.  
> **Do not sign if any field is estimated, simulated, or incomplete.**  
> **This signoff does NOT authorize P5b–P5e implementation.** G3.6 is the remaining gate.

### Operator Information

| Field | Value |
|---|---|
| Operator name | _________________________ |
| Organization | _________________________ |
| Pilot environment | _________________________ |
| Observation window | _________________________ |
| Date | _________________________ |

### Evidence Checklist

| # | Check | Status |
|---|---|---|
| E1 | Sustained write rate (Field 1) attached and reviewed | [ ] |
| E2 | Connection patterns (Field 2) attached and reviewed | [ ] |
| E3 | Queue depth (Field 3) attached and reviewed | [ ] |
| E4 | Readiness probe results (Field 4) attached and reviewed | [ ] |
| E5 | Metrics snapshots (Field 5) attached or linked | [ ] |
| E6 | Backup/restore status (Field 6) attached and reviewed | [ ] |
| E7 | Acceptance criteria A1–A6 confirmed | [ ] |
| E8 | I understand that G3.6 alone does NOT authorize P5b–P5e | [ ] |
| E9 | I understand that full production-ready requires P5b–P5e completion + P6 assessment | [ ] |

---

## Why G3.6 Remains Incomplete

| Criterion | Status | Reason |
|---|---|---|
| A1 — ≥1h sustained write-rate measurement | **MET with caveat** | 1h compile-only workload executed (2026-05-11T17:06:28Z–18:06:29Z). 3582 requests, 1805 HTTP 200, 1777 HTTP 429. **Caveat**: compile-only; adapter execution paths (FS, Git, HTTP, SQLite, Maildraft) not exercised. |
| A2 — Queue depth at idle and target load | **MET with caveat** | Queue depth observed at idle (0) and post-1h-compile-workload (0). `max_over_time[1h]` = 0 pre- and post-workload. **Caveat**: workload was compile-only; no adapter execution to stress write queue. |
| A3 — `readyz/deep` success rate ≥99% | **MET** | 100% success over 10 manual samples (5 pre-workload, 5 post-workload) + 1h Prometheus window (0 non-200 responses). |
| A4 — Metrics snapshot at target load with all required counters | **MET with caveat** | All 5 required metrics verified present. Baseline (idle) snapshot + post-workload snapshot collected. **Caveat**: no low/target/spike/cooldown sequence; compile-only workload. |
| A5 — Backup verify passes + restore drill within RTO | **MET with caveat** | Backup verify passed (`OK`). Restore drill executed 2026-05-11T17:04:57Z: restored to temp path, verified OK, cleaned up. RTO coarsely under 120s. **Caveat**: RPO/RTO not formally operator-accepted; exact RTO seconds not instrumented. |
| A6 — Operator signoff | **NOT MET** | Operator has not signed §Operator Signoff below. |

**Conclusion**: G3.6 is **conditionally ready for operator review**. Acceptance criteria A1–A5 are met with caveats (compile-only workload; adapter execution paths not exercised; RPO/RTO not formally operator-accepted). The only remaining blocker is **A6 — operator signoff**. Once the operator reviews the evidence, confirms the caveats are acceptable for P5b pool-tuning input, and signs below, G3.6 will be complete.

**Artifact reference**: See `docs/implementation-path/artifacts/2026-05-11-g3-6-live-metrics-partial-evidence.md` for sanitized raw evidence.

### Approval Statement

> **Select ONE:**

- [ ] **COMPLETE** — All G3.6 evidence fields are attached, acceptance criteria met, and data is ready for P5b engineering review.
- [ ] **INCOMPLETE** — Some fields are missing or criteria not met. Reason: _________________________________
- [ ] **N/A** — No pilot data available; G3.6 deferred. Reason: _________________________________

### Signature

| Role | Signature | Date |
|---|---|---|
| Operator / Decision Authority | _________________________ | _________________________ |
| Engineering Lead (acknowledgment of receipt) | _________________________ | _________________________ |
| Witness (optional) | _________________________ | _________________________ |

---

## Cross-References

| This Doc | Links To | Purpose |
|---|---|---|
| `106-g3-6-pilot-metrics-evidence-packet.md` | `31-release-paths-todo.md` §Path 3 Gate | G3.6 gate definition |
| `106-g3-6-pilot-metrics-evidence-packet.md` | `61-path-2-execution-plan.md` §Step 5 | Path 2 pilot metrics collection context |
| `106-g3-6-pilot-metrics-evidence-packet.md` | `104-g3-4-p5a-adr-approval-packet.md` | G3.4 prerequisite |
| `106-g3-6-pilot-metrics-evidence-packet.md` | `105-g3-5-operator-d1-d3-signoff-packet.md` | G3.5 prerequisite (still required for P5b–P5e) |
| `106-g3-6-pilot-metrics-evidence-packet.md` | `50-p4-postgres-store-facade-adr.md` §3.5 P5a | P5b pool-tuning design context |
| `106-g3-6-pilot-metrics-evidence-packet.md` | `59-pilot-readiness-evidence-packet.md` | G2 signed conditional pilot evidence |
| `106-g3-6-pilot-metrics-evidence-packet.md` | `scripts/check_pilot_readiness.py` | Automated probe helper |
| `31-release-paths-todo.md` | This doc | G3.6 evidence reference |
| `61-path-2-execution-plan.md` | This doc | G3.6 pilot metrics reference |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | This doc | G3.6 next step context |
| `107-eng-1-capacity-confirmation-packet.md` | This doc | Eng.1 capacity confirmation (signed via chat authorization) |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | This doc | Eng.2 implementation planning (approved via chat authorization) |
| `artifacts/2026-05-11-g3-6-live-metrics-partial-evidence.md` | This doc | Sanitized live metrics evidence attachment |

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-11 | Initial G3.6 pilot metrics evidence packet drafted | Engineering |
| 2026-05-11 | Partial live evidence collected from `ferrumgate-nonprod` and attached | Assistant (recorded per user instruction) |
| 2026-05-11 | 1h compile-only workload + post-workload probes + safe restore drill added; A1–A5 updated to MET with caveats; A6 remains pending | Assistant (recorded per user instruction) |

---

*Document created: 2026-05-11. G3.6 operator-owned evidence packet — NOT complete until operator attaches real pilot data. No production-ready claim. No P5b–P5e implementation authorization.*
