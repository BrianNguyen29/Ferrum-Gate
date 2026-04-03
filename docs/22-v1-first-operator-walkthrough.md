# 22 — v1 First-Operator Walkthrough

**Purpose:** Functional readiness proof for a new FerrumGate v1 single-node operator.
Guides the operator through the minimum sequence required to confirm end-to-end
functionality: install/start → functional readiness probe → first governance
action → first rollback/compensate drill.

**Scope:** single-node, SQLite-backed, v1 only.
**Audience:** First-time operators, on-call engineers performing a functional
readiness check, SREs validating a new deployment.
**Last updated:** 2026-04-03.

---

## 0. Boundary Note

This walkthrough is the functional readiness proof for FerrumGate v1 single-node.
It is not a general-purpose operations guide. For deployment, backup, restore,
and incident procedures, see:

- [18-single-node-operations-runbook.md](./18-single-node-operations-runbook.md)
- [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)
- [21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md)

For the support contract and known limitations, see:

- [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

---

## 1. Install and Start

### 1.1 Preconditions

Before starting `ferrumd`, confirm the following:

| Check | How to verify |
|---|---|
| Store DSN is set | `sqlite://` for a persistent file, `sqlite::memory:` for transient. Parent directory must exist and be writable. |
| Auth mode and bind address are consistent | If `auth_mode = "disabled"`, bind must be loopback (127.0.0.1 or ::1). For non-loopback bind, use `auth_mode = "bearer"` with a non-empty `bearer_token`. |
| Disk space is adequate | SQLite persists to a file; ensure adequate space for the database. |

### 1.2 Start the server

```bash
# Minimal start with persistent SQLite store
ferrumd \
  --bind 127.0.0.1:8080 \
  --store-dsn "sqlite:///var/lib/ferrumgate/ferrumgate.db" \
  --auth-mode bearer \
  --bearer-token "$FERRUM_BEARER_TOKEN" \
  --log-filter info
```

Or from a config file:

```bash
ferrumd --config /path/to/ferrumgate.toml
```

Expected startup sequence:
1. Server binds to the configured address.
2. `ferrum-store` applies embedded migrations to the SQLite store.
3. Gateway registers routes.
4. Server begins serving HTTP.
5. Log shows: `"ferrumd listening on {addr}"`

If the server fails to bind or migrations fail, the process exits with a
non-zero code and an error message to stderr. See the
[runbook Section 3](./18-single-node-operations-runbook.md#3-deploy-or-restart-procedure)
for troubleshooting.

---

## 2. Functional Readiness Probe

> **Canonical definition:** [21-v1-single-node-observability-minimums.md Section 5.1](./21-v1-single-node-observability-minimums.md#51-functional-probe-definition)

`healthz` and `readyz` are shallow. They confirm the HTTP server goroutine is
alive but do **not** validate the store, migrations, or governance loop.
A functional probe is required after every startup.

### 2.1 Authenticated probe (auth_mode=bearer)

```bash
export FERRUMCTL_SERVER_URL=http://127.0.0.1:8080
export FERRUMCTL_BEARER_TOKEN="$FERRUM_BEARER_TOKEN"

# Functional probe — minimum readiness check
curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
# Expected: 200 OK with valid JSON: {"items":[...],"next_cursor":null}
# An empty items list {"items":[],"next_cursor":null} is normal on first run.
```

Or via CLI:

```bash
ferrumctl server inspect-approvals
```

### 2.2 Unauthenticated probe (auth_mode=disabled)

```bash
curl http://127.0.0.1:8080/v1/approvals?limit=1
# Expected: 200 OK with valid JSON.
```

### 2.3 What a passing probe confirms

A 200 with valid JSON from `GET /v1/approvals?limit=1` confirms:

- HTTP server is reachable
- SQLite store is accessible
- Auth is correctly configured (if enabled)
- Governance loop can query the store

If this probe fails, do not proceed. See the
[runbook Section 8 for common incidents](./18-single-node-operations-runbook.md#8-common-incidents).

---

## 3. First Control Action

After the functional probe passes, confirm the operator control surface is
functional by exercising one read path and one mutating path.

### 3.1 Read path — inspect approvals

```bash
ferrumctl server inspect-approvals --limit 5
# Or:
curl "http://127.0.0.1:8080/v1/approvals?limit=5" \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

Expected: 200 OK with a list of approvals (possibly empty).

### 3.2 Read path — inspect execution record (optional, if IDs are known)

```bash
ferrumctl server inspect-execution 00000000-0000-0000-0000-000000000001
# Returns 404 for unknown IDs (expected); transport error indicates connectivity issue.
```

### 3.3 Control path — cancel a pre-execute execution (if one exists)

> **Note:** You need a known execution_id in a pre-execute state
> (Proposed, Authorized, or Prepared) to exercise this. If no executions
> exist yet, you can create one via the REST API (out of scope for this
> walkthrough — see [14-api-and-contracts-map.md](./14-api-and-contracts-map.md))
> or skip to Section 4.

```bash
# Cancel an execution in a pre-execute state
ferrumctl server cancel-execution <execution_id>
# Or:
curl -X POST "http://127.0.0.1:8080/v1/executions/<execution_id>/cancel" \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

**Expected outcome for cancel:** HTTP 200; execution transitions to a terminal
state (Cancelled). If the execution is in a non-cancellable state (Running,
AwaitingVerification, Completed, Failed, RolledBack), the API returns an error.

### 3.4 Control path — pause and resume (if a Running execution exists)

```bash
# Pause
ferrumctl server pause-execution <execution_id>
# Resume
ferrumctl server resume-execution <execution_id>
```

These require the execution to be in the correct state (Running or
AwaitingVerification for pause; Paused for resume). See
[20-v1-single-node-operator-checks.md Section 7](./20-v1-single-node-operator-checks.md#7-operator-control-commands)
for the full control command reference.

---

## 4. First Upgrade / Change Path Check

> **Important framing:** In-place upgrade of `ferrumd` is **not** a v1 feature.
> No built-in upgrade mechanism exists. Upgrade tracks U2/U3/U4 are post-v1
> backlog. The check below is a **controlled change-path verification**, not
> a product feature.
>
> See: [19-v1-single-node-support-contract.md Section 2.4](./19-v1-single-node-support-contract.md#24-upgrade-tracks)

The upgrade-path check confirms the operator understands the safe change
sequence if a new `ferrumd` binary must be deployed.

### 4.1 Safe change sequence for binary replacement

1. **Backup before any change.** Back up the SQLite store:
   ```bash
   STORE_PATH="/var/lib/ferrumgate/ferrumgate.db"
   BACKUP_DIR="/var/backups/ferrumgate"
   TIMESTAMP=$(date +%Y%m%d_%H%M%S)
   cp "$STORE_PATH" "${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.db"
   sqlite3 "${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.db" "PRAGMA integrity_check;"
   # Expected: "ok"
   ```
   See: [runbook Section 5](./18-single-node-operations-runbook.md#5-backup-procedure-manual-sqlite-file-backup)

2. **Stop the running `ferrumd` process.** Identify and stop the process:
   ```bash
   FERRUM_PID=$(pgrep -f ferrumd)
   kill "$FERRUM_PID"
   sleep 2
   # Verify stopped
   curl http://127.0.0.1:8080/v1/healthz
   # Expected: connection refused
   ```

3. **Replace the binary.** Swap `ferrumd` with the new version.

4. **Start the new binary** using the same config as step 1.

5. **Re-run the functional readiness probe** (Section 2 above).

6. **Verify existing execution records are present.** If you had known
   execution IDs before the change, confirm they are still queryable:
   ```bash
   ferrumctl server inspect-execution <known_execution_id>
   ```

### 4.2 What this check verifies

The controlled change-path check is **not** a guarantee that v1 supports
hot-upgrade or zero-downtime replacement. It confirms:

- The operator can perform a binary swap with a known-good SQLite backup.
- The governance loop re-connects to the existing store after restart.
- Existing execution and approval records are still queryable.
- The functional probe still passes after a binary swap.

If the new binary fails to start (migration error, store incompatibility),
restore from the pre-change backup per [runbook Section 6](./18-single-node-operations-runbook.md#6-restore-procedure-manual-sqlite-file-restore).

### 4.3 What v1 does not support for upgrade

| Unsupported upgrade pattern | Workaround |
|---|---|
| In-place hot-upgrade | Not available in v1. Binary swap with downtime required. |
| Automatic rollback to previous binary | Operator must manually revert the binary and restore from backup. |
| Upgrade tracks U2/U3/U4 | Post-v1 backlog. |

---

## 5. First Rollback / Compensate Drill

> **Canonical reference:** [runbook Section 7](./18-single-node-operations-runbook.md#7-recovery-procedure-compensate--manual-restore-fallback)

The compensate endpoint (`POST /v1/executions/{execution_id}/compensate`) is the
primary recovery path in v1. However, compensate may be a **no-op** depending
on the adapter and rollback class (R0/R1/R2/R3).

The drill below confirms the operator can:
1. Attempt compensate on a known execution.
2. Verify the execution state transition.
3. Fall back to manual SQLite restore if compensate is insufficient.

### 5.1 Drill preconditions

- A known execution_id in a non-terminal state (Authorized or Prepared).
- If no such execution exists, skip to Section 5.4 to verify compensate on a
  Prepared execution created through a test proposal (out of scope for this
  walkthrough; use the REST API per [14-api-and-contracts-map.md](./14-api-and-contracts-map.md)).

### 5.2 Attempt compensate

```bash
curl -X POST "http://127.0.0.1:8080/v1/executions/<execution_id>/compensate" \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

Or via CLI:

```bash
ferrumctl server compensate-execution <execution_id>
```

**Expected:** HTTP 200 with the execution record showing a state transition
(e.g., `Prepared → Compensated` or similar). The execution may remain in its
current state if compensate is a no-op for the adapter/rollback class.

### 5.3 Verify execution state post-compensate

```bash
ferrumctl server inspect-execution <execution_id>
```

**Important:** A 200 from compensate does **not** guarantee an external undo
action occurred. Always verify the affected resource state manually. See the
runbook Section 7 for the decision guide.

### 5.4 Manual restore drill (if compensate is inconclusive or insufficient)

This drill confirms the operator can restore from a known-good backup when
compensate is insufficient. See [runbook Section 6.4](./18-single-node-operations-runbook.md#64-restore-drill-procedure-and-evidence)
for the full drill procedure and evidence template.

### 5.5 What this drill verifies

| Step | Pass criteria |
|---|---|
| Compensate call returns 200 | API surface is functional |
| Execution record transitions to Compensated (or stays in known state) | Compensate response is well-formed |
| Execution inspect returns valid JSON | Store query is functional |
| Manual restore (if performed) restores known records | Backup + restore path is verified |

---

## 6. Attestation / Evidence Capture Block

> **Use this block to record functional readiness proof evidence.** Complete
> all fields. This block serves as the P3.G1 evidence record when the
> walkthrough is performed and signed off by the operator.

```
Functional Readiness Proof — FerrumGate v1 Single-Node
=======================================================
Date:          <YYYY-MM-DD>
Operator:      <name or ticket>
Node ID:       <host or instance identifier>
ferrumd version: <version or git commit if available>

Section 1 — Install and Start
-----------------------------
Startup log shows "ferrumd listening on {addr}":  <PASS | FAIL | SKIP>
Startup error (if any):                     <none | error message>

Section 2 — Functional Readiness Probe
-------------------------------------
Probe endpoint: GET /v1/approvals?limit=1
Auth mode:      <bearer | disabled>
HTTP status:    <200 | other>
JSON parseable: <yes | no>
Probe outcome:  <PASS | FAIL>

Section 3 — First Control Action
---------------------------------
Read path (inspect-approvals):              <PASS | FAIL | SKIP>
Control path (cancel/pause/resume):         <PASS | FAIL | SKIP>
Control outcome:                            <describe outcome>

Section 4 — Upgrade / Change Path Check
---------------------------------------
Pre-change backup taken:                    <yes | no>
Backup integrity (PRAGMA integrity_check):  <ok | FAIL | SKIP>
Binary replaced and restarted:              <PASS | FAIL | SKIP>
Functional probe after restart:              <PASS | FAIL | SKIP>
Existing execution records queryable:        <PASS | FAIL | SKIP>
Change-path outcome:                        <PASS | FAIL | SKIP>

Section 5 — Rollback / Compensate Drill
---------------------------------------
Compensate call made (execution_id):        <execution_id or SKIP>
Compensate HTTP status:                     <200 | other>
Execution state post-compensate:            <state>
Manual restore drill performed:             <yes | no | SKIP>
Restore drill outcome:                      <PASS | FAIL | SKIP>
Rollback/compensate outcome:                <PASS | FAIL | SKIP>

Overall Functional Readiness:               <PASS | FAIL>
Operator sign-off:                         <name / ticket / date>
Notes:                                     <any observations or corrective actions>
```

### P3.G1 Completion Criteria

P3.G1 (Functional readiness proof — end-to-end operator walkthrough) is
**complete** when:

- All applicable sections above are marked PASS.
- The attestation block is signed off by the operator.
- The document is retained as an operational record.

P3.G1 is **not** complete if any required section is marked FAIL. Fix the
failure and re-run the relevant section before signing off.

---

## 7. Cross-Reference Summary

| Topic | Doc |
|---|---|
| Full operations runbook (deploy, backup, restore, recovery) | [18-single-node-operations-runbook.md](./18-single-node-operations-runbook.md) |
| CLI-first operator checks and control surface | [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md) |
| Observability minimums (probes, logs, thresholds) | [21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md) |
| Support contract and known limitations | [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md) |
| API route reference | [14-api-and-contracts-map.md](./14-api-and-contracts-map.md) |
| v1 RC evidence | [implementation-path/25-v1-single-node-rc-evidence.md](./implementation-path/25-v1-single-node-rc-evidence.md) |
| Production roadmap | [implementation-path/30-production-roadmap.md](./implementation-path/30-production-roadmap.md) |
