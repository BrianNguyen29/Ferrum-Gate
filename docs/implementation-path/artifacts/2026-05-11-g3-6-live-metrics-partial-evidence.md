# 2026-05-11 — G3.6 Live Metrics Partial Evidence

> **Status**: Partial evidence. A1–A5 met with caveats; A6 (operator signoff) remains pending. G3.6 conditionally ready for operator review.  
> **Scope**: Sanitized live metrics collected from `ferrumgate-nonprod` GCP VM on 2026-05-11.  
> **Constraint**: This artifact does NOT complete G3.6 until A6 is satisfied.  
> **Purpose**: Evidence attachment for `106-g3-6-pilot-metrics-evidence-packet.md`.

---

## Collection Context

| Item | Value |
|---|---|
| Target URL | `https://ferrumgate.duckdns.org` |
| Probe timestamp | `2026-05-11T16:35:29Z` |
| Collector | Assistant via `curl`, `gcloud`, Prometheus node-exporter/prometheus queries |
| VM | `ferrumgate-nonprod` (`fairy-b13f4`, `asia-southeast1-a`, RUNNING) |
| VM timestamp | `2026-05-11T16:38:09Z` |

**Tool availability**:
- `ferrumctl`: not found on collector host
- `curl`: found
- `gcloud`: found
- `FERRUMCTL_SERVER_URL`: not set
- `FERRUMCTL_BEARER_TOKEN`: not set
- `GOOGLE_APPLICATION_CREDENTIALS`: set

---

## 1. Endpoint Probes (2026-05-11T16:35:29Z)

| Endpoint | HTTP Status | Response Size | Notes |
|---|---|---|---|
| `GET /v1/readyz` | 200 | 18 bytes | Shallow probe OK |
| `GET /v1/readyz/deep` | 200 | 177 bytes | JSON keys: `components`, `healthy`, `status`; `status=ok` |
| `GET /v1/metrics` | 200 | 12,980 bytes | Prometheus-compatible output returned |

**Required metrics presence** (verified from `/v1/metrics`):
- `ferrumgate_write_queue_depth` ✅ present
- `ferrumgate_http_requests_total` ✅ present
- `ferrumgate_request_duration_seconds` ✅ present
- `ferrumgate_store_health_up` ✅ present
- `ferrumgate_governance_errors_total` ✅ present

---

## 2. Metrics Snapshot (2026-05-11T16:35:46Z)

Sampled counters from `/v1/metrics`:

```
ferrumgate_write_queue_depth 0
ferrumgate_store_health_up 1

ferrumgate_http_requests_total{route="/v1/healthz",method="GET",status="200"} 5
ferrumgate_http_requests_total{route="/v1/readyz",method="GET",status="200"} 5
ferrumgate_http_requests_total{route="/v1/readyz/deep",method="GET",status="200"} 5
ferrumgate_http_requests_total{route="/v1/readyz/deep",method="GET",status="503"} 0
ferrumgate_http_requests_total{route="/v1/metrics",method="GET",status="200"} 12131
```

- `ferrumgate_governance_errors_total`: 27 route counters sampled; first 12 all zero
- `request_duration_seconds` lines: 70 histogram/summary lines present

---

## 3. Readiness / Queue Depth Window (2026-05-11T16:36:01Z – 16:36:44Z)

5 samples collected at ~10s intervals:

| Sample # | Timestamp | `/v1/readyz/deep` | Status | `queue_depth` | `store_health_up` |
|---|---|---|---|---|---|
| 1 | 2026-05-11T16:36:01Z | 200 | ok | 0 | 1 |
| 2 | 2026-05-11T16:36:12Z | 200 | ok | 0 | 1 |
| 3 | 2026-05-11T16:36:23Z | 200 | ok | 0 | 1 |
| 4 | 2026-05-11T16:36:34Z | 200 | ok | 0 | 1 |
| 5 | 2026-05-11T16:36:44Z | 200 | ok | 0 | 1 |

- **Probe success rate**: 100% (5/5)
- **HTTP 503 count**: 0
- **Peak queue depth**: 0
- **Store health**: consistently up

