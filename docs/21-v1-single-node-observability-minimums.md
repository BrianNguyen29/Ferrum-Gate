# 21 — v1 Single-Node Observability Minimums

FerrumGate v1 single-node observability guide. Practical operator reference for
logs, probes, derived signals, and minimum alert thresholds.

**Scope**: single-node, SQLite-backed, v1 only.
**Audience**: operators, on-call engineers, SREs.
**Last updated**: 2026-04-02.

---

## 1. Title and Scope

**Title**: FerrumGate v1 Single-Node Observability Minimums

**Scope**: This document describes the minimum observability surface available to
operators of a FerrumGate v1 single-node deployment. It covers logs that
exist today, probe-based signals, derived signals, known blind spots, and
conservative escalation thresholds.

This document is **operator-facing and practical**. It does not describe
metrics or tracing infrastructure that does not exist in v1.

---

## 2. Boundary Note

For support scope, limits, and known caveats, see the canonical support
contract:

[19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

For CLI-first operator checks and verification ladders, see:

[20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)

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

**Both healthz and readyz are shallow.** They confirm only that the HTTP
server goroutine is running. They do **not** validate:
- The SQLite store connection
- That migrations completed successfully
- That the governance loop is functional
- That any internal state is accessible

A 200 from healthz or readyz does not mean the node is ready to serve
workload traffic. See Section 5 for the functional probe definition.

### 3.3 Metrics Endpoint

FerrumGate v1 exposes a Prometheus-format `/metrics` endpoint at `GET /metrics`.
It requires bearer-token authentication (same auth as other protected routes).

The endpoint exposes:
- `ferrum_gateway_http_requests_total` — request count by method, path (normalized), status
- `ferrum_gateway_http_request_duration_seconds` — request latency histograms
- `ferrum_gateway_http_requests_total` with `kind="error"` — error count by method, path

Path normalization: raw UUIDs in paths are replaced with `/{id}` placeholders to
avoid high-cardinality label sets.

Alerting can use log analysis, probe responses, periodic CLI/REST calls, and/or
this metrics endpoint.

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
| No SQLite WAL size or page count | Store health must be checked via `sqlite3` CLI directly |
| No connection pool saturation signal | sqlx pool exhaustion is not exposed as a metric |

---

## 4. Logs to Watch

The following log patterns require operator attention when observed.
These are ordered by severity.

### 4.1 Startup Fatal Log Watchpoints

These messages appear at startup and indicate the server will not start.
The process exits with a non-zero code.

```
"failed to connect to sqlite"
```

**Cause**: Store DSN is invalid, parent directory does not exist, or process
lacks write permission on the store path.

**Escalation**: P1 — server will not start. Check DSN, directory permissions,
and disk space.

---

```
"failed to apply migrations"
```

**Cause**: Migration SQL could not be executed against the SQLite store.
May indicate a schema conflict or a corrupt store file.

**Escalation**: P1 — server will not start. Restore from a known-good
SQLite backup if the store is corrupt.

---

```
"binding to non-loopback address requires --allow-insecure-nonlocal-bind when auth is disabled"
```

**Cause**: `auth_mode = "disabled"` with a non-loopback bind address
(0.0.0.0 or LAN IP) is a startup rejection.

**Escalation**: P2 — misconfiguration. Set `auth_mode = "bearer"` with a
non-empty token, or bind to loopback.

---

```
"bearer token cannot be empty when auth mode is bearer"
```

**Cause**: `auth_mode = "bearer"` with no `bearer_token` configured.

**Escalation**: P2 — misconfiguration. Provide a non-empty bearer token.

---

### 4.2 Runtime Warning Log Watchpoints

These messages do not cause process exit but indicate degraded operation.

```
"failed to append provenance event for capability revocation"
```

**Provenance append warning — best-effort only.**

This message appears when a provenance event fails to write to the store.
It is logged at `warn` level. The operation that triggered the event
(e.g., capability revocation) still succeeds, but the provenance record is
silently dropped.

**Operator action**: This is a known gap in v1. Provenance is best-effort
for capability revocation and other events that append after the main
transaction. If provenance completeness is required for audit, check
lineage for the affected execution_id and manually verify the chain is
complete. See `19-v1-single-node-support-contract.md` Section 3.4.

**Escalation**: P2 for audit compliance environments; P3 otherwise.

---

### 4.3 Other Runtime Events to Note

```
"ferrumd listening on {addr}"
```

**Startup success confirmation.** This confirms the HTTP server has
successfully bound. Migrations have already been applied before this line
appears (checked in `main.rs` before `run_http_server`).

---

```
"starting ferrumd with config: auth_mode={}, bind_addr={}, store_dsn={}"
```

**Startup context log.** Emitted before migrations. Useful for confirming
which config was loaded.

---

## 5. Signals to Monitor

### 5.1 Functional Probe Definition

The only way to confirm end-to-end readiness in v1 is a functional probe.
Neither healthz nor readyz qualifies.

**Functional probe (authenticated)**:

```bash
curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
# Omit -H when auth_mode=disabled
```

**Expected**: 200 OK with valid JSON envelope:
`{"items":[...],"next_cursor":null}` or `{"items":[],"next_cursor":null}`.

A 200 with valid JSON confirms:
- HTTP server is reachable
- SQLite store is accessible
- Auth is correctly configured (if enabled)
- Governance loop can query the store

**Failure modes**:
- 401 Unauthorized → auth misconfigured
- 500 Internal Server Error → store or migration issue
- Connection refused → server not running

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
- Provenance emission was silently dropped (see Section 4.2)

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

### 7.1 Metrics Endpoint is Auth-Protected

The `/metrics` endpoint requires bearer-token authentication. Configure
alerting tools to present valid credentials. Without auth, `/metrics` returns
401 Unauthorized.

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

### 7.5 SQLite Store Health

There is no built-in store health endpoint. Operators must use
`sqlite3 /path/to/db "PRAGMA integrity_check;"` directly to verify
store integrity.

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
