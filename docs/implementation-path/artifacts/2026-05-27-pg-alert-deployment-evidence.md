# PostgreSQL Alert Deployment Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-pg-alert-deployment-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 1 — PG-P.6 (Alert rules deployed to Prometheus)
> **Constraint**: Target VM Prometheus. No production-ready claim.

---

## 1. Summary

This artifact records the successful deployment of PostgreSQL-specific alert rules to Prometheus, with ferrumd metrics scraping configured.

---

## 2. Prometheus Configuration

### Scrape Config

```yaml
- job_name: 'ferrumgate-ferrumd'
  static_configs:
    - targets: ['localhost:19080']
  metrics_path: '/v1/metrics'
  scrape_interval: 15s
```

### Alert Rules File

Location: `/etc/prometheus/rules/ferrumgate-postgres-alerts.yml`

---

## 3. Alert Rules Deployed

| Alert Name | Severity | Condition | For |
|------------|----------|-----------|-----|
| FerrumGatePostgresMetricsAbsent | critical | absent(ferrumgate_store_pg_pool_max) == 1 | 2m |
| FerrumGatePostgresPoolSaturation | warning | pool_idle == 0 AND pool_size >= pool_max | 1m |
| FerrumGatePostgresSlowAcquire | warning | rate(acquire_timeouts_total[5m]) > 0 | 2m |
| FerrumGatePostgresBackupStale | warning | (time() - backup_last_success) > 1800 | 5m |
| FerrumGateStoreUnhealthy | critical | store_health_up == 0 | 1m |

---

## 4. Evidence

| Check | Result |
|-------|--------|
| Prometheus scrape config added | PASS |
| Alert rules file created | PASS |
| Alert rules syntax valid | PASS (promtool: 5 rules) |
| Prometheus scraping ferrumd | PASS (target UP) |
| PG metrics queryable | PASS |
| Alert rules loaded | PASS |
| Alerts inactive (healthy state) | PASS |

### Prometheus Service Status

```
Active: active (running) since Wed 2026-05-27 05:04:12 UTC
```

### Target Health

```json
{
  "job": "ferrumgate-ferrumd",
  "url": "http://localhost:19080/v1/metrics",
  "health": "up",
  "lastScrape": "2026-05-27T05:05:33.386495616Z"
}
```

### PG Metrics Query

```
ferrumgate_store_pg_pool_max = 10
```

### Alert Rules Loaded

```
Group: ferrumgate_postgres
Rules: 5 (all inactive)
```

### Alert States

All 5 alerts are **inactive** (PostgreSQL is healthy).

---

## 5. Boundary and Non-Claims

- **Target VM Prometheus**: Not a production monitoring stack.
- **5 PG-specific alerts**: Complement existing ferrumgate alert groups.
- **No production-ready claim**: Alert deployment validated on target VM only.

---

## 6. Related Artifacts

- [`2026-05-27-pg-restore-drill-evidence.md`](./2026-05-27-pg-restore-drill-evidence.md) — Backup/restore drill
- [`2026-05-27-pg-production-deployment-signoff.md`](./2026-05-27-pg-production-deployment-signoff.md) — Consolidated signoff

---

*Artifact created: 2026-05-27. PostgreSQL alert deployment evidence. No production-ready claim.*