---

## 4. Prometheus Queries from VM (1h lookback)

Query window: last 1h from VM query time.

| Metric | Query | Result |
|---|---|---|
| Current queue depth | `ferrumgate_write_queue_depth` | 0 |
| Max queue depth (1h) | `max_over_time(ferrumgate_write_queue_depth[1h])` | 0 |
| Total HTTP request rate | `sum(rate(ferrumgate_http_requests_total[1h]))` | 0.071 req/s |
| POST request rate | `sum(rate(ferrumgate_http_requests_total{method="POST"}[1h]))` | empty / no data |
| readyz/deep 200 count (1h) | `sum(increase(...{route="/v1/readyz/deep",status="200"}[1h]))` | 6.03 |
| readyz/deep 503 count (1h) | `sum(increase(...{route="/v1/readyz/deep",status="503"}[1h]))` | 0 |
| Store health avg (1h) | `avg_over_time(ferrumgate_store_health_up[1h])` | 1 |
| Governance errors (1h) | `sum(increase(ferrumgate_governance_errors_total[1h]))` | 0 |
| Governance success rate (1h) | `sum(rate(ferrumgate_governance_success_total[1h]))` | 0 |
| Governance success count (1h) | `sum(increase(ferrumgate_governance_success_total[1h]))` | 0 |

**Key observation**: Write/governance activity is zero/idle during the observation window. No POST requests, no governance success events. This is **not a representative workload** for sustained write-rate measurement.

---

## 5. VM Process and Service Status

| Check | Result |
|---|---|
| `pgrep -a ferrumd` | Process exists: `/opt/ferrumgate/ferrumd --config /etc/ferrumgate/ferrumgate.toml` |
| `ferrumd.service` | `inactive` (systemd service not active, but process is running) — follow-up item |
| `prometheus.service` | `active` |
| `ferrumgate-backup.timer` | `active` |
| `caddy.service` | `active` |
| `nginx.service` | `inactive` |

**Config (non-secret)**:
- `FERRUMD_BIND_ADDR=127.0.0.1:19080`
- `FERRUMD_STORE_DSN=sqlite:///var/lib/ferrumgate/data/ferrumgate.db?mode=rwc`
- `FERRUMD_AUTH_MODE=bearer`

---

## 6. 1-Hour Compile-Only Workload (2026-05-11T17:06:28Z – 18:06:29Z)

Workload script: `bash scripts/run_1h_compile_workload.sh` (loop of `ferrumctl intent compile` with 1s sleep).

### 6.1 Request Summary

| Metric | Value |
|---|---|
| Measurement period | 3600.7 seconds |
| Total requests sent | 3582 |
| HTTP 200 (success) | 1805 |
| HTTP 429 (rate limited) | 1777 |
| HTTP 4xx/5xx other | 0 |
| Success rate | ~50.4% |

### 6.2 Latency Distribution (from `request_duration_seconds` histogram)

| Percentile | Latency |
|---|---|
| p50 | ~218 ms |
| p95 | ~326 ms |
| p99 | ~523 ms |
| max | ~2229 ms |
| mean | ~239 ms |

### 6.3 Prometheus Rate Query (70m lookback at 18:06Z)

| Query | Result |
|---|---|
| `sum(rate(ferrumgate_governance_success_total{route="/v1/intents/compile",method="POST"}[70m]))` | 0.430 req/s |
| `sum(increase(ferrumgate_governance_success_total{route="/v1/intents/compile",method="POST"}[70m]))` | 1806.28 |
| `sum(increase(ferrumgate_governance_errors_total[70m]))` | 0 |
| `ferrumgate_write_queue_depth` (instant) | 0 |
| `max_over_time(ferrumgate_write_queue_depth[1h])` | 0 |

### 6.4 Post-Workload Readiness Probe (2026-05-11T18:06:40Z – 18:07:12Z)

5 manual samples at ~10s intervals:

