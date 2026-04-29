# 58 — Workload Compensation Drill Evidence Template

> **Status**: Created 2026-04-29 — documentation-only template
> **Scope**: Single-node v1 SQLite unless labeled post-v1
> **Purpose**: Operator-fillable evidence template for verifying adapter compensation behavior via drills D1–D6
> **Prerequisite**: Run drills per `57-workload-compensation-drill-plan.md` before completing this template

---

## Purpose

This template provides structured evidence capture for each compensation drill (D1–D6). It is **operator-fillable documentation only** — completing this template does not claim production readiness and does not complete any G2 gate on behalf of the operator.

Each section captures:
- Command/output evidence from drill execution
- `recovered=true/false` result fields
- Accepted exceptions (where behavior deviates from ideal)
- Operator acknowledgment fields

---

## D1 — Filesystem Adapter Drill Evidence

### Drill Reference
`57-workload-compensation-drill-plan.md` §D1

### Preconditions Verified
- [ ] Isolated test directory confirmed
- [ ] No production data in test path
- [ ] Server running with fs adapter registered

### Command / Output Evidence

**FileWrite Drill:**
```bash
# Capture pre-state
$ stat /tmp/ferrum-drill-d1/file.txt 2>&1 || echo "file absent"

# Execute intent (engineer fills intent_id after execution)
Intent ID: _______________________________

# Capture compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-compensation state
$ stat /tmp/ferrum-drill-d1/file.txt 2>&1 || echo "file absent"
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre-state | [ ] file absent  [ ] file existed with content |
| Post-state | [ ] file absent  [ ] file restored to pre-state |
| Failure reason (if any) | _______________________________ |

**FileDelete Drill:**
```bash
# Pre-state capture
$ ls -la /tmp/ferrum-drill-d1/file.txt

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ ls -la /tmp/ferrum-drill-d1/file.txt
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre-state | [ ] file existed  [ ] file absent |
| Post-state | [ ] file absent  [ ] file restored |
| Failure reason (if any) | _______________________________ |

**FileMove Drill:**
```bash
# Pre-state
$ ls -la /tmp/ferrum-drill-d1/src.txt /tmp/ferrum-drill-d1/dst.txt 2>&1

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ ls -la /tmp/ferrum-drill-d1/src.txt /tmp/ferrum-drill-d1/dst.txt 2>&1
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Source restored | [ ] yes  [ ] no |
| Destination removed | [ ] yes  [ ] no |
| Failure reason (if any) | _______________________________ |

### Accepted Exception Fields
| Exception | Description | Operator Initials | Date |
|-----------|-------------|-------------------|------|
| Symlink denied (fail-closed) | Symlink operations correctly rejected | _________________ | _________ |
| Cross-mount boundary denied | Cross-mount move correctly rejected | _________________ | _________ |
| Other: _________________ | _________________________________ | _________________ | _________ |

### Operator Acknowledgment
> **D1 Acknowledgment**: I verify the fs adapter compensation drills were run in an isolated test environment and results captured above.

Operator signature: _________________ Date: _________

---

## D2 — Git Local Ref Operations Drill Evidence

### Drill Reference
`57-workload-compensation-drill-plan.md` §D2

### Preconditions Verified
- [ ] Isolated local git repository (not production)
- [ ] Clean worktree preferred
- [ ] No branch protection on test branches

### Command / Output Evidence

**GitCommit Drill:**
```bash
# Capture pre-state
$ git -C /tmp/ferrum-drill-d2/repo rev-parse HEAD

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ git -C /tmp/ferrum-drill-d2/repo rev-parse HEAD
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre-HEAD | _______________________________ |
| Post-HEAD | _______________________________ |
| HEAD restored | [ ] yes  [ ] no |
| Failure reason (if any) | _______________________________ |

**GitBranchCreate Drill:**
```bash
# Pre-state
$ git -C /tmp/ferrum-drill-d2/repo branch -l

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ git -C /tmp/ferrum-drill-d2/repo branch -l
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Branch deleted | [ ] yes  [ ] no (if on created branch) |
| Failure reason (if any) | _______________________________ |

**GitTagCreate Drill:**
```bash
# Pre-state
$ git -C /tmp/ferrum-drill-d2/repo tag -l

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ git -C /tmp/ferrum-drill-d2/repo tag -l
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Tag deleted | [ ] yes  [ ] no |
| Failure reason (if any) | _______________________________ |

### Accepted Exception Fields
| Exception | Description | Operator Initials | Date |
|-----------|-------------|-------------------|------|
| Dirty worktree blocks rollback | Correctly fails closed when worktree dirty | _________________ | _________ |
| On-branch blocks delete | Correctly refuses branch deletion when HEAD on branch | _________________ | _________ |
| Other: _________________ | _________________________________ | _________________ | _________ |

