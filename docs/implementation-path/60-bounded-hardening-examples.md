# 60 — Bounded Hardening Examples

> **Status**: Created 2026-04-29 — documentation-only examples
> **Scope**: Single-node v1 SQLite unless labeled post-v1
> **Purpose**: Bounded, conservative examples of hardening drill scenarios for operator training and verification
> **Constraint**: These are illustrative bounded examples, not production configurations. RC-ready/conditional only.

---

## Purpose

This document provides bounded, conservative examples for operator hardening drills. Examples are intentionally scoped to single-node SQLite v1 constraints and do not represent production-ready configurations.

All examples follow:
- Conservative posture (fail-closed where behavior is uncertain)
- RC-ready/conditional scope (not production-ready)
- Operator-owned procedures (not automated)

---

## Example 1 — Git Remote Push Fail-Closed Drill Notes

### Context
Per `56-adapter-compensation-evidence-matrix.md` §Git adapter: GitPush compensation is `fail_closed` / remote-dependent. This example shows the expected fail-closed behavior when remote rollback cannot be proven.

### Bounded Scenario
- Local repo: `/tmp/ferrum-drill-git-push`
- Remote repo: `/tmp/ferrum-drill-git-remote`
- Branch protection: enabled (pre-receive hook blocks branch deletion)
- Expected outcome: `recovered=false` with `rollback_failed=true`

### Drill Commands

**Setup:**
```bash
# Create remote repo
mkdir -p /tmp/ferrum-drill-git-remote
git -C /tmp/ferrum-drill-git-remote init --bare

# Create local repo
mkdir -p /tmp/ferrum-drill-git-push
git -C /tmp/ferrum-drill-git-push init
git -C /tmp/ferrum-drill-git-push remote add origin /tmp/ferrum-drill-git-remote

# Configure pre-receive hook to block deletions
cat > /tmp/ferrum-drill-git-remote/hooks/pre-receive << 'EOF'
#!/bin/bash
while read old_sha new_sha ref; do
  if echo "$new_sha" | grep -qE '^0+$'; then
    echo "Deletion denied by pre-receive hook"
    exit 1
  fi
done
exit 0
EOF
chmod +x /tmp/ferrum-drill-git-remote/hooks/pre-receive

# Create test commit
echo "test content" > /tmp/ferrum-drill-git-push/file.txt
git -C /tmp/ferrum-drill-git-push add .
git -C /tmp/ferrum-drill-git-push commit -m "test commit"
```

**Execute push via FerrumGate intent** (operator fills intent_id after execution):
```bash
Intent ID: _______________________________
```

**Capture pre-push remote state:**
```bash
$ git -C /tmp/ferrum-drill-git-remote rev-parse refs/heads/main
abc1234...
```

**Trigger compensation:**
```bash
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

**Expected Response (fail-closed):**
```json
{
  "execution_id": "{intent_id}",
  "recovered": false,
  "rollback_failed": true,
  "failure_reason": "remote_ref deletion denied by pre-receive hook",
  "adapter": "git",
  "action": "GitPush"
}
```

**Verify remote state unchanged:**
```bash
$ git -C /tmp/ferrum-drill-git-remote rev-parse refs/heads/main
abc1234...
```

### Key Takeaways
- `recovered=false` with `rollback_failed=true` is **correct behavior**, not an error
- Remote state must remain unchanged after failed compensation
- Operators must evaluate their remote's branch protection policy before relying on GitPush compensation

---

## Example 2 — HTTP Replay Contract Examples

### Context
Per `56-adapter-compensation-evidence-matrix.md` §HTTP adapter: HTTP compensation is `replay_compensation`, not true undo. This example shows the bounded contract for POST/PUT/PATCH replay.

### Bounded Scenario
- External API: `https://api.example.com/v1/resource`
- Compensation plan: `http.replay_v1` with idempotency key
- Expected behavior: Replay sent with idempotency key; `recovered=true` confirms replay, not undo

### POST Mutation — Replay Contract

**Original Request (captured during intent execution):**
```json
{
  "method": "POST",
  "url": "https://api.example.com/v1/resource",
  "headers": {
    "Content-Type": "application/json",
    "Idempotency-Key": "ferrum-{execution_id}"
  },
  "body": {
    "name": "test-resource",
    "value": 42
  }
}
```

