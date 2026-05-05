# 21 — v1 Single-Node Observability Minimums

FerrumGate v1 single-node observability guide. Practical operator reference for
logs, probes, derived signals, and minimum alert thresholds.

**Scope**: single-node, SQLite-backed, v1 only.
**Audience**: operators, on-call engineers, SREs.
**Last updated**: 2026-04-28.

---

## 1. Title and Scope

**Title**: FerrumGate v1 Single-Node Observability Minimums

**Scope**: This document describes the minimum observability surface available to
operators of a FerrumGate v1 single-node deployment. It covers logs that
exist today, probe-based signals, derived signals, known blind spots, and
conservative escalation thresholds.

This document is **operator-facing and practical**. It describes the bounded
built-in metrics surface available in the current code and the blind spots that
still require external tooling.

---

## 2. Boundary Note

For support scope, limits, and known caveats, see the canonical support
contract:

[19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

For CLI-first operator checks and verification ladders (startup ladder, daily checks, incident triage), see:

[20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)

For deployment, backup, restore, and recovery procedures, see:

[18-single-node-operations-runbook.md](./18-single-node-operations-runbook.md)

---

## 3. What Exists Today

### 3.1 Logs

The `ferrumd` process emits structured logs via the `tracing` crate.  Log
output goes to stdout (no built-in logrotate or file rotation in v1).

Log levels controlled via `--log-filter` (or `FERRUMD_LOG_FILTER`):

| Level | Includes |
|---|---|
| `error` | Error-level events only |
| `warn`  | Warnings and errors |
| `info`  | Informational, warnings, errors (default) |
| `debug` | Debug, info, warnings, errors |

The default log filter is `info`.

### 3.2 Probe Endpoints

| Endpoint | Auth | What it confirms |
|---|---|---|
| `GET /v1/healthz` | None | Server process is alive, HTTP endpoint reachable |
| `GET /v1/readyz`  | None | Same as healthz; shallow only |
| `GET /v1/readyz/deep` | None | Store health probe via `StoreFacade::health_check()` |
| `GET /v1/metrics` | None | Bounded Prometheus-style text metrics for health/metrics routes and store health |

**healthz and readyz are shallow.** They confirm only that the HTTP server
goroutine is running. They do **not** validate the SQLite store connection or
that the governance loop is functional. Use `/v1/readyz/deep` for the built-in
store probe, and use the functional probe in Section 5 for end-to-end readiness.

A 200 from healthz or readyz does not mean the node is ready to serve workload
traffic. See Section 5 for the functional probe definition.

### 3.3 Built-In Metrics Endpoint

FerrumGate exposes a bounded `GET /v1/metrics` endpoint using Prometheus text
format without a Prometheus/OpenTelemetry runtime dependency. The endpoint is
unauthenticated like health/readiness endpoints and currently reports:

- `ferrumgate_http_requests_total{route="/v1/healthz"}`
- `ferrumgate_http_requests_total{route="/v1/readyz"}`
- `ferrumgate_http_requests_total{route="/v1/readyz/deep"}`
- `ferrumgate_http_requests_total{route="/v1/metrics"}`
- `ferrumgate_store_health_up` (`1` when `store.health_check()` passes, `0` otherwise)
- `ferrumgate_metrics_scrapes_total`
- `ferrumgate_governance_errors_total{route="/v1/..."}` - bounded per-route governance error counters for all governance endpoints

This is intentionally a minimal built-in surface. It provides bounded latency histograms
(`ferrumgate_request_duration_seconds`) for public endpoints only (`/v1/healthz`, `/v1/readyz`,
`/v1/readyz/deep`, `/v1/metrics`) with labels `route`, `method`, `status`, `le`. WAL size/page-count
gauges and connection-pool saturation signals are not yet available.

### 3.4 Derived Signals

The following signals can be derived by combining probe responses with
periodic REST calls:

- **Store reachability**: periodic `GET /v1/approvals?limit=1` with bearer auth
  returning 200 with valid JSON confirms the SQLite store is accessible.
- **Governance loop**: successful execution record fetch via
  `GET /v1/executions/{id}` confirms the PDP, capability, and store layers
  are chained correctly.
- **Provenance emission**: lineage query via `GET /v1/provenance/lineage/{id}`
  returning events confirms the provenance store is appending.

### 3.5 Blind Spots

The following cannot be observed in v1 without external tooling:

| Blind spot | Description |
|---|---|
| Bounded latency histograms for public endpoints | `/v1/metrics` exposes `ferrumgate_request_duration_seconds` histogram for `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics`; governance route latency remains future/deferred |
| No broad error rate counters (bounded governance error counters now available) | `/v1/metrics` now exposes bounded `ferrumgate_governance_errors_total` for governance endpoints; WAL/page gauges still require external tooling |
| No SQLite WAL size or page count | `/v1/metrics` exposes store up/down only; WAL/page details still require `sqlite3` CLI directly |
| No connection pool saturation signal | sqlx pool exhaustion is not exposed as a metric |
| No rollback class enforcement signal | R3 `auto_commit=false` bypass at prepare is not observable |
| No single-use capability reuse detection | Reuse is not enforced server-side at authorize |
| No provenance completeness assertion | Silent gaps in lineage are not flagged automatically |

---

## 4. Logs to Watch

The following log patterns require operator attention. For detailed
causes and resolutions, see the incident guide in
[18-single-node-operations-runbook.md §8](./18-single-node-operations-runbook.md#8-common-incidents).

| Log pattern | Alert on | Severity | Notes |
|---|---|---|---|
| `"failed to connect to sqlite"` | Any occurrence | P1 | Store DSN invalid or permissions issue; server will not start |
| `"failed to apply migrations"` | Any occurrence | P1 | Schema conflict or corrupt store; restore from backup |
| `"binding to non-loopback address requires"` | Any occurrence | P2 | Misconfiguration; set auth_mode=bearer or bind to loopback |
| `"bearer token cannot be empty"` | Any occurrence | P2 | Misconfiguration; provide non-empty bearer_token |
| `"failed to append provenance event"` | > 5 / hour | P2 | Provenance is best-effort in v1; manual lineage check required |

Startup success confirmation: `"ferrumd listening on {addr}"` confirms HTTP
server has bound and migrations have been applied.

## 5. Signals to Monitor

### 5.1 Functional Probe

The only way to confirm end-to-end readiness in v1 is a functional probe.
**See**: [20-v1-single-node-operator-checks.md §3](./20-v1-single-node-operator-checks.md#3-startup-health-verification-ladder)
for the full verification ladder (healthz → readyz → functional probe).

### 5.2 Execution Chain Signal

To confirm a specific execution has progressed through the gateway:

```bash
curl http://127.0.0.1:8080/v1/executions/{execution_id} \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

Monitor for `ExecutionState` transitions: `Authorized` → `Prepared` →
`Compensated`. Under the conservative v1 support contract the execution
chain may not reliably reach the `Committed` state; the documented
terminal state is `Compensated`.

### 5.3 Provenance Completeness Signal

To check lineage for a specific execution:

```bash
curl http://127.0.0.1:8080/v1/provenance/lineage/{execution_id} \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

A non-empty events array confirms events were emitted. An empty array may
indicate:
- The execution_id is not yet in the store
- Provenance emission was silently dropped (see Section 4)

There is no automatic completeness assertion. Manual verification against
the expected gateway step sequence is required.

---

## 6. Minimum Thresholds and Escalation Guidance

### 6.1 Startup Health

| Condition | Threshold | Severity | Action |
|---|---|---|---|
| Server does not emit "ferrumd listening" within 30s of start | Immediate | P1 | Check logs for sqlite connect/migration errors |
| healthz returns non-200 | Immediate | P1 | Server process may be deadlocked; restart |
| readyz returns non-200 | Immediate | P1 | Same as above |
| Functional probe (approvals) returns non-200 | Immediate | P1 | Store may be inaccessible; check logs |
| Functional probe returns 200 but governance loop unresponsive | 5 consecutive failures | P1 | Restart; consider restore from backup |

### 6.2 Consecutive Failure Streaks

Use streak-based thresholds for alerting rather than single-sample errors,
to avoid flapping on transient blips.

| Signal | Streak threshold | Severity | Notes |
|---|---|---|---|
| healthz returning non-200 | 3 consecutive | P1 | Indicates server process failure |
| readyz returning non-200 | 3 consecutive | P1 | Same |
| Functional probe returning 5xx | 3 consecutive | P1 | Store or internal error |
| Functional probe returning 401 | 1 | P2 | Auth misconfiguration |

### 6.3 Log-Based Alerting

Configure log scraping for the following patterns if log aggregation is
available:

| Log pattern | Alert on | Suggested severity |
|---|---|---|
| `"failed to connect to sqlite"` | Any occurrence | P1 |
| `"failed to apply migrations"` | Any occurrence | P1 |
| `"binding to non-loopback address requires"` | Any occurrence | P2 |
| `"bearer token cannot be empty"` | Any occurrence | P2 |
| `"failed to append provenance event"` | > 5 occurrences per hour | P2 |

### 6.4 Severity Reference

| Severity | Definition |
|---|---|
| P1 | Complete service unavailability or data loss risk. Immediate response required. |
| P2 | Degraded operation, misconfiguration, or elevated error rate. Respond within hours. |
| P3 | Informational. Known limitation. Monitor but no immediate action. |

---

## 7. Known Blind Spots

These are intrinsic to the v1 single-node design and are documented
here for completeness. They cannot be resolved without architectural
changes beyond v1 scope.

### 7.1 Metrics Endpoint is Bounded

`GET /v1/metrics` exists, but it is intentionally minimal. It reports counters
for health/metrics routes, store up/down gauge, and bounded governance error counters.
Operators still need external tooling for latency histograms, WAL/page size, and pool saturation.

### 7.2 healthz and readyz are Shallow

Both endpoints confirm only that the HTTP server goroutine is alive.
They do not validate the store, migrations, or governance loop. Always
use the functional probe defined in Section 5.1 after startup.

### 7.3 Provenance Append is Best-Effort

The warning `"failed to append provenance event"` indicates provenance
writes can silently fail while the primary operation succeeds. There is
no replay or retry for failed provenance appends in v1.

### 7.4 No Automatic Completeness Assertion

The lineage endpoint will return whatever events were successfully written.
Gaps in the event chain are not detected or flagged automatically.
Manual lineage verification is required if completeness is needed for
audit.

### 7.5 Rollback Class Signal is Limited

R3 `auto_commit=false` handling is verified by the current invariant evidence,
but `/v1/metrics` does not expose rollback-class distribution or per-class
prepare/execute counters. Operators should use lineage/provenance inspection for
case-specific review.

### 7.6 Capability Reuse Signal is Limited

Durable single-use capability handling is verified by the current invariant
evidence, but `/v1/metrics` does not expose rejected-reuse counters. Operators
should use logs and provenance/capability inspection for incident review.

### 7.7 SQLite Store Health is Up/Down Only

`/v1/readyz/deep` and `/v1/metrics` expose store health as a cheap up/down probe.
Operators must still use `sqlite3 /path/to/db "PRAGMA integrity_check;"`
directly to verify full store integrity.

---

## 8. References

- Support contract (scope, limits, accepted risks):
  [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
- Operations runbook (deployment, backup, restore):
  [18-single-node-operations-runbook.md](./18-single-node-operations-runbook.md)
- Operator checks (CLI-first verification):
  [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)
- Configuration reference:
  [15-deployment-and-operations.md](./15-deployment-and-operations.md)
- Troubleshooting guide:
  [17-troubleshooting.md](./17-troubleshooting.md)
- API endpoint reference:
  [14-api-and-contracts-map.md](./14-api-and-contracts-map.md)
