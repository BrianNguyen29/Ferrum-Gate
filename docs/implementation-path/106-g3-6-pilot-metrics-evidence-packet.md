# 106 — G3.6 Pilot Metrics Evidence Packet

> **Status**: Pending. Awaiting operator-supplied pilot metrics/logs. Do not mark complete without real evidence.  
> **Scope**: Path 2 single-node SQLite pilot metrics collection for P5b pool-tuning input only.  
> **Constraint**: This packet does NOT authorize P5b–P5e implementation. G3.5 and Eng.1 must also be satisfied before P5b–P5e begin.  
> **Purpose**: Structured evidence collection template for G3.6 per `31-release-paths-todo.md` §Path 3 Gate.

---

## Purpose

This packet captures the real pilot metrics and logs required to satisfy **G3.6**:

> **G3.6**: G2 pilot data available for P5b pool-tuning input — sustained write rate, connection patterns, queue depth, readyz/deep behavior, metrics snapshots, and backup/restore status.

G3.6 is an **operator-owned gate**. Engineering cannot fabricate or assume pilot data.
The evidence in this packet is used solely for P5b connection-pool sizing and circuit-breaker tuning.
It does **not** constitute a production-ready claim, does **not** authorize P5b–P5e implementation by itself, and does **not** replace G3.5 operator D1–D3 signoff or Eng.1 capacity confirmation.

**Operator-owned**: All fields below require real data from the target pilot environment.
Do not pre-fill with estimates or local simulation results unless explicitly labeled as such.

---

## Explicit Non-Claims

- **No production-ready claim**: Collecting G3.6 metrics does NOT make FerrumGate production-ready.
- **No P5 implementation authorization**: P5b–P5e remain gated on G3.5 (operator D1–D3) and Eng.1 (engineering planning).
- **No HA/multi-node authorization**: Pilot metrics from single-node SQLite do not validate HA/clustering behavior.
- **No PostgreSQL production deployment**: G3.6 data informs P5b design only; production deployment requires P5b–P5e completion + P6 assessment.
- **No operator signature pre-filled**: All signoff fields remain blank until the operator attaches real evidence and signs.

---

## Prerequisites

Before collecting G3.6 evidence, confirm the following:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | G3.4 (P5a design) approved | `104-g3-4-p5a-adr-approval-packet.md` signed | ☑ DONE |
| R2 | Path 2 pilot is running or has completed | `59-pilot-readiness-evidence-packet.md` signed; `ferrumd` operational on target host | ☐ Pending (operator) |
| R3 | Monitoring endpoint accessible | `/v1/metrics` and `/v1/readyz/deep` reachable | ☐ Pending (operator) |
| R4 | Backup schedule operational | At least one automated backup has run and verified successfully | ☐ Pending (operator) |

---

## Evidence Collection Fields

### 1. Sustained Write Rate

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `measurement_period` | Start and end timestamps of the observation window | _________________________________ |
| `peak_writes_per_second` | Highest instantaneous write rate observed | _________________________________ |
| `sustained_writes_per_second_p50` | Median sustained write rate over the window | _________________________________ |
| `sustained_writes_per_second_p95` | 95th-percentile sustained write rate | _________________________________ |
| `sustained_writes_per_second_p99` | 99th-percentile sustained write rate | _________________________________ |
| `total_intents_executed` | Count of intents successfully executed in the window | _________________________________ |
| `write_source_breakdown` | Breakdown by adapter (FS, Git, HTTP, SQLite, Maildraft) | _________________________________ |

**Acceptance threshold for single-node SQLite**: ≤300 writes/s sustained.  
**P5b relevance**: If sustained rate approaches or exceeds 250 writes/s, P5b pool tuning should target ≥500 writes/s headroom for PostgreSQL.

---