**Compensation Plan (in intent):**
```json
{
  "adapter": "http",
  "action": "HttpMutation",
  "config": {
    "http.replay_v1": {
      "method": "POST",
      "url": "https://api.example.com/v1/resource",
      "expected_statuses": [200, 201],
      "idempotency_key_template": "ferrum-{{execution_id}}",
      "body_template": {
        "name": "test-resource",
        "value": 42
      }
    }
  }
}
```

**Replayed Request (sent during compensation):**
```
POST /v1/resource HTTP/1.1
Host: api.example.com
Content-Type: application/json
Idempotency-Key: ferrum-{execution_id}

{"name":"test-resource","value":42}
```

**Expected Response:**
```json
{
  "execution_id": "{execution_id}",
  "recovered": true,
  "compensation_type": "replay",
  "idempotency_key": "ferrum-{execution_id}",
  "replay_metadata": {
    "url": "https://api.example.com/v1/resource",
    "method": "POST",
    "response_status": 200
  }
}
```

### PUT Mutation — Replay Contract

**Original Request:**
```json
{
  "method": "PUT",
  "url": "https://api.example.com/v1/resource/123",
  "headers": {
    "Content-Type": "application/json",
    "Idempotency-Key": "ferrum-{execution_id}"
  },
  "body": {
    "name": "updated-resource",
    "value": 100
  }
}
```

**Compensation Plan:**
```json
{
  "adapter": "http",
  "action": "HttpMutation",
  "config": {
    "http.replay_v1": {
      "method": "PUT",
      "url": "https://api.example.com/v1/resource/123",
      "expected_statuses": [200],
      "idempotency_key_template": "ferrum-{{execution_id}}",
      "body_template": {
        "name": "updated-resource",
        "value": 100
      }
    }
  }
}
```

### PATCH Mutation — Replay Contract

**Original Request:**
```json
{
  "method": "PATCH",
  "url": "https://api.example.com/v1/resource/123",
  "headers": {
    "Content-Type": "application/json",
    "Idempotency-Key": "ferrum-{execution_id}"
  },
  "body": {
    "value": 200
  }
}
```

**Compensation Plan:**
```json
{
  "adapter": "http",
  "action": "HttpMutation",
  "config": {
    "http.replay_v1": {
      "method": "PATCH",
      "url": "https://api.example.com/v1/resource/123",
      "expected_statuses": [200],
      "idempotency_key_template": "ferrum-{{execution_id}}",
      "body_template": {
        "value": 200
      }
    }
  }
}
```

### Failure Cases

**Status Mismatch:**
```json
{
  "execution_id": "{execution_id}",
  "recovered": false,
  "failure_reason": "replay response status 500 did not match expected_statuses [200, 201]",
  "compensation_type": "replay",
  "actual_status": 500
}
```

**No Compensation Plan:**
```json
{
  "execution_id": "{execution_id}",
  "error": "Unsupported",
  "reason_code": "NO_COMPENSATION_PLAN",
  "message": "No http.replay_v1 compensation plan configured for this intent"
}
```

**GET/DELETE Rejected:**
```json
{
  "execution_id": "{execution_id}",
  "error": "Unsupported",
  "reason_code": "GET_DELETE_COMPENSATION_NOT_SUPPORTED",
  "message": "GET and DELETE methods do not support compensation"
}
```

### Idempotency Key Format
```
ferrum-{execution_id}
```
Where `{execution_id}` is the FerrumGate execution identifier assigned at intent creation. The external API must honor this key for replay to succeed.

### Key Takeaways
- HTTP replay compensation sends the same idempotency key as the original request
- External API must implement idempotent semantics
- `recovered=true` confirms replay was sent; it does NOT confirm external undo
- Operators must verify external API idempotency support before relying on HTTP compensation

---

## Example 3 — Backup Restore Evidence Example

### Context
Per `54-operator-signoff-packet.md` §3 and `27-production-evaluation-plan.md` §3.5: Backup/restore drill evidence is required for G2.4. This example shows bounded evidence capture for a non-production restore drill.

### Bounded Scenario
- Test environment: `/tmp/ferrum-drill-restore`
- Source DB: `/tmp/ferrum-drill-restore/source.db`
- Backup file: `/tmp/ferrum-drill-restore/backups/source.db_20260429.db`
- Server must be stopped before restore

### Drill Commands

