# TEMPLATE — Daily SLO Evidence Log

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real observation data.
> See [`docs/implementation-path/artifacts/2026-05-28-phase1.10-slo-window-start-evidence.md`](./2026-05-28-phase1.10-slo-window-start-evidence.md) for the window plan and baseline state.
> See [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) for measurement procedures.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Date** | `YYYY-MM-DD` |
| **Window day** | `N of M` (e.g., `1 of 7` or `1 of 30`) |
| **Operator** | `name` |
| **Environment** | `local / pilot / staging / production-target` |
| **ferrumd backend** | `SQLite / PostgreSQL` |
| **Rate-limit profile** | `default safety / SLO certification / operator-tuned` |
| **Monitoring stack** | `Prometheus version / AlertManager version / Grafana version` |

---

## Daily Metrics

> **Source**: Prometheus query results, Grafana dashboard screenshot, or `scripts/run_slo_sustained_observation.sh` summary.
> If a metric is not applicable to the backend or config, write `N/A` and note why.

| Metric | Value | Source / Query | Notes |
|--------|-------|----------------|-------|
| Readiness probe uptime (%) | | `up{job="ferrumgate"}` or healthz log | |
| Deep readiness probe uptime (%) | | `up{job="ferrumgate"}` or readyz/deep log | |
| p95 latency — evaluate (ms) | | `histogram_quantile(0.95, rate(ferrumgate_request_duration_seconds_bucket{handler="evaluate"}[5m]))` | |
| p95 latency — mint (ms) | | `histogram_quantile(0.95, rate(ferrumgate_request_duration_seconds_bucket{handler="mint"}[5m]))` | |
| p95 latency — execute pipeline (ms) | | `histogram_quantile(0.95, rate(ferrumgate_request_duration_seconds_bucket{handler="execute"}[5m]))` | |
| p99 latency — evaluate (ms) | | `histogram_quantile(0.99, rate(ferrumgate_request_duration_seconds_bucket{handler="evaluate"}[5m]))` | |
| p99 latency — mint (ms) | | `histogram_quantile(0.99, rate(ferrumgate_request_duration_seconds_bucket{handler="mint"}[5m]))` | |
| p99 latency — execute pipeline (ms) | | `histogram_quantile(0.99, rate(ferrumgate_request_duration_seconds_bucket{handler="execute"}[5m]))` | |
| Error rate — 5xx (%) | | `rate(ferrumgate_http_requests_total{status=~"5.."}[5m])` | |
| Error rate — 429 (%) | | `rate(ferrumgate_http_requests_total{status="429"}[5m])` | |
| Throughput (requests/min) | | `rate(ferrumgate_http_requests_total[5m]) * 60` | |
| PG pool active / idle / total | | `ferrumgate_store_pg_pool_active`, `idle`, `max` | N/A if SQLite |
| Backup freshness (hours since last success) | | `time() - backup_last_success_timestamp` or operator log | |
| Replication lag (seconds) | | `pg_stat_replication_pg_wal_lsn_diff` or equivalent | N/A if single-node / SQLite |
| Restart / reconnect events | | server log / systemd journal | |
| Operator intervention events | | operator log | |

---

## Events & Incidents

> Log any event that could affect SLO measurements or indicate a gap in observation.

| Time (UTC) | Event type | Description | Impact on metrics | Operator action |
|------------|------------|-------------|-------------------|-----------------|
| `HH:MM` | `restart / reconnect / config_change / alert_firing / manual_intervention / other` | | | |

---

## Anomalies & Notes

- [ ] *(add any unexpected latency spikes, error bursts, missing metrics, or tooling issues)*

---

## Non-Claims

- **NOT production-ready**: A single daily log does not constitute production readiness.
- **NOT Tier 2**: Tier 2 requires the full sustained window (7–30 days) plus operator signoff.
- **NOT an SLO window closure claim**: This is a per-day data point only.
- **NOT target-host validated by default**: If collected from localhost, note `domainless evidence`.
- **NOT a code defect report**: Anomalies observed here require separate investigation before classification.
- **NOT a substitute for operator signoff**: The final `YYYY-MM-DD-sustained-slo-window-evidence.md` artifact must be reviewed and signed separately.

---

## Signoff (optional daily ack)

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Operator | | | |

---

## Related Docs

- [`docs/implementation-path/artifacts/2026-05-28-phase1.10-slo-window-start-evidence.md`](./2026-05-28-phase1.10-slo-window-start-evidence.md) — Window plan and baseline
- [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) — Draft SLO targets
- [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) — Validation procedure
- [`configs/monitoring/README.md`](../../configs/monitoring/README.md) — Monitoring stack setup
- [`scripts/run_slo_sustained_observation.sh`](../../scripts/run_slo_sustained_observation.sh) — Sustained observation script
