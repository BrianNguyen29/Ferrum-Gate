# 57 — Workload Compensation Drill Plan

> **Status**: Created 2026-04-29 — documentation
> **Scope**: Single-node v1 SQLite unless labeled post-v1
> **Purpose**: Operator-facing drill plan to verify compensation behavior for each adapter before production use

---

## Purpose

This document provides concrete drill procedures for verifying adapter compensation behavior in a controlled test environment before production deployment. Each drill covers:

- Preconditions required to run the drill
- Step-by-step actions to execute
- Expected evidence to collect
- Operator acceptance criteria

This plan supplements `56-adapter-compensation-evidence-matrix.md` with actionable verification procedures.

---

## Drill Overview

| # | Adapter | Drill Name | Classification Verified | Risk Level |
|---|---------|------------|------------------------|------------|
| D1 | fs | Filesystem mutation undo | `real_undo` for FileWrite/FileDelete/FileMove | LOW |
| D2 | git | Local ref operations undo | `real_undo` for GitCommit/GitBranchCreate/GitTagCreate | LOW |
| D3 | git | Remote push compensation | `fail_closed` / remote-dependent | MED |
| D4 | http | Strict replay compensation | `replay_compensation`, not true undo | MED |
| D5 | sqlite | SQL mutation undo | `real_undo` for INSERT/UPDATE/DELETE | LOW |
| D6 | maildraft | Draft store undo | `real_undo` for create/update/delete | LOW |

---

## D1 — Filesystem Adapter Drill

### Preconditions
- FerrumGate server running with fs adapter registered
- Temporary directory accessible for test files
- No production data in test directory

### Actions

1. **FileWrite drill**:
   - Create a file with known content via intent/execution
   - Execute the intent
   - Trigger compensation/rollback
   - Verify file content restored OR deleted (depending on operation)

2. **FileDelete drill**:
   - Create a file
   - Delete the file via intent/execution
   - Trigger compensation/rollback
   - Verify file restored from snapshot

3. **FileMove drill**:
   - Create source file
   - Move to destination via intent/execution
   - Trigger compensation/rollback
   - Verify file moved back to source

### Expected Evidence
- Compensation succeeds with `recovered=true`
- File content/path matches pre-execute state
- No data loss beyond the expected rollback point

### Operator Acceptance Criteria
- [ ] FileWrite compensation restores original content or removes new file
- [ ] FileDelete compensation restores deleted file
- [ ] FileMove compensation moves file back to source
- [ ] Symlink operations are denied (fail-closed)

---

## D2 — Git Local Ref Operations Drill

### Preconditions
- Local git repository for testing (not production repo)
- Clean worktree preferred
- No branch protection on test branches

### Actions

1. **GitCommit drill**:
   - Capture HEAD before commit
   - Execute intent that creates a commit
   - Trigger compensation/rollback
   - Verify HEAD restored to captured SHA

2. **GitBranchCreate drill**:
   - Create a new branch via intent/execution
   - Trigger compensation/rollback
   - Verify branch deleted (if safe to delete)

3. **GitTagCreate drill**:
   - Create a tag via intent/execution
   - Trigger compensation/rollback
   - Verify tag deleted

### Expected Evidence
- Compensation succeeds with `recovered=true`
- Local refs match pre-execute state
- Dirty worktree blocks rollback (fail-closed)

### Operator Acceptance Criteria
- [ ] GitCommit rollback restores HEAD to captured SHA
- [ ] GitBranchCreate rollback deletes created branch (if not on created branch)
- [ ] GitTagCreate rollback deletes created tag
- [ ] Dirty worktree blocks rollback with clear error

---

## D3 — Git Remote Push Compensation Drill (Critical)

> **Important**: This drill verifies the `fail_closed` / remote-dependent semantics documented in `56-adapter-compensation-evidence-matrix.md`.

### Preconditions
- Two local git repositories: one as "remote", one as "local"
- Local repo configured with remote pointing to "remote" repo
- No branch protection hooks on remote (for baseline test)
- Second test: remote with pre-receive hook blocking deletions (for fail-closed test)

### Actions

1. **Baseline: Successful remote push rollback**:
   - Create a commit in local repo
   - Push to remote via intent/execution
   - Trigger compensation/rollback
   - Verify remote ref deleted (or rolled back if possible)

2. **Fail-closed: Remote deletion blocked** (key test):
   - Set up remote with pre-receive hook rejecting branch deletions
   - Push to remote via intent/execution
   - Trigger compensation/rollback
   - **Verify `recovered=false` with failure metadata**
   - Verify remote state unchanged

3. **Permission denied scenario**:
   - Configure remote with no-delete permissions
   - Repeat push and rollback
   - **Verify `recovered=false`** (fail-closed behavior)

### Expected Evidence
- Baseline: `recovered=true` when remote rollback succeeds
- Fail-closed: `recovered=false` with `rollback_failed=true` and `failure_reason` in metadata
- Remote state unchanged when rollback fails

### Operator Acceptance Criteria
- [ ] Push compensation succeeds when remote allows deletion
- [ ] Push compensation returns `recovered=false` when remote denies deletion
- [ ] Push compensation returns `recovered=false` when remote has no permissions
- [ ] Failure includes clear `failure_reason` metadata
- [ ] **Document explicitly**: GitPush compensation is NOT guaranteed remote undo — it is remote-dependent and fail-closed

### Production Note
> **GitPush compensation does NOT guarantee remote undo.** The remote repository may:
> - Refuse to delete the branch ref (branch protection)
> - Lack permissions for the credentials provided
> - Have already incorporated the pushed commits into other branches
>
> Operators must evaluate their remote's branch protection policy before relying on GitPush compensation.

---