**Step 1: Create test data:**
```bash
mkdir -p /tmp/ferrum-drill-restore/backups

# Start server and create some data (illustrative)
# ferrumctl intent create --tool fs --action FileWrite --path /tmp/test.txt --content "before-backup"

# Capture execution_id
Intent ID: _______________________________

# Verify data exists before backup
$ sqlite3 /tmp/ferrum-drill-restore/source.db "SELECT * FROM executions LIMIT 5;"
1|before-backup|...
```

**Step 2: Create backup:**
```bash
$ ferrumctl backup create \
  --output /tmp/ferrum-drill-restore/backups/source.db_20260429.db

Backup created: /tmp/ferrum-drill-restore/backups/source.db_20260429.db (8192 bytes)
Database integrity check passed / OK
```

**Step 3: Verify backup:**
```bash
$ ferrumctl backup verify \
  --backup /tmp/ferrum-drill-restore/backups/source.db_20260429.db

Database integrity check passed / OK
```

**Step 4: Stop server (required for restore):**
```bash
# Server must be stopped — restore will refuse if server is running
$ pkill -f ferrumd  # or stop via service manager

Server stopped.
```

**Step 5: Perform restore:**
```bash
$ ferrumctl backup restore \
  --backup /tmp/ferrum-drill-restore/backups/source.db_20260429.db \
  --confirm

Pre-restore snapshot saved: /tmp/ferrum-drill-restore/pre-restore-20260429.db
Restoring from: /tmp/ferrum-drill-restore/backups/source.db_20260429.db
Restore complete.
```

**Step 6: Post-restore verification:**
```bash
# Direct SQLite integrity check
$ sqlite3 /tmp/ferrum-drill-restore/backups/source.db_20260429.db "PRAGMA integrity_check;"
ok

# Verify execution lineage queryable
$ curl -s http://localhost:8080/v1/executions \
  -H "Authorization: Bearer {token}" | jq '.executions | length'
5

# Verify approval queue readable
$ curl -s http://localhost:8080/v1/approvals \
  -H "Authorization: Bearer {token}" | jq '.approvals | length'
3
```

### Evidence Capture Summary

| Check | Result |
|-------|--------|
| Backup created | [ ] PASS |
| Backup verify (`ferrumctl backup verify`) | [ ] PASS |
| `PRAGMA integrity_check` on backup | [ ] PASS (ok) |
| Pre-restore copy preserved | [ ] PASS |
| Restore completed | [ ] PASS |
| `PRAGMA integrity_check` on restored DB | [ ] PASS (ok) |
| Execution lineage queryable after restore | [ ] PASS |
| Approval queue readable after restore | [ ] PASS |

### Evidence Block (for G2.4 template):
```
Source DB: /tmp/ferrum-drill-restore/source.db
Backup: /tmp/ferrum-drill-restore/backups/source.db_20260429.db
Create: Backup created (8192 bytes)
Verify backup: Database integrity check passed / OK
Restore: Pre-restore snapshot saved; Database restored successfully / Restore complete
Verify restored DB: Database integrity check passed / OK
Direct PRAGMA: integrity_check=ok
Post-restore lineage: 5 executions queryable
Post-restore approvals: 3 approvals readable
Drill outcome: SUCCESS — All steps passed
```

### Key Takeaways
- Server must be stopped before restore (exclusive lock required)
- Pre-restore copy is automatically preserved
- `PRAGMA integrity_check=ok` is required evidence
- Execution lineage and approval queue must remain queryable after restore
- RPO = time since last backup; any writes after last backup are lost

---

## Example 4 — `/v1/metrics` Scrape Expected Output

### Context
Per `27-production-evaluation-plan.md` §Operations and `21-v1-single-node-observability-minimums.md`: Metrics endpoint is part of the observability minimums. This example shows bounded expected output for `/v1/metrics` on single-node SQLite v1.

### Endpoint Information
- Path: `/v1/metrics`
- Authentication: Bearer token required (same as other governance routes)
- Content-Type: `text/plain; version=0.0.4` (Prometheus exposition format)

### Bounded Expected Output