### Operator Acknowledgment
> **D2 Acknowledgment**: I verify the git local ref operations compensation drills were run and results captured above. Dirty worktree and on-branch fail-closed behavior verified.

Operator signature: _________________ Date: _________

---

## D3 — Git Remote Push Compensation Drill Evidence

### Drill Reference
`57-workload-compensation-drill-plan.md` §D3

> **Critical**: This drill verifies `fail_closed` / remote-dependent semantics. `recovered=false` is the **expected** outcome when remote rollback cannot be proven.

### Preconditions Verified
- [ ] Two local repos: "remote" and "local" with remote configured
- [ ] No branch protection on remote (baseline test)
- [ ] Remote with pre-receive hook blocking deletions (fail-closed test)

### Command / Output Evidence

**Baseline: Successful Remote Push Rollback:**
```bash
# Pre-state
$ git -C /tmp/ferrum-drill-d3/remote rev-parse refs/heads/main

Intent ID: _______________________________

# Push via intent
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ git -C /tmp/ferrum-drill-d3/remote rev-parse refs/heads/main
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Remote ref rolled back | [ ] yes  [ ] no |
| Failure reason (if any) | _______________________________ |

**Fail-Closed: Remote Deletion Blocked (key test):**
```bash
# Configure remote with pre-receive hook blocking deletions
# Run push via intent

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

```json
{
  "recovered": false,
  "rollback_failed": true,
  "failure_reason": "remote_ref deletion denied by pre-receive hook"
}
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| `rollback_failed` | [ ] true  [ ] false |
| `failure_reason` captured | _______________________________ |
| Remote state unchanged | [ ] yes  [ ] no |

**Permission Denied Scenario:**
```bash
Intent ID: _______________________________

$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| `failure_reason` | _______________________________ |

### Accepted Exception Fields
> **Explicit acknowledgment required**: GitPush compensation is NOT guaranteed remote undo. Remote permissions, branch protection, and remote state may prevent rollback.

| Exception | Description | Operator Initials | Date |
|-----------|-------------|-------------------|------|
| `recovered=false` accepted | Push compensation returned false when remote denied | _________________ | _________ |
| Remote-dependent acknowledged | Rollback success depends on remote policy | _________________ | _________ |

### Operator Acknowledgment
> **D3 Acknowledgment**: I verify GitPush compensation is remote-dependent and fail-closed. I accept that `recovered=false` when remote rollback cannot be proven is correct behavior. I have evaluated my remote's branch protection policy.

Operator signature: _________________ Date: _________

---

## D4 — HTTP Strict Replay Compensation Drill Evidence

### Drill Reference
`57-workload-compensation-drill-plan.md` §D4

> **Critical**: HTTP compensation is `replay_compensation`, not true undo. `recovered=true` only confirms replay was sent; external server must honor idempotency.

### Preconditions Verified
- [ ] HTTP test server running locally
- [ ] Server configured to return expected status codes
- [ ] Idempotency key support on test server (if applicable)

### Command / Output Evidence

**Successful Replay Compensation:**
```bash
Intent ID: _______________________________

$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

```json
{
  "recovered": true,
  "compensation_type": "replay",
  "idempotency_key": "ferrum-{execution_id}",
  "replay_metadata": {
    "url": "https://example.com/api/resource",
    "method": "POST",
    "response_status": 200
  }
}
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Compensation type | [ ] replay  [ ] other |
| Idempotency key sent | [ ] yes  [ ] no |
| Server response status | _______________________________ |

**Status Mismatch (compensation failure expected):**
```bash
Intent ID: _______________________________

$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Status mismatch error | [ ] yes  [ ] no |
| Failure reason | _______________________________ |

**GET/DELETE Rejection:**
```bash
Intent ID: _______________________________

$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

```json
{
  "error": "Unsupported",
  "reason_code": "GET_DELETE_COMPENSATION_NOT_SUPPORTED"
}
```

| Field | Value |
|-------|-------|
| Rejected correctly | [ ] yes  [ ] no |
| Error code | _______________________________ |

### Accepted Exception Fields
> **Explicit acknowledgment required**: HTTP replay compensation relies on server-side idempotency support. It is NOT true undo.

| Exception | Description | Operator Initials | Date |
|-----------|-------------|-------------------|------|
| Replay-only compensation accepted | HTTP compensation confirms replay, not undo | _________________ | _________ |
| Server idempotency required | External API must honor idempotency key | _________________ | _________ |
| Other: _________________ | _________________________________ | _________________ | _________ |

### Operator Acknowledgment
> **D4 Acknowledgment**: I verify HTTP compensation is replay-based, not true undo. I confirm my target external API supports idempotent operations and honors the compensation contract.

Operator signature: _________________ Date: _________

---

## D5 — SQLite Adapter Drill Evidence