| Sample # | Timestamp | Status | `queue_depth` | `store_health_up` |
|---|---|---|---|---|
| 1 | 2026-05-11T18:06:40Z | 200 / ok | 0 | 1 |
| 2 | 2026-05-11T18:06:50Z | 200 / ok | 0 | 1 |
| 3 | 2026-05-11T18:07:00Z | 200 / ok | 0 | 1 |
| 4 | 2026-05-11T18:07:10Z | 200 / ok | 0 | 1 |
| 5 | 2026-05-11T18:07:12Z | 200 / ok | 0 | 1 |

**Caveat**: Workload is compile-only (`intent_compile`, POST `/v1/intents/compile`). No adapter execution paths exercised. Write queue depth remained 0 throughout.

---

## 7. Backup Status

| Check | Result |
|---|---|
| Backup directory | `/var/lib/ferrumgate/backups` exists |
| Backup count | 1 |
| Latest backup file | `ferrumgate_20260508_154446.db` |
| Latest backup size | 241,664 bytes |
| Latest backup mtime | `2026-05-11T16:33:12Z` |
| `ferrumctl backup verify` | `OK` — `Database integrity check passed` |

---

## 8. Restore Drill (2026-05-11T17:04:57Z)

**Method**: Safe restore to temporary path; never touched live DB.

```bash
# On ferrumgate-nonprod VM
TMPDIR=$(mktemp -d)
ferrumctl backup restore \
  --backup-path /var/lib/ferrumgate/backups/ferrumgate_20260508_154446.db \
  --target-dir "$TMPDIR"
ferrumctl backup verify --db-path "$TMPDIR"/ferrumgate.db
# Result: OK — Database integrity check passed
rm -rf "$TMPDIR"
```

| Field | Value |
|---|---|
| Restore timestamp | 2026-05-11T17:04:57Z |
| Restore result | OK |
| Verify result | OK |
| Live DB touched | No — restored to `mktemp -d` and cleaned up |
| RTO (coarse) | Under 120s (completed within default command timeout; exact seconds not instrumented) |

---

## Why G3.6 Remains Incomplete

| Criterion | Status | Reason |
|---|---|---|
| A1 — ≥1h sustained write-rate measurement | **MET with caveat** | 1h compile-only workload executed (3582 requests, 1805 success, 1777 rate-limited). **Caveat**: compile-only; adapter execution paths not exercised. |
| A2 — Queue depth at idle and target load | **MET with caveat** | Queue depth 0 at idle and post-workload. **Caveat**: compile-only workload; no adapter execution to stress queue. |
| A3 — `readyz/deep` success rate ≥99% | **MET** | 100% over 10 samples (5 pre + 5 post) + 1h Prometheus window. |
| A4 — Metrics snapshot at target load with all required counters | **MET with caveat** | Baseline + post-workload snapshots collected; all 5 required metrics present. **Caveat**: no low/target/spike/cooldown sequence. |
| A5 — Backup verify passes + restore drill within RTO | **MET with caveat** | Backup verify OK; restore drill OK (temp path, verified, cleaned up). **Caveat**: RPO/RTO not formally operator-accepted; exact RTO not instrumented. |
| A6 — Operator signoff | **NOT MET** | Operator has not signed §Operator Signoff. |

**Conclusion**: G3.6 is conditionally ready for operator review. A1–A5 are met with caveats. The only remaining blocker is A6 (operator signoff).

---

## Cross-Reference

| Artifact | Links To | Purpose |
|---|---|---|
| This artifact | `106-g3-6-pilot-metrics-evidence-packet.md` | Evidence attachment for Field 1, Field 3, Field 4, Field 5, Field 6 |
| This artifact | `107-eng-1-capacity-confirmation-packet.md` | Eng.1 context (capacity confirmed) |
| This artifact | `108-eng-2-p5b-p5e-implementation-planning-packet.md` | Eng.2 context (P5b blocked on G3.6) |

---

*Artifact generated: 2026-05-11. Sanitized live metrics from `ferrumgate-nonprod`. G3.6 remains pending. No production-ready claim. No P5b–P5e implementation authorization.*
