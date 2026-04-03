# 32 — P3.G4 Observability Verification Evidence

**Purpose:** Operator evidence record template for P3.G4 — confirming that
the metrics, logging, and probe surface is operational in a target
environment.

**Scope:** Single-node, SQLite-backed, v1 only.

**Audience:** Operators verifying observability infrastructure on a newly
deployed or existing node, SREs validating monitoring readiness, compliance
attestors.

**Last updated:** 2026-04-03.

---

## 0. Relationship to Other Documents

This document is the **P3.G4 evidence pack** for the production roadmap.
It complements the observability minimums reference:

| Topic | Doc |
|---|---|
| Observability surface reference (what exists in v1) | [21-v1-single-node-observability-minimums.md](../21-v1-single-node-observability-minimums.md) |
| Functional probe definition | [21-v1-single-node-observability-minimums.md Section 5.1](../21-v1-single-node-observability-minimums.md#51-functional-probe-definition) |
| Log watchpoints and escalation thresholds | [21-v1-single-node-observability-minimums.md Section 4](../21-v1-single-node-observability-minimums.md#4-logs-to-watch) |
| Operations runbook | [18-single-node-operations-runbook.md](../18-single-node-operations-runbook.md) |
| First-operator walkthrough (includes live probe) | [22-v1-first-operator-walkthrough.md](../22-v1-first-operator-walkthrough.md) |

**Do not use this document as a procedures guide.** It is an evidence
template. Use the documents above for step-by-step procedures and surface
definitions.

---

## 1. What Is Verifiable in v1

P3.G4 requires confirming that the existing observability surface is
operational in the target environment. The following signals exist in v1:

| Signal | Endpoint / Method | Auth Required | Notes |
|---|---|---|---|
| Health probe | `GET /v1/healthz` | No | Shallow — confirms HTTP server goroutine only |
| Readiness probe | `GET /v1/readyz` | No | Identical to healthz in v1 |
| Functional probe | `GET /v1/approvals?limit=1` | Yes (bearer) | Confirms store + governance loop; see Section 5.1 of observability minimums |
| Metrics (Prometheus) | `GET /metrics` | Yes (bearer) | HTTP request counts, latency histograms, error counts |
| Execution chain | `GET /v1/executions/{id}` | Yes (bearer) | Confirms PDP + capability + store chain |
| Provenance lineage | `GET /v1/provenance/lineage/{id}` | Yes (bearer) | Confirms provenance store is appending |
| Structured logs | stdout / `ferrumd` process output | No | Controlled via `--log-filter`; default `info` |

**Not available in v1:** Distributed tracing (OpenTelemetry, Jaeger, Zipkin),
SQLite WAL size metrics, connection pool saturation metrics, automatic
provenance completeness assertions. See [21-v1-single-node-observability-minimums.md
Section 7](../21-v1-single-node-observability-minimums.md#7-known-blind-spots) for the full blind-spots list.

> **Drill cadence:** Perform after initial deployment, after configuration
> changes that affect logging or auth, and as part of periodic observability
> health checks. v1 does not have auto-alerting; operators configure
> external monitoring against the signals above.

---

## 2. Probe Verification Evidence Template

Complete one block per verification session.

```
Observability Probe Verification Record — FerrumGate v1 Single-Node
====================================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>
Environment:           <production | staging | drill>
ferrumd running:        <yes | no>
ferrumd version/git:   <version or git hash if available>

--- Probe endpoint checks ---

GET /v1/healthz
  HTTP status:         <200 | other>
  Response time (ms): <number>
  healthz outcome:     <PASS | FAIL>

GET /v1/readyz
  HTTP status:         <200 | other>
  Response time (ms): <number>
  readyz outcome:      <PASS | FAIL>

--- Functional probe check ---

GET /v1/approvals?limit=1  (with bearer auth)
  HTTP status:         <200 | other>
  Response time (ms): <number>
  JSON parseable:      <yes | no>
  Has items key:       <yes | no>
  functional probe outcome: <PASS | FAIL>

--- Metrics endpoint check ---

GET /metrics  (with bearer auth)
  HTTP status:         <200 | other>
  Response time (ms): <number>
  Content-Type:        <text/plain; version=prometheus | other>
  Contains ferrum_gateway_http_requests_total: <yes | no>
  Contains ferrum_gateway_http_request_duration_seconds: <yes | no>
  metrics endpoint outcome: <PASS | FAIL>

--- Log emission check ---

Log filter in use:     <info | debug | warn | error>
ferrumd stdout visible: <yes | no | SKIP (logs to file)>
Startup log "ferrumd listening" seen: <yes | no>
Runtime logs flowing:   <yes | no | SKIP>
log emission outcome:   <PASS | FAIL | SKIP>

Overall probe verification outcome: <PASS | FAIL>
Notes:                         <any observations or corrective actions>
```

### Probe Verification Pass Criteria

| Check | Required |
|---|---|
| `GET /v1/healthz` returns 200 | Yes |
| `GET /v1/readyz` returns 200 | Yes |
| `GET /v1/approvals?limit=1` returns 200 with valid JSON envelope | Yes |
| `/metrics` returns 200 with Prometheus text format | Yes |
| Prometheus metrics include `ferrum_gateway_http_requests_total` | Yes |
| Prometheus metrics include `ferrum_gateway_http_request_duration_seconds` | Yes |
| Logs are flowing from `ferrumd` process (if stdout accessible) | Yes (or SKIP if logging to file) |

---

## 3. Metrics Detail Verification Template

Use this section to record detailed metrics content when deeper
verification is required.

```
Metrics Detail Record — FerrumGate v1 Single-Node
===================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>

--- Metrics sample ---

GET /metrics HTTP status:     <200 | other>
Sample duration (collection interval): <seconds>

--- HTTP request metrics ---

ferrum_gateway_http_requests_total (sample count):
  Count > 0:                 <yes | no>
  Labels present:           <method, path, status, kind | unknown>

ferrum_gateway_http_request_duration_seconds (sample):
  Count > 0:                <yes | no>
  Has histogram buckets:    <yes | no>

--- Error metrics ---

ferrum_gateway_http_requests_total{kind="error"} (if present):
  Error count > 0:           <yes | no | not present>

--- First success response ---

First recorded request timestamp: <ISO8601 or "unknown">
First recorded successful (2xx) request: <yes | no | unknown>

Overall metrics detail outcome: <PASS | FAIL>
Notes:                         <any observations or corrective actions>
```

---

## 4. Derived Signals Verification Template

Use this section to verify derived signals (execution chain and provenance).

```
Derived Signals Verification Record — FerrumGate v1 Single-Node
=================================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>

--- Precondition ---
Known execution_id available:  <yes | no | SKIP>

--- Execution chain signal ---

GET /v1/executions/{execution_id}  (with bearer auth)
  HTTP status:         <200 | other>
  JSON parseable:      <yes | no>
  Contains state field: <yes | no>
  execution chain outcome: <PASS | FAIL | SKIP>

--- Provenance lineage signal ---

GET /v1/provenance/lineage/{execution_id}  (with bearer auth)
  HTTP status:         <200 | 404 | other>
  JSON parseable:      <yes | no>
  Contains events array: <yes | no>
  lineage outcome:     <PASS | FAIL | SKIP>
  Note: empty events array is not a failure — provenance append is
        best-effort in v1. See observability minimums Section 4.2.

Overall derived signals outcome: <PASS | FAIL | SKIP>
Notes:                         <any observations or corrective actions>
```

---

## 5. Combined Attestation Block

```
P3.G4 — Observability Verification — Operator Attestation
==========================================================
Date of verification session:  <YYYY-MM-DD>
Operator:                       <name or ticket>
Node ID:                        <host or instance identifier>
Environment:                    <production | staging | drill>

Probe verification outcome:    <PASS | FAIL>
Metrics detail outcome:         <PASS | FAIL | SKIP>
Derived signals outcome:        <PASS | FAIL | SKIP>

I confirm:
  [ ] All probe endpoints (/healthz, /readyz) returned 200.
  [ ] Functional probe (GET /v1/approvals?limit=1) returned 200 with valid JSON.
  [ ] /metrics endpoint returned 200 with Prometheus-formatted metrics.
  [ ] HTTP request count and latency histogram metrics are present.
  [ ] Log emission is active (or has been confirmed via startup log).
  [ ] All pass criteria in Sections 2, 3, and 4 above are satisfied.

Verifications skipped:          <none | list any SKIP items and reason>
Findings:                        <none | describe any anomalies>
Corrective actions taken:        <none | describe actions>

Overall P3.G4 verdict:          <PASS | FAIL — requires re-verification>
Operator sign-off:               <name / ticket / date>
```

---

## 6. Quick-Reference: v1 Observability Signal Summary

| Signal | How to Collect | Auth | Pass Indicator |
|---|---|---|---|
| Process liveness | `curl http://<addr>:<port>/v1/healthz` | None | 200 |
| Readiness (shallow) | `curl http://<addr>:<port>/v1/readyz` | None | 200 |
| Store + governance | `curl http://<addr>:<port>/v1/approvals?limit=1` | Bearer | 200 + JSON |
| Prometheus metrics | `curl http://<addr>:<port>/metrics` | Bearer | 200 + Prometheus text |
| Execution state | `curl http://<addr>:<port>/v1/executions/<id>` | Bearer | 200 + JSON |
| Provenance lineage | `curl http://<addr>:<port>/v1/provenance/lineage/<id>` | Bearer | 200 + JSON |
| Structured logs | stdout / `journalctl -u ferrumd` / log file | None | `ferrumd listening` seen |

For thresholds and escalation guidance, see
[21-v1-single-node-observability-minimums.md Section 6](../21-v1-single-node-observability-minimums.md#6-minimum-thresholds-and-escalation-guidance).

---

## 7. Blind Spots That Are Not Covered by This Drill

The following cannot be verified via this evidence template because v1
does not expose them:

| Signal | Why It Is Not Verifiable | Workaround |
|---|---|---|
| SQLite WAL size / page count | No metric exposed | Use `sqlite3 <path> "PRAGMA integrity_check;"` directly |
| sqlx connection pool saturation | No metric exposed | Monitor `ferrumd` process memory; restart if connection exhaustion suspected |
| Distributed tracing (OTLP) | Not implemented in v1 | Post-v1 roadmap |
| Automatic provenance completeness check | Lineage returns best-effort events; no gap detection | Manual lineage review against expected gateway step sequence |
| Auto-alerting | No built-in alerting; configure external monitoring | Use metrics endpoint + external Prometheus/Alertmanager |

See [21-v1-single-node-observability-minimums.md Section 7](../21-v1-single-node-observability-minimums.md#7-known-blind-spots) for the full list.
