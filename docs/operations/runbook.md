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
| Audit fail-closed mode | [ADR 007](../adr/007-audit-fail-closed.md) | Accepted |
| R3 approval timeout / second factor | [ADR 008](../adr/008-r3-approval-timeout-mfa.md) | Proposed |
| WORM export and portable audit bundle | [ADR 009](../adr/009-worm-export-audit-bundle.md) | Proposed |
| Behavioral anomaly detection | [ADR 010](../adr/010-behavioral-anomaly-detection.md) | Accepted (Phase 1 V1) |
| Performance regression gate | [ADR 011](../adr/011-performance-regression-gate.md) | Accepted |

## 10. Alert runbook anchors

> Stable anchors consumed by `configs/monitoring/ferrumgate-alerts.yaml`
> `runbook_url` annotations. Each alert id maps to the most relevant existing
> section above; the deep link is `<repo>/docs/operations/runbook.md#<anchor>`.
> Operators running standalone Prometheus may repoint the URLs to a hosted copy
> of this runbook (see the alerts file header note).

<a id="ferrumgate-down"></a>
- **FerrumGateDown** — `ferrumd` unreachable. See [§1 Health and readiness checks](#1-health-and-readiness-checks) and [§8 Escalation principles](#8-escalation-principles); verify process, port, and store before restart.

<a id="ferrumgate-store-unhealthy"></a>
- **FerrumGateStoreUnhealthy** — store health check failing. See [§1 Health and readiness checks](#1-health-and-readiness-checks) and [§5 PostgreSQL recovery](#5-postgresql-recovery); fail closed until root cause is identified.

<a id="ferrumgate-queue-high"></a>
- **FerrumGateWriteQueueHigh** — write queue depth above threshold. See [§2 Metrics checks](#2-metrics-checks) and [§3 Common incident patterns](#3-common-incident-patterns); scale to PostgreSQL or reduce burst.

<a id="ferrumgate-queue-full"></a>
- **FerrumGateWriteQueueFull** — write queue blocking writes. See [§3 Common incident patterns](#3-common-incident-patterns); immediate action: shed load and recover the store before draining.

<a id="ferrumgate-high-error-rate"></a>
- **FerrumGateHighErrorRate** — elevated 5xx rate. See [§2 Metrics checks](#2-metrics-checks); correlate by route and inspect recent governance errors.

<a id="ferrumgate-high-latency"></a>
- **FerrumGateHighLatency** — p99 latency above threshold. See [§2 Metrics checks](#2-metrics-checks); check store latency and write-queue depth.

<a id="ferrumgate-cap-mint-failures"></a>
- **FerrumGateCapabilityMintFailures** — capability mint errors elevated. See [§2 Metrics checks](#2-metrics-checks) and [§8 Escalation principles](#8-escalation-principles); never bypass the gateway or reuse capabilities.

<a id="ferrumgate-policy-eval-failures"></a>
- **FerrumGatePolicyEvalFailures** — policy evaluation errors elevated. See [§2 Metrics checks](#2-metrics-checks); inspect policy bundle and PDP connectivity.

<a id="ferrumgate-execution-failures"></a>
- **FerrumGateExecutionFailures** — execution errors elevated. See [§2 Metrics checks](#2-metrics-checks) and [§4 Lifecycle outbox review](#4-lifecycle-outbox-review).

<a id="ferrumgate-rollback-failures"></a>
- **FerrumGateRollbackFailures** — compensate/rollback errors elevated (critical). See [§4 Lifecycle outbox review](#4-lifecycle-outbox-review) and [§8 Escalation principles](#8-escalation-principles); compensations may not complete — investigate before any restart.

<a id="ferrumgate-provenance-gaps"></a>
- **FerrumGateProvenanceGaps** — lifecycle outbox records need operator review. See [§4 Lifecycle outbox review](#4-lifecycle-outbox-review); inspect with `ferrumctl admin lifecycle-outbox` and resolve manually.

<a id="ferrumgate-disk-space-low"></a>
- **FerrumGateDiskSpaceLow** — `/var/lib/ferrumgate` below 10% free. Free space or expand the volume before store writes fail; see [§8 Escalation principles](#8-escalation-principles).

<a id="ferrumgate-pg-down"></a>
- **FerrumGatePostgresMetricsAbsent** — PG pool metrics absent (connection lost or wrong backend). See [§5 PostgreSQL recovery](#5-postgresql-recovery).

<a id="ferrumgate-pg-pool-saturation"></a>
- **FerrumGatePostgresPoolSaturation** — PG pool has 0 idle connections at max. See [§5 PostgreSQL recovery](#5-postgresql-recovery); raise pool size or reduce concurrency.

<a id="ferrumgate-pg-slow-acquire"></a>
- **FerrumGatePostgresSlowAcquire** — PG connection acquire timeouts. See [§5 PostgreSQL recovery](#5-postgresql-recovery); pool may be undersized or PG slow/unreachable.

<a id="ferrumgate-pg-replication-lag"></a>
- **FerrumGatePostgresReplicationLag** — TEMPLATE; HA/replication not deployed by default. Enable only with postgres_exporter metrics; see ADR/HA docs before relying on this alert.

<a id="ferrumgate-high-cpu"></a>
- **FerrumGateHighCPU** — instance CPU above 80% for 10m. Profile hot routes and store load; see [§2 Metrics checks](#2-metrics-checks).

<a id="ferrumgate-high-memory"></a>
- **FerrumGateHighMemory** — instance memory above 85% for 10m. Check store cache and queue growth; see [§2 Metrics checks](#2-metrics-checks).
