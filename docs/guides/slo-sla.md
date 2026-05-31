# Service Metrics Guide

> **Parent**: [`guides/README.md`](./README.md)

---

## Purpose

This guide documents the metrics and indicators available for FerrumGate and explains how to observe them. These are observability references, not committed service agreements.

## Metrics categories

| Category | Measurement |
|----------|-------------|
| Availability | Ratio of successful `healthz` / `readyz/deep` probes over a window |
| Latency | p50/p95/p99 request duration per operation from `/v1/metrics` |
| Error rate | Ratio of 5xx and 429 responses to total requests |
| Durability | Backup age, restore success, row-count/hash parity |
| Correctness | Capability bypass count, provenance gap count, scope violation count |
| Security | Auth bypass count, secret leak in output/logs count |
| Operational | Incident acknowledgement time |

## Reference thresholds

### Availability

| Endpoint | Reference |
|----------|-----------|
| `/v1/healthz` | 99.0%–99.9% |
| `/v1/readyz/deep` | 99.0%–99.9% |

### Latency

| Operation | Reference p99 |
|-----------|---------------|
| evaluate | < 200ms–500ms |
| mint | < 200ms–500ms |
| authorize | < 200ms–500ms |
| prepare | < 300ms–1s |
| execute | < 500ms–2s |
| verify | < 300ms–1s |
| full pipeline | < 2s–5s |

### Error rate

| Metric | Reference |
|--------|-----------|
| 5xx rate | < 0.1%–1% |
| 429 rate | < 1%–5% |

### Durability

| Metric | Reference |
|--------|-----------|
| Backup age | < 15 minutes |
| Restore success | 100% |
| RPO | 15 minutes |
| RTO | 15 minutes |

### Correctness

| Metric | Reference |
|--------|-----------|
| Capability bypass | 0 |
| Provenance gap | 0 |
| Scope violation | 0 |

### Security

| Metric | Reference |
|--------|-----------|
| Auth bypass | 0 |
| Secret leak in output/logs | 0 |

### Operational

| Metric | Reference |
|--------|-----------|
| Incident acknowledgement | < 15min–1h |

## Rate-limit profiles

| Profile | `rate_limit_per_second` | `rate_limit_burst` | When to use |
|---------|------------------------|--------------------|-------------|
| **Default safety** | 2 | 50 | Low-traffic local development, accidental-overload protection |
| **High-throughput** | 1000 | 10000 | Load validation, stress testing, or high-traffic deployments |
| **Deployment-specific** | TBD | TBD | Derive from observed per-IP traffic and backend capacity |

## Validation procedure

### Prechecks

1. Confirm target hardware specs match or exceed local test environment.
2. Verify store backend (SQLite or PostgreSQL) is healthy.
3. Confirm auth mode and bearer token are configured.
4. Verify `/v1/readyz/deep` returns 200.

### Workload steps

```
baseline  600s  (low load, establish metrics)
low       600s  (ramp up)
target   1800s  (sustained load)
spike     300s  (burst above target)
cooldown  600s  (return to baseline)
```

### Validation criteria

- p99 latency for each operation must be under threshold.
- 5xx rate must be under threshold.
- 429 rate must be under threshold.
- Readiness success rate must be >= availability threshold.
- No capability bypass, provenance gap, or scope violation observed.

### Evidence artifact

Each run should produce:

- `metrics-before.json` — `/v1/metrics` scrape before workload
- `metrics-during.json` — scrape during target step
- `metrics-after.json` — scrape after cooldown
- `latency-report.md` — p50/p95/p99 per operation
- `error-report.md` — error counts and rates
- `runbook-checklist.md` — checks per gate

### Quick validation commands

```bash
# Health
curl http://localhost:8080/v1/healthz
curl http://localhost:8080/v1/readyz/deep

# Pool saturation (PostgreSQL)
curl -s http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_pool_idle
curl -s http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_acquire_timeouts_total

# Rate-limit errors
curl -s http://localhost:8080/v1/metrics | grep 'ferrumgate_governance_errors_total{status="429"}'
```

## Notes

> These thresholds are reference values. Tune for your deployment.

## Related docs

- [`docs/operations/rate-limit-tuning-guide.md`](../operations/rate-limit-tuning-guide.md) — Rate limit tuning guide.
- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Stress test baselines.
- [`operator.md`](./operator.md) — Monitoring and alerting guidance.
