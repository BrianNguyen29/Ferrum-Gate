# Gateway-Facing Execute/Verify Surface — Design Note

**Date:** 2026-04-10
**Updated:** 2026-04-11 to reflect implemented execute/verify HTTP endpoints
**Scope:** fs-first FileWrite slice — gateway-level execute/verify HTTP endpoints

## Current State

The gateway router (`server.rs:155–162`) now exposes execute and verify HTTP endpoints:

| Endpoint | Status |
|---|---|
| `POST /v1/executions/{id}/prepare` | EXISTS — creates snapshot, persists contract |
| `POST /v1/executions/{id}/compensate` | EXISTS — restores file via FsAdapter |
| `POST /v1/executions/{id}/execute` | EXISTS — writes file via adapter, guarded by state |
| `POST /v1/executions/{id}/verify` | EXISTS — verifies post-execute state, guarded by state |
| `POST /v1/executions/{id}/evaluate-outcome` | EXISTS — outcome-level evaluation |

## State Guards

Execute and verify endpoints enforce explicit state guards and return 409 Conflict for invalid transitions:

### `POST /v1/executions/{id}/execute`
- **Contract state guard**: `RollbackState::Prepared`
- **Execution state guard**: `ExecutionState::Prepared`, `Authorized`, or `Proposed`
- Returns 409 Conflict if guards fail

### `POST /v1/executions/{id}/verify`
- **Contract state guard**: `RollbackState::ExecutedAwaitingVerify`
- **Execution state guard**: `ExecutionState::Running` or `ExecutionState::AwaitingVerification`
- Returns 409 Conflict if guards fail

## fs-first FileWrite Slice Scope

The implemented slice covers the fs-first FileWrite use case only:
- **Execute**: calls `FsAdapter.execute()` with the contract's target and payload
- **Verify**: runs `verify_checks` (e.g., `FileHashMatches`) against post-execute file state
- **Persist verify-time mutations**: `after_hash` on `RollbackTarget::FilePath` is updated after execute and before verify, and persisted to the contract store for future inspection

## What Remains Out of Scope

- Full Q2 completion (git adapter, sqlite adapter)
- Adapter-specific execute/verify beyond fs FileWrite
- Idempotency guarantees for re-runnable execute
- Error model for execute failures mid-write

## Related Evidence

- Invalid-state 409 guard coverage (4 integration tests): `12-gateway-execute-verify-invalid-state-409-evidence.md`
