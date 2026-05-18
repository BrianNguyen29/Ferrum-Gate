# SLO/SLA Guide

> **Status**: Scaffold. SLO targets are draft; not yet validated.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Purpose

This guide documents the draft Service Level Objectives (SLOs) for FerrumGate and explains how to validate them. These are **not committed SLAs** until ratified by an operator and backed by evidence.

## Current baselines

SQLite single-node stress test results (post-Phase-1 write queue):

| Scenario | p50 Latency | Error Rate |
|----------|-------------|------------|
| health (GET /v1/healthz) | 1.28ms | 0% |
| auth (GET /v1/approvals) | 3.03ms | 0% |
| provenance-query (POST) | 2.76ms | 0% |
| intent-compile (POST, sync write) | 2.25ms | 0% |
| execution-pipeline (6 steps) | 16.0ms | 0% |
| capability (mint→revoke cycle) | 0.30ms | 0% |
| sqlite-contention (ingest writes, 50 workers) | 29.9ms | 0% |
| mixed workload (5 workers) | 4.80ms | 0% |

> **Caveat**: These are local results. Target-host results may differ.

## Draft SLOs

### Availability

| Endpoint | Pilot target | Single-node PG target | HA target |
|----------|--------------|-----------------------|-----------|
| `/v1/healthz` | 99.0% | 99.5% | 99.9% |
| `/v1/readyz/deep` | 99.0% | 99.5% | 99.9% |

### Latency

| Operation | Pilot p99 | Single-node PG p99 | HA p99 |
|-----------|-----------|--------------------|--------|
| evaluate | < 500ms | < 300ms | < 200ms |
| mint | < 500ms | < 300ms | < 200ms |
| authorize | < 500ms | < 300ms | < 200ms |
| prepare | < 1s | < 500ms | < 300ms |
| execute | < 2s | < 1s | < 500ms |
| verify | < 1s | < 500ms | < 300ms |
| full pipeline | < 5s | < 3s | < 2s |

### Error rate

| Metric | Pilot | Single-node PG | HA |
|--------|-------|----------------|----|
| 5xx rate | < 1% | < 0.5% | < 0.1% |
| 429 rate | < 5% | < 2% | < 1% |

### Durability

| Metric | Target |
|--------|--------|
| Backup age | < 15 minutes |
| Restore success | 100% |
| RPO | 15 minutes |
| RTO | 15 minutes |

### Correctness

| Metric | Target |
|--------|--------|
| Capability bypass | 0 |
| Provenance gap | 0 |
| Scope violation | 0 |

### Security

| Metric | Target |
|--------|--------|
| Auth bypass | 0 |
| Secret leak in output/logs | 0 |

### Operational

| Metric | Pilot | Single-node PG | HA |
|--------|-------|----------------|----|
| Incident acknowledgement | < 1h | < 30min | < 15min |

## Validation runbook

### Prechecks

1. Confirm target hardware specs match or exceed local test environment.
2. Verify store backend (SQLite or PostgreSQL) is healthy.
3. Confirm auth mode and bearer token are configured.
4. Verify `/v1/readyz/deep` returns 200.

### Workload phases

```
baseline  600s  (low load, establish metrics)
low       600s  (ramp up)
target   1800s  (sustained target load)
spike     300s  (burst above target)
cooldown  600s  (return to baseline)
```

### Pass/fail criteria

- p99 latency for each operation must be under threshold.
- 5xx rate must be under threshold.
- 429 rate must be under threshold.
- Readiness success rate must be >= availability target.
- No capability bypass, provenance gap, or scope violation observed.

### Evidence artifact

Each run must produce:

- `metrics-before.json` — `/v1/metrics` scrape before workload
- `metrics-during.json` — scrape during target phase
- `metrics-after.json` — scrape after cooldown
- `latency-report.md` — p50/p95/p99 per operation
- `error-report.md` — error counts and rates
- `runbook-checklist.md` — signed pass/fail per gate

## Status caveat

> **production-ready = NO**. These SLOs are draft targets. No target-host sustained workload has been run against them. Do not cite as committed SLAs. See [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md).

## Related docs

- [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) — Full SLO draft and runbook plan.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Stress test baselines.
- [`operator.md`](./operator.md) — Monitoring and alerting guidance.