### Drill Reference
`57-workload-compensation-drill-plan.md` §D5

### Preconditions Verified
- [ ] Isolated test database (not production)
- [ ] Known table schema
- [ ] Bounded SQL mutation shapes (INSERT, UPDATE, DELETE)

### Command / Output Evidence

**INSERT Drill:**
```bash
# Pre-state row count
$ sqlite3 /tmp/ferrum-drill-d5/test.db "SELECT COUNT(*) FROM t;"

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state row count
$ sqlite3 /tmp/ferrum-drill-d5/test.db "SELECT COUNT(*) FROM t;"
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre row count | _______________________________ |
| Post row count | _______________________________ |
| Row removed | [ ] yes  [ ] no |

**UPDATE Drill:**
```bash
# Pre-state values
$ sqlite3 /tmp/ferrum-drill-d5/test.db "SELECT id, val FROM t WHERE id=1;"

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state values
$ sqlite3 /tmp/ferrum-drill-d5/test.db "SELECT id, val FROM t WHERE id=1;"
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre val | _______________________________ |
| Post val | _______________________________ |
| Original restored | [ ] yes  [ ] no |

**DELETE Drill:**
```bash
# Pre-state
$ sqlite3 /tmp/ferrum-drill-d5/test.db "SELECT COUNT(*) FROM t WHERE id=1;"

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state
$ sqlite3 /tmp/ferrum-drill-d5/test.db "SELECT COUNT(*) FROM t WHERE id=1;"
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre state | [ ] row existed  [ ] row absent |
| Post state | [ ] row existed  [ ] row absent |
| Row restored | [ ] yes  [ ] no |

### Accepted Exception Fields
| Exception | Description | Operator Initials | Date |
|-----------|-------------|-------------------|------|
| Schema drift blocks rollback | Schema change correctly triggers fail-closed | _________________ | _________ |
| Other: _________________ | _________________________________ | _________________ | _________ |

### Operator Acknowledgment
> **D5 Acknowledgment**: I verify SQLite compensation correctly restores SQL mutation state. Schema drift correctly triggers fail-closed behavior.

Operator signature: _________________ Date: _________

---

## D6 — Maildraft Adapter Drill Evidence

### Drill Reference
`57-workload-compensation-drill-plan.md` §D6

### Preconditions Verified
- [ ] In-memory draft store operational
- [ ] Draft operations only (not sent email)

### Command / Output Evidence

**Draft Create Drill:**
```bash
Intent ID: _______________________________

$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Draft deleted | [ ] yes  [ ] no |

**Draft Update Drill:**
```bash
# Pre-state content captured
$ curl -s http://localhost:8080/v1/drafts/{draft_id} | jq .

Intent ID: _______________________________

# Compensation response
$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .

# Post-state content
$ curl -s http://localhost:8080/v1/drafts/{draft_id} | jq .
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Pre-content | _______________________________ |
| Post-content | _______________________________ |
| Original restored | [ ] yes  [ ] no |

**Draft Delete Drill:**
```bash
Intent ID: _______________________________

$ curl -s -X POST http://localhost:8080/v1/executions/{intent_id}/compensate \
  -H "Authorization: Bearer {token}" | jq .
```

| Field | Value |
|-------|-------|
| `recovered` | [ ] true  [ ] false |
| Draft recreated | [ ] yes  [ ] no |

### Accepted Exception Fields
| Exception | Description | Operator Initials | Date |
|-----------|-------------|-------------------|------|
| Draft-only semantics verified | Compensation does not recover sent emails | _________________ | _________ |
| Other: _________________ | _________________________________ | _________________ | _________ |

### Operator Acknowledgment
> **D6 Acknowledgment**: I verify maildraft compensation applies to drafts only, not sent emails. Draft-only semantics confirmed.

Operator signature: _________________ Date: _________

---

## Final Signoff

All D1–D6 drills completed with evidence captured above. Exceptions documented where applicable.

> **Note**: This template is repo-side tooling documentation. It does not complete any G2 gate. Operator signoff on this template does not authorize production deployment.

Operator signature: _________________ Date: _________

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `58-workload-compensation-drill-evidence-template.md` | `57-workload-compensation-drill-plan.md` | Drill procedures |
| `58-workload-compensation-drill-evidence-template.md` | `56-adapter-compensation-evidence-matrix.md` | Classification reference |
| `58-workload-compensation-drill-evidence-template.md` | `54-operator-signoff-packet.md` | G2 gate context |
| `58-workload-compensation-drill-evidence-template.md` | `59-pilot-readiness-evidence-packet.md` | G2.1–G2.8 evidence packet |
| `58-workload-compensation-drill-evidence-template.md` | `60-bounded-hardening-examples.md` | Bounded hardening examples |

---

*Template generated: 2026-04-29. Documentation-only — operator signoff still required before production pilot.*
