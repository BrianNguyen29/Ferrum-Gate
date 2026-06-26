# Incident Runbook

> **Parent**: [`guides/operator.md`](../guides/operator.md)

---

## 1. Health and readiness checks

| Endpoint | Command | Expected |
|----------|---------|----------|
| Liveness | `curl http://127.0.0.1:18080/v1/healthz` | `{"status":"ok"}` (200) |
| Readiness (shallow) | `curl http://127.0.0.1:18080/v1/readyz` | `{"status":"ready"}` (200) |
| Readiness (deep) | `curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/readyz/deep` | 200 when healthy, 503 when degraded |

> `/v1/readyz/deep` is the only endpoint suitable for load-balancer / Kubernetes readiness probes.

## 2. Metrics checks

```bash
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/metrics
```

Key metrics to alert on:

| Metric | Threshold | Meaning |
|--------|-----------|---------|
| `ferrumgate_store_health_up` | 0 | Store is unhealthy; page immediately |
| `ferrumgate_write_queue_depth` | > 100 | Write saturation; scale to PostgreSQL or reduce burst |
| `ferrumgate_governance_errors_total` | spike | Investigate endpoint or policy errors |
| `ferrumgate_lifecycle_outbox_operator_review` | > 0 | Reconciliation requires manual review |

## 3. Common incident patterns

| Symptom | Likely cause | Action |
|---------|--------------|--------|
| `readyz/deep` 503 | Store unhealthy or queue backpressure | Check store connectivity and load; see [PostgreSQL recovery](#postgresql-recovery) below |
| High `write_queue_depth` | Write saturation | Scale to PostgreSQL or reduce concurrent burst |
| 401 on workload endpoints | Token mismatch or auth mode changed | Verify `FERRUMD_AUTH_MODE` and bearer token |
| 429 rate limited | Governor burst exceeded | Review `rate_limit_per_second` and `rate_limit_burst` |
| Lifecycle outbox `needs_operator_review` | Reconciliation failure or ambiguous provenance | Inspect with `ferrumctl admin lifecycle-outbox` and resolve manually |

## 4. Lifecycle outbox review

See [`guides/operator.md`](../guides/operator.md) § "Lifecycle outbox operator review" for inspection, retry, and resolve commands.

## 5. PostgreSQL recovery

See [`guides/operator.md`](../guides/operator.md) § "PostgreSQL reconnect and recovery" for pool behavior, automatic recovery, and when to restart `ferrumd`.

## 6. Backup and restore

See [`guides/operator.md`](../guides/operator.md) § "Backup and restore".

## 7. Token rotation

See [`guides/operator.md`](../guides/operator.md) § "Token rotation".

## 8. Escalation principles

- **Do not bypass gateway or policy checks.**
- **Do not reuse capabilities.**
- **If the store is unhealthy, fail closed.** Restart only after the root cause is identified.
- **If in doubt, consult [`SCOPE.md`](../SCOPE.md) for honest project boundaries.**

## 9. Future hardening (not yet implemented)

The following controls are proposed but not yet implemented. See the referenced ADRs for design and acceptance criteria.

| Control | ADR | Status |
|---------|-----|--------|
| Audit fail-closed mode | [ADR 007](../adr/007-audit-fail-closed.md) | Proposed |
| R3 approval timeout / second factor | [ADR 008](../adr/008-r3-approval-timeout-mfa.md) | Proposed |
| WORM export and portable audit bundle | [ADR 009](../adr/009-worm-export-audit-bundle.md) | Proposed |
| Behavioral anomaly detection | [ADR 010](../adr/010-behavioral-anomaly-detection.md) | Proposed |
| Performance regression gate | [ADR 011](../adr/011-performance-regression-gate.md) | Proposed |
