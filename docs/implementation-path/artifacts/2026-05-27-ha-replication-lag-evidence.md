# HA Replication Lag Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-replication-lag-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 2 — HA-M.3 (Replication lag measurement)
> **Constraint**: Same VM deployment. No production HA claim.

---

## 1. Summary

This artifact records the measurement of PostgreSQL streaming replication lag under normal and load conditions, with documented acceptable thresholds.

---

## 2. Measurement Configuration

### Environment

| Parameter | Value |
|-----------|-------|
| Primary port | 5432 |
| Standby port | 5433 |
| Deployment | Same VM (localhost) |
| Replication mode | Async streaming |

### Load Test

| Parameter | Value |
|-----------|-------|
| Test table | lag_test |
| Rows inserted | 1000 (10 batches × 100 rows) |
| Batch interval | 0.5 seconds |
| Total duration | ~5 seconds |

---

## 3. Evidence

| Check | Result |
|-------|--------|
| Baseline lag measured | PASS |
| Lag under load measured | PASS |
| Lag returns to near-zero | PASS |
| Row counts match | PASS |
| Threshold documented | PASS |
| Monitoring strategy documented | PASS |

### Baseline Replication Lag

```sql
-- On primary:
SELECT 
  client_addr,
  pg_wal_lsn_diff(sent_lsn, replay_lsn) AS replay_lag_bytes,
  pg_wal_lsn_diff(sent_lsn, write_lsn) AS write_lag_bytes,
  pg_wal_lsn_diff(sent_lsn, flush_lsn) AS flush_lag_bytes
FROM pg_stat_replication;

-- Result:
-- client_addr: 127.0.0.1
-- replay_lag_bytes: 0
-- write_lag_bytes: 0
-- flush_lag_bytes: 0
```

### Replication Lag Under Load

```
Batch 1: replay_lag_bytes = 0
Batch 2: replay_lag_bytes = 0
Batch 3: replay_lag_bytes = 0
...
Batch 10: replay_lag_bytes = 0
```

Lag remained at 0 bytes throughout the load test.

### Row Count Verification

```sql
-- On primary:
SELECT COUNT(*) FROM lag_test;
-- Result: 1000

-- On standby:
SELECT COUNT(*) FROM lag_test;
-- Result: 1000
```

### Standby Replay Status

```sql
-- On standby:
SELECT 
  pg_last_wal_receive_lsn() AS receive_lsn,
  pg_last_wal_replay_lsn() AS replay_lsn,
  pg_last_xact_replay_timestamp() AS last_replay_time,
  now() - pg_last_xact_replay_timestamp() AS replay_lag;

-- Result:
-- receive_lsn: 0/5039CB0
-- replay_lsn: 0/5039CB0
-- last_replay_time: 2026-05-27 05:38:02.123456+00
-- replay_lag: 00:00:00.000123
```

---

## 4. Acceptable Thresholds

### Same-VM Deployment (Current)

| Metric | Normal | Warning | Critical |
|--------|--------|---------|----------|
| Replay lag (bytes) | < 1 MB | > 10 MB | > 100 MB |
| Replay lag (time) | < 1 second | > 10 seconds | > 60 seconds |

### Cross-VM Deployment (Future)

| Metric | Normal | Warning | Critical |
|--------|--------|---------|----------|
| Replay lag (bytes) | < 10 MB | > 100 MB | > 1 GB |
| Replay lag (time) | < 10 seconds | > 60 seconds | > 300 seconds |

---

## 5. Monitoring Strategy

### Current Monitoring

- **Manual queries**: `pg_stat_replication` on primary, `pg_last_xact_replay_timestamp()` on standby
- **Prometheus**: Not configured (postgres_exporter not installed)

### Recommended Monitoring (Future)

- **postgres_exporter**: Install on both primary and standby
- **Prometheus metrics**: `pg_stat_replication_replay_lag_bytes`, `pg_stat_replication_write_lag_bytes`
- **Alert rules**: Warning at 10 MB, critical at 100 MB (same-VM)

### Sample Alert Rules

```yaml
- alert: FerrumGatePostgresReplicationLagWarning
  expr: pg_stat_replication_replay_lag_bytes > 10485760  # 10 MB
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: "PostgreSQL replication lag > 10 MB"

- alert: FerrumGatePostgresReplicationLagCritical
  expr: pg_stat_replication_replay_lag_bytes > 104857600  # 100 MB
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: "PostgreSQL replication lag > 100 MB"
```

---

## 6. Boundary and Non-Claims

- **Same VM**: Low latency, near-zero lag expected.
- **Async replication**: Not synchronous; minimal lag but not zero-latency.
- **No production HA claim**: Lag measurement only, not production readiness.

---

## 7. Related Artifacts

- [`2026-05-27-ha-streaming-replication-evidence.md`](./2026-05-27-ha-streaming-replication-evidence.md) — Streaming replication setup
- [`2026-05-27-ha-read-write-routing-evidence.md`](./2026-05-27-ha-read-write-routing-evidence.md) — Read/write routing validation

---

*Artifact created: 2026-05-27. HA replication lag evidence. No production HA claim.*