### 2. Connection Patterns

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `concurrent_client_connections_peak` | Peak number of simultaneous HTTP client connections | _________________________________ |
| `concurrent_client_connections_typical` | Typical number of simultaneous HTTP client connections | _________________________________ |
| `connection_duration_p50` | Median connection lifetime | _________________________________ |
| `connection_duration_p95` | 95th-percentile connection lifetime | _________________________________ |
| `auth_mode` | Bearer auth or disabled | _________________________________ |
| `tls_termination` | Reverse proxy TLS or direct | _________________________________ |
| `client_geography` | Single region or multi-region | _________________________________ |

**P5b relevance**: `concurrent_client_connections_peak` directly informs `max_connections` pool sizing.

---

### 3. Queue Depth

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `write_queue_depth_peak` | Maximum `ferrumgate_write_queue_depth` observed | _________________________________ |
| `write_queue_depth_sustained` | Sustained (p95) queue depth | _________________________________ |
| `write_queue_drain_time_p50` | Median time for queue to drain from peak to empty | _________________________________ |
| `queue_backlog_events` | Number of times backlog exceeded 100 items | _________________________________ |
| `queue_rejection_events` | Number of intents rejected due to queue saturation | _________________________________ |

**P5b relevance**: Peak queue depth and drain time determine whether PostgreSQL pool sizing can absorb bursts or whether backpressure/circuit-breaker tuning is required.

---

### 4. Readiness Probe (`readyz/deep`)

| Field | Description | Value / Evidence (operator fills in) |
|---|---|---|
| `probe_schedule` | How often `GET /v1/readyz/deep` was polled | _________________________________ |
| `probe_success_rate` | Percentage of probes returning HTTP 200 | _________________________________ |
| `probe_failure_count` | Number of non-200 responses | _________________________________ |
| `probe_failure_codes` | HTTP status codes observed on failure (e.g., 503) | _________________________________ |
| `component_store_up` | Percentage of successful probes where `store` component reported `up` | _________________________________ |
| `component_write_queue_up` | Percentage of successful probes where `write_queue` component reported `up` | _________________________________ |
| `deepest_failure_reason` | If any probe failed, root cause (e.g., store timeout, disk full) | _________________________________ |

**P5b relevance**: Persistent `store` or `write_queue` component failures under load indicate pool or concurrency model mismatch.

---

### 5. Metrics Snapshots

Attach raw metrics output or link to monitoring system. Minimum required snapshots:

| Snapshot | Timing | Content Required |
|---|---|---|
| Baseline (idle) | Before pilot workload | `GET /v1/metrics` output with all counters at rest |
| Low load | ≤50% of expected sustained rate | `GET /v1/metrics` output |
| Target load | Expected sustained rate | `GET /v1/metrics` output |
| Spike load | ≥150% of expected sustained rate (if safe) | `GET /v1/metrics` output |
| Cooldown | After workload stops | `GET /v1/metrics` output showing queue drain |

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
| `backup_schedule` | Cron expression or systemd timer schedule | _________________________________ |
| `backups_taken_during_pilot` | Number of backups executed during the observation window | _________________________________ |
| `backup_verify_pass_rate` | Percentage of backups where `ferrumctl backup verify` returned OK | _________________________________ |
| `last_backup_timestamp` | Timestamp of most recent backup | _________________________________ |
| `last_restore_drill_timestamp` | Timestamp of most recent restore drill | _________________________________ |
| `restore_drill_result` | OK / FAILED (with reason) | _________________________________ |
| `rpo_accepted_minutes` | Operator-accepted RPO in minutes | _________________________________ |
| `rto_accepted_minutes` | Operator-accepted RTO in minutes | _________________________________ |

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
> **This signoff does NOT authorize P5b–P5e implementation.** G3.5 and Eng.1 are still required.

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

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-11 | Initial G3.6 pilot metrics evidence packet drafted | Engineering |

---

*Document created: 2026-05-11. G3.6 operator-owned evidence packet — NOT complete until operator attaches real pilot data. No production-ready claim. No P5b–P5e implementation authorization.*
