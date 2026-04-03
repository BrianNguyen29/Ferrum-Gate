# 33 — P3.G2 Smoke Stability Evidence

**Purpose:** Operator evidence record template for P3.G2 — confirming
FerrumGate v1 single-node remains stable under sustained operational
lifecycle through repeated probe checks and control-path verification.

**Scope:** Single-node, SQLite-backed, v1 only.

**Audience:** Operators performing scheduled smoke stability validation,
SREs running sustained-lifecycle checks, compliance attestors.

**Last updated:** 2026-04-03.

---

## 0. Relationship to Other Documents

This document is the **P3.G2 evidence pack** for the production roadmap.
It complements the observability minimums and operator walkthrough:

| Topic | Doc |
|---|---|
| Observability surface reference (probes, thresholds) | [21-v1-single-node-observability-minimums.md](../21-v1-single-node-observability-minimums.md) |
| Functional probe definition | [21-v1-single-node-observability-minimums.md Section 5.1](../21-v1-single-node-observability-minimums.md#51-functional-probe-definition) |
| Consecutive failure streaks and escalation thresholds | [21-v1-single-node-observability-minimums.md Section 6](../21-v1-single-node-observability-minimums.md#6-minimum-thresholds-and-escalation-guidance) |
| Operations runbook | [18-single-node-operations-runbook.md](../18-single-node-operations-runbook.md) |
| First-operator walkthrough (includes functional probe) | [22-v1-first-operator-walkthrough.md](../22-v1-first-operator-walkthrough.md) |

**Do not use this document as a procedures guide.** It is an evidence
template. Use the documents above for step-by-step procedures and
threshold definitions.

---

## 1. What P3.G2 Requires

P3.G2 (Smoke stability evidence) requires confirming that a FerrumGate
v1 single-node remains stable under sustained operational load through
repeated, interval-based probe and control-path checks.

**What "smoke stability" means in v1 context:**

- The node survives a sustained-lifecycle run (minimum 48 hours
  recommended, or an equivalent automated cycle of repeated probes)
- No crashes, unhandled panics, or unrecoverable states occur
- The functional probe continues to return 200 with valid JSON throughout
- Control-path endpoints remain responsive
- Store integrity is confirmed at the start and end of the run
- No startup fatal log messages appear

**What this document does NOT require:**

- A production workload running against the node
- Maximum throughput or stress testing
- A formal soak test harness (automated or manual cycles are both acceptable)

**Drill cadence:** Perform after initial deployment, after any
configuration change that affects stability, and on a scheduled basis
(minimum quarterly recommended).

---

## 2. Pre-Run Store Integrity Check

Before beginning a smoke stability run, confirm the store is healthy.
Record the result in the attestation block (Section 6).

```
Pre-Run Store Integrity Check — FerrumGate v1 Single-Node
==========================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>

--- Store info ---
Store file path:       <absolute path to the .db file>
Store size (bytes):    <number>

--- Integrity check ---
Integrity check cmd:   sqlite3 <store_path> "PRAGMA integrity_check;"
Integrity result:      <ok | FAIL>

Pre-run outcome:       <PASS | FAIL>
```

### Pre-Run Pass Criteria

| Check | Required |
|---|---|
| `PRAGMA integrity_check` returns `ok` | Yes |
| Store file is non-zero size | Yes |
| No startup fatal log messages in `ferrumd` output at time of check | Yes |

---

## 3. Smoke Probe Checks (Repeated Interval Probe)

Complete one block per probe interval during the smoke run.
Record the timestamp, endpoint, response code, and outcome.

**Minimum recommended interval:** Every 15 minutes (96 samples per 24h).
**Minimum run duration:** 48 hours or 96 consecutive passing samples,
whichever comes first. Shorter automated cycles are acceptable if they
demonstrate repeated stability over multiple iterations.

```
Smoke Probe Record — FerrumGate v1 Single-Node
==============================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>
Run ID:                <operator-defined run identifier>

--- Probe checks ---

GET /v1/healthz
  HTTP status:         <200 | other>
  Response time (ms): <number>
  healthz outcome:     <PASS | FAIL>

GET /v1/readyz
  HTTP status:         <200 | other>
  Response time (ms): <number>
  readyz outcome:      <PASS | FAIL>

GET /v1/approvals?limit=1  (with bearer auth if auth_mode=bearer)
  HTTP status:         <200 | other>
  Response time (ms): <number>
  JSON parseable:      <yes | no>
  Has envelope key:    <yes | no>
  functional probe outcome: <PASS | FAIL>

--- Consecutive failure tracking ---
Failures this run (total): <number>
Consecutive failures:      <number>

Overall probe outcome:     <PASS | FAIL>
Notes:                    <any observations>
```

### Smoke Probe Pass Criteria

| Check | Threshold | Required |
|---|---|---|
| `GET /v1/healthz` returns 200 | Always | Yes |
| `GET /v1/readyz` returns 200 | Always | Yes |
| `GET /v1/approvals?limit=1` returns 200 with valid JSON | Always | Yes |
| Consecutive probe failures | ≤ 2 consecutive → warn; ≥ 3 consecutive → FAIL | Yes |
| Any single probe returning 500 | Any occurrence → note; ≥ 3 occurrences → FAIL | Yes |

See [observability minimums Section 6.2](../21-v1-single-node-observability-minimums.md#62-consecutive-failure-streaks)
for the canonical streak thresholds.

---

## 4. Control-Path Verification Checks

Periodically verify that mutating control-path endpoints remain
responsive throughout the run. Perform at least once at the start
and once at the end of the run.

```
Control-Path Check Record — FerrumGate v1 Single-Node
====================================================
Date:                  <YYYY-MM-DD>
Time (UTC):            <HH:MM:SS>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>
Run ID:                <operator-defined run identifier>
Check type:            <start-of-run | end-of-run | interval | ad-hoc>

--- Read path checks ---

GET /v1/executions/{execution_id}  (with bearer auth)
  HTTP status:         <200 | other>
  JSON parseable:      <yes | no>
  read-path outcome:  <PASS | FAIL>

GET /v1/approvals?limit=1  (with bearer auth if auth_mode=bearer)
  HTTP status:         <200 | other>
  JSON parseable:      <yes | no>
  approvals-path outcome: <PASS | FAIL>

--- Control probe (cancel/pause/resume) ---
Execution ID used:     <execution_id or "none available">
Control action:        <cancel | pause | resume | SKIP>
HTTP status:           <200 | 404 | other>
Control outcome:       <PASS | FAIL | SKIP>

Overall control-path outcome: <PASS | FAIL | SKIP>
Notes:                         <any observations>
```

### Control-Path Pass Criteria

| Check | Required |
|---|---|
| Read-path (`GET /v1/executions/{execution_id}`) returns 200 with valid JSON | Yes |
| Approvals path (`GET /v1/approvals?limit=1`) returns 200 with valid JSON | Yes |
| Control action (cancel/pause/resume) returns expected HTTP status | Yes (or SKIP if no execution available) |

---

## 5. Interval Recording Log

Maintain a running log of all probe checks during the smoke run.
Use this section to record batch results if running an automated cycle.

```
Smoke Stability Run — Interval Summary
======================================
Run ID:                <operator-defined run identifier>
Run start:             <YYYY-MM-DD HH:MM:SS>
Run end:               <YYYY-MM-DD HH:MM:SS>
Total duration:        <hours | days>
Operator:              <name or ticket>
Node ID:               <host or instance identifier>

--- Aggregate results ---

Total probe intervals: <number>
Passing intervals:    <number>
Failing intervals:     <number>
Skipped intervals:     <number>
Overall pass rate:     <percentage>

--- Endpoint breakdown ---

healthz:
  Total checks:        <number>
  Pass:                 <number>
  Fail:                 <number>

readyz:
  Total checks:        <number>
  Pass:                 <number>
  Fail:                 <number>

approvals (functional probe):
  Total checks:        <number>
  Pass:                 <number>
  Fail:                 <number>
  Consecutive failure max: <number>

--- Control-path summary ---
Control-path checks performed: <number>
Control-path passes:           <number>
Control-path failures:         <number>

--- Log watch ---
Startup fatal logs observed: <yes | no>
  If yes, describe:         <none | error message>

--- Run outcome ---
Overall run outcome:         <PASS | FAIL | INCONCLUSIVE>
Operator sign-off:           <name / ticket / date>
Notes:                       <any observations or corrective actions>
```

### Interval Recording Pass Criteria

| Check | Threshold | Required |
|---|---|---|
| Pass rate across all probe intervals | ≥ 95% (allow for transient blips) | Yes |
| Maximum consecutive failures | ≤ 2 | Yes |
| Any 500-class response from functional probe | ≥ 3 occurrences → FAIL | Yes |
| Any startup fatal log during run | Any occurrence → FAIL | Yes |
| Control-path checks | At least 1 pass at start and 1 pass at end | Yes |

---

## 6. Combined Attestation Block

```
P3.G2 — Smoke Stability Evidence — Operator Attestation
=======================================================
Date of smoke run:          <YYYY-MM-DD>
Operator:                   <name or ticket>
Node ID:                    <host or instance identifier>
Run ID:                     <operator-defined run identifier>

Pre-run store integrity:   <PASS | FAIL>
Interval recording pass rate: <percentage>
Control-path outcome:       <PASS | FAIL | SKIP>
Log watch findings:         <none | describe>

I confirm:
  [ ] Pre-run PRAGMA integrity_check returned "ok".
  [ ] The smoke run covered at least 48 hours OR at least 96 automated probe cycles.
  [ ] No startup fatal logs ("failed to connect to sqlite", "failed to apply migrations") were observed.
  [ ] The functional probe (GET /v1/approvals?limit=1) returned 200 with valid JSON on every interval.
  [ ] Consecutive failure streak did not exceed 2.
  [ ] At least one control-path check was performed and passed (or SKIP with justification).
  [ ] All pass criteria in Sections 2, 3, 4, and 5 above are satisfied.

Findings:                   <none | describe any anomalies>
Corrective actions taken:   <none | describe actions>

Overall P3.G2 verdict:      <PASS | FAIL — requires re-run>
Operator sign-off:          <name / ticket / date>
```

---

## 7. Quick-Reference: Smoke Stability Signals

| Signal | How to Check | Auth | Pass Indicator |
|---|---|---|---|
| Process liveness | `curl http://<addr>:<port>/v1/healthz` | None | 200 |
| Readiness (shallow) | `curl http://<addr>:<port>/v1/readyz` | None | 200 |
| Store + governance (functional probe) | `curl http://<addr>:<port>/v1/approvals?limit=1` | Bearer (if auth enabled) | 200 + JSON |
| Store integrity | `sqlite3 <path> "PRAGMA integrity_check;"` | None | `ok` |
| Control-path read | `curl http://<addr>:<port>/v1/executions/{id}` | Bearer | 200 + JSON |
| Control-path mutating | `curl -X POST http://<addr>:<port>/v1/executions/<id>/cancel` | Bearer | 200 or 404 |

For escalation thresholds and consecutive failure streak definitions,
see [21-v1-single-node-observability-minimums.md Section 6](../21-v1-single-node-observability-minimums.md#6-minimum-thresholds-and-escalation-guidance).

---

## 8. What Is Not Covered by This Evidence Template

| Scenario | Why Not Covered | Workaround |
|---|---|---|
| Throughput / load stress testing | Not required for P3.G2 smoke stability | Post-v1 benchmarking |
| Adapter side-effect correctness under load | Adapters are skeleton in v1; real side-effects are post-v1 | Use adapter integration tests |
| Multi-node stability | v1 is single-node only | Multi-node is P5 roadmap |
| Automatic store integrity monitoring | No continuous integrity check endpoint; only manual `PRAGMA integrity_check` | Periodic manual check or script |
| Distributed tracing | Not implemented in v1 | Post-v1 roadmap |

---

## 9. Relationship to P3.G1 and P3.G3 / P3.G4

P3.G2 is complementary to the other P3.G evidence items:

| Item | Focus | Doc |
|---|---|---|
| P3.G1 | One-time functional readiness proof (first-operator walkthrough) | [22-v1-first-operator-walkthrough.md](../22-v1-first-operator-walkthrough.md) |
| P3.G2 | Sustained-lifecycle smoke stability (repeated probes over time) | This doc |
| P3.G3 | Backup / restore drill under rollback scenario | [31-p3-g3-backup-restore-drill-evidence.md](./31-p3-g3-backup-restore-drill-evidence.md) |
| P3.G4 | Observability surface confirmed operational in target environment | [32-p3-g4-observability-verification-evidence.md](./32-p3-g4-observability-verification-evidence.md) |

All four P3.G items must be completed and attested before the
Priority 3 (Operational Hardening) track can be marked DONE in the
production roadmap.