## D4 — HTTP Strict Replay Compensation Drill (Critical)

> **Important**: This drill verifies the `replay_compensation` semantics documented in `56-adapter-compensation-evidence-matrix.md`. HTTP compensation is NOT true undo.

### Preconditions
- HTTP test server running locally (or controlled test endpoint)
- Server configured to return expected status codes
- Idempotency key support on test server (optional, for full drill)

### Actions

1. **Successful replay compensation**:
   - Execute HTTP POST/PUT/PATCH mutation via intent
   - Configure `http.replay_v1` compensation plan
   - Trigger compensation
   - Verify replay request sent with idempotency key
   - Verify response status matches expected statuses

2. **Replay with different response status**:
   - Configure `expected_statuses` in compensation plan
   - Trigger compensation when server returns different status
   - **Verify compensation fails** (status mismatch)

3. **No compensation plan**:
   - Execute HTTP mutation without compensation plan
   - Trigger compensation
   - **Verify `Unsupported` error with reason codes**

4. **GET/DELETE rejection**:
   - Attempt HTTP GET or DELETE with compensation
   - **Verify fail-closed rejection**

### Expected Evidence
- Successful replay: `recovered=true` with replay metadata
- Status mismatch: Error with clear reason
- No compensation plan: `Unsupported` error with `NO_COMPENSATION_PLAN` reason code
- GET/DELETE: `Unsupported` error

### Operator Acceptance Criteria
- [ ] POST/PUT/PATCH replay compensation succeeds with valid `http.replay_v1` plan
- [ ] Compensation fails on status mismatch
- [ ] Missing compensation plan returns clear error
- [ ] GET/DELETE mutations are rejected for compensation
- [ ] **Document explicitly**: HTTP replay compensation is NOT true undo — it relies on server-side idempotency

### Production Note
> **HTTP replay compensation does NOT provide true undo.** It replays a compensating request that the external API must honor. True undo requires:
> - Server-side support for the idempotency key
> - Server implementing compensation/rollback semantics
> - No side effects from the original request that the server cannot reverse
>
> Operators must verify their external API supports idempotent operations before relying on HTTP compensation.

---

## D5 — SQLite Adapter Drill

### Preconditions
- Clean test database (not production)
- Known table schema
- Bounded SQL mutation shapes (INSERT, UPDATE, DELETE)

### Actions

1. **INSERT drill**:
   - Capture row count before insert
   - Execute INSERT via intent/execution
   - Trigger compensation/rollback
   - Verify row count restored

2. **UPDATE drill**:
   - Capture original row values
   - Execute UPDATE via intent/execution
   - Trigger compensation/rollback
   - Verify original values restored

3. **DELETE drill**:
   - Capture deleted row data
   - Execute DELETE via intent/execution
   - Trigger compensation/rollback
   - Verify row restored

### Expected Evidence
- Compensation succeeds with `recovered=true`
- Row data matches pre-execute state
- Schema drift blocked (fail-closed if schema changed)

### Operator Acceptance Criteria
- [ ] INSERT compensation removes inserted row
- [ ] UPDATE compensation restores original values
- [ ] DELETE compensation restores deleted row
- [ ] Schema drift blocks rollback with clear error

---

## D6 — Maildraft Adapter Drill

### Preconditions
- In-memory draft store (no external dependencies)
- Draft operations only (not sent email)

### Actions

1. **Draft create drill**:
   - Create draft via intent/execution
   - Trigger compensation/rollback
   - Verify draft deleted

2. **Draft update drill**:
   - Create and update draft
   - Capture original draft content
   - Trigger compensation/rollback
   - Verify original content restored

3. **Draft delete drill**:
   - Create and delete draft
   - Capture deleted draft content
   - Trigger compensation/rollback
   - Verify draft restored

### Expected Evidence
- Compensation succeeds with `recovered=true`
- Draft state matches pre-execute state
- Draft-only semantics verified (no sent email recovery)

### Operator Acceptance Criteria
- [ ] Create compensation deletes created draft
- [ ] Update compensation restores original draft content
- [ ] Delete compensation recreates deleted draft
- [ ] **Document explicitly**: Maildraft compensation applies to drafts only, not sent emails

---

## Running the Drill Plan

### Prerequisites
- Isolated test environment (not production)
- Access to FerrumGate server logs
- Test repositories/endpoints as specified in each drill

### Execution Order
1. Run D1 (fs) — lowest risk, establishes baseline
2. Run D2 (git local) — confirms local ref handling
3. Run D5 (sqlite) — confirms SQL compensation
4. Run D6 (maildraft) — confirms draft-only semantics
5. Run D3 (git remote) — **critical for remote-dependent semantics**
6. Run D4 (http replay) — **critical for replay compensation semantics**

### Evidence Collection
For each drill, collect:
- Compensation result (`recovered` field)
- Adapter metadata (failure reasons, if any)
- Pre/post state comparisons
- Logs showing phase transitions

### Sign-off
Operator sign-off requires:
- All drills passing OR documented exceptions
- Explicit acknowledgment of remote-dependent (D3) and replay-only (D4) semantics
- No production deployment until sign-off complete

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `57-workload-compensation-drill-plan.md` | `56-adapter-compensation-evidence-matrix.md` | Evidence basis for drill classifications |
| `57-workload-compensation-drill-plan.md` | `33-feature-completion-backlog.md` | P6 non-uniform compensation status |
| `57-workload-compensation-drill-plan.md` | `45-current-feature-audit.md` | G2 gap tracking |
| `57-workload-compensation-drill-plan.md` | `52-d6-priority-expansion-list.md` | Priority 5 adapter hardening |

---

*Document created: 2026-04-29. Part of P6 documentation for non-uniform adapter compensation.*