```text
# HELP ferrum_executions_total Total number of executions processed
# TYPE ferrum_executions_total counter
ferrum_executions_total{status="completed"} 42
ferrum_executions_total{status="failed"} 3
ferrum_executions_total{status="compensated"} 2

# HELP ferrum_compensation_total Total number of compensation calls
# TYPE ferrum_compensation_total counter
ferrum_compensation_total{recovered="true"} 2
ferrum_compensation_total{recovered="false"} 0

# HELP ferrum_intents_total Total number of intents created
# TYPE ferrum_intents_total counter
ferrum_intents_total{approval_mode="auto"} 38
ferrum_intents_total{approval_mode="manual"} 7

# HELP ferrum_capabilities_total Total number of capabilities minted
# TYPE ferrum_capabilities_total counter
ferrum_capabilities_total{used="true"} 35
ferrum_capabilities_total{used="false"} 10

# HELP ferrum_store_size_bytes SQLite database file size in bytes
# TYPE ferrum_store_size_bytes gauge
ferrum_store_size_bytes 81920

# HELP ferrum_write_queue_depth Current write queue depth
# TYPE ferrum_write_queue_depth gauge
ferrum_write_queue_depth 0

# HELP ferrum_lineage_events_total Total lineage events emitted
# TYPE ferrum_lineage_events_total counter
ferrum_lineage_events_total{event="ActionProposalSubmitted"} 47
ferrum_lineage_events_total{event="PolicyEvaluated"} 47
ferrum_lineage_events_total{event="CapabilityMinted"} 45
ferrum_lineage_events_total{event="ToolCallPrepared"} 45
ferrum_lineage_events_total{event="ToolCallExecuted"} 45
ferrum_lineage_events_total{event="SideEffectPrepared"} 45
ferrum_lineage_events_total{event="SideEffectVerified"} 45
ferrum_lineage_events_total{event="SideEffectCommitted"} 43
ferrum_lineage_events_total{event="SideEffectRolledBack"} 2

# HELP ferrum_http_adapter_requests_total HTTP adapter requests
# TYPE ferrum_http_adapter_requests_total counter
ferrum_http_adapter_requests_total{method="POST",status="success"} 10
ferrum_http_adapter_requests_total{method="POST",status="failure"} 1
ferrum_http_adapter_requests_total{method="PUT",status="success"} 5
ferrum_http_adapter_requests_total{method="PATCH",status="success"} 3

# HELP ferrum_fs_adapter_operations_total Filesystem adapter operations
# TYPE ferrum_fs_adapter_operations_total counter
ferrum_fs_adapter_operations_total{operation="FileWrite",status="success"} 15
ferrum_fs_adapter_operations_total{operation="FileDelete",status="success"} 8
ferrum_fs_adapter_operations_total{operation="FileMove",status="success"} 4

# HELP process_cpu_seconds_total Total user and system CPU time spent in seconds.
# TYPE process_cpu_seconds_total counter
process_cpu_seconds_total 12.34

# HELP process_resident_memory_bytes Resident memory size in bytes.
# TYPE process_resident_memory_bytes gauge
process_resident_memory_bytes 104857600
```

### Scrape Command
```bash
$ curl -s http://localhost:8080/v1/metrics \
  -H "Authorization: Bearer {token}"
```

### Bounded Metrics Notes

**SQLite-specific metrics:**
- `ferrum_store_size_bytes`: Monitors SQLite file size; bounded by single-node disk capacity
- `ferrum_write_queue_depth`: Write queue depth (0 when idle); spikes indicate write pressure

**Single-node constraints:**
- No multi-node metrics (not implemented)
- No PostgreSQL metrics (not implemented)

**Known limitations:**
- Metrics are in-memory counters; reset on server restart
- No long-term persistence of metrics history
- No alerting configuration in FerrumGate (operator manages externally)

### Key Takeaways
- `/v1/metrics` returns Prometheus exposition format
- Bearer auth required (not public)
- Metrics are bounded to single-node SQLite scope
- Operators should scrape and persist metrics externally for historical analysis

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `60-bounded-hardening-examples.md` | `56-adapter-compensation-evidence-matrix.md` | HTTP and git compensation classifications |
| `60-bounded-hardening-examples.md` | `57-workload-compensation-drill-plan.md` | Drill procedures for D3/D4 |
| `60-bounded-hardening-examples.md` | `58-workload-compensation-drill-evidence-template.md` | Drill evidence template |
| `60-bounded-hardening-examples.md` | `59-pilot-readiness-evidence-packet.md` | G2.4 restore drill evidence |
| `60-bounded-hardening-examples.md` | `27-production-evaluation-plan.md` | Observability minimums |
| `60-bounded-hardening-examples.md` | `21-v1-single-node-observability-minimums.md` | Observability requirements |

---

*Document generated: 2026-04-29. Documentation-only examples. Not production configurations. RC-ready/conditional single-node SQLite only.*
