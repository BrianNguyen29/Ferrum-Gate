# SLO/SLA Guide

> **Status**: Expanded guide. Canonical SLO evidence and rate-limit profiles incorporated.
> **Parent**: [`guides/README.md`](./README.md)

---

## Purpose

This guide documents the draft Service Level Objectives (SLOs) for FerrumGate and explains how to validate them. These are **not committed SLAs** until ratified by an operator and backed by evidence.

## SLI / SLO definitions

### Service Level Indicators (SLIs)

| SLI | Measurement |
|-----|-------------|
| Availability | Ratio of successful `healthz` / `readyz/deep` probes over a window |
| Latency | p50/p95/p99 request duration per operation from `/v1/metrics` |
| Error rate | Ratio of 5xx and 429 responses to total requests |
| Durability | Backup age, restore success, row-count/hash parity |
| Correctness | Capability bypass count, provenance gap count, scope violation count |
| Security | Auth bypass count, secret leak in output/logs count |
| Operational | Incident acknowledgement time |

### Draft SLOs

#### Availability

| Endpoint | Pilot target | Single-node PG target | HA target |
|----------|--------------|-----------------------|-----------|
| `/v1/healthz` | 99.0% | 99.5% | 99.9% |
| `/v1/readyz/deep` | 99.0% | 99.5% | 99.9% |

#### Latency

| Operation | Pilot p99 | Single-node PG p99 | HA p99 |
|-----------|-----------|--------------------|--------|
| evaluate | < 500ms | < 300ms | < 200ms |
| mint | < 500ms | < 300ms | < 200ms |
| authorize | < 500ms | < 300ms | < 200ms |
| prepare | < 1s | < 500ms | < 300ms |
| execute | < 2s | < 1s | < 500ms |
| verify | < 1s | < 500ms | < 300ms |
| full pipeline | < 5s | < 3s | < 2s |

#### Error rate

| Metric | Pilot | Single-node PG | HA |
|--------|-------|----------------|----|
| 5xx rate | < 1% | < 0.5% | < 0.1% |
| 429 rate | < 5% | < 2% | < 1% |

#### Durability

| Metric | Target |
|--------|--------|
| Backup age | < 15 minutes |
| Restore success | 100% |
| RPO | 15 minutes |
| RTO | 15 minutes |

#### Correctness

| Metric | Target |
|--------|--------|
| Capability bypass | 0 |
| Provenance gap | 0 |
| Scope violation | 0 |

#### Security

| Metric | Target |
|--------|--------|
| Auth bypass | 0 |
| Secret leak in output/logs | 0 |

#### Operational

| Metric | Pilot | Single-node PG | HA |
|--------|-------|----------------|----|
| Incident acknowledgement | < 1h | < 30min | < 15min |

## Canonical SLO evidence summary

On 2026-05-21, a canonical target-host SLO certification was attempted with three rate-limit configurations:

| Run | Config | 429 rate | Result |
|-----|--------|----------|--------|
| #1 | Default `2/50` | 46.8% | **FAIL** |
| #2 | Tuned `20/500` | 73.4% | **FAIL** |
| #3 | Max-valid `1000/10000` | 0% | **PASS** |

**Decision**: Default and tuned configurations are intentionally conservative and remain unchanged. SLO certification requires explicit high-throughput profile selection. Operator must tune based on real traffic and IP distribution.

Evidence:
- [`docs/operations/rate-limit-tuning-guide.md`](../../operations/rate-limit-tuning-guide.md)

## Rate-limit profiles

| Profile | `rate_limit_per_second` | `rate_limit_burst` | When to use |
|---------|------------------------|--------------------|-------------|
| **Default safety** | 2 | 50 | Low-traffic pilots, local development, accidental-overload protection |
| **SLO certification** | 1000 | 10000 | Canonical five-phase SLO validation workload |
| **Production / operator-tuned** | TBD | TBD | Real deployments; derive from observed per-IP traffic and backend capacity |

> **Do not** claim SLO certification unless you explicitly used the SLO-certification profile or a validated operator-tuned equivalent.

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

## Status caveat

> **production-ready = NO**. These SLOs are draft targets. No sustained SLO window (7–30 days) has been observed. Do not cite as committed SLAs.

## Non-claims

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** |
| **full G2** | **NOT COMPLETE** |
| **Block A** | **WAIVED/CONDITIONAL** |
| **Tier 2** | **NOT COMPLETE** |
| **sustained SLO window** | **NO** |
| **default-config SLO certification** | **FAIL** — only max-valid config passed |

## Related docs

- [`docs/operations/rate-limit-tuning-guide.md`](../operations/rate-limit-tuning-guide.md) — Rate limit tuning guide.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Stress test baselines.
- [`operator.md`](./operator.md) — Monitoring and alerting guidance.
- [`docs/operations/rate-limit-tuning-guide.md`](../../operations/rate-limit-tuning-guide.md) — Rate-limit profile selection.
