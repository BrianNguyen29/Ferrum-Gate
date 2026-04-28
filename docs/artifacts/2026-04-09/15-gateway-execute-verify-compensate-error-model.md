# Gateway Execute/Verify/Compensate Error Model — fs-first FileWrite

**Date:** 2026-04-11
**Scope:** fs-first FileWrite slice — current error behavior for execute, verify, compensate endpoints
**Purpose:** Document current error model as it exists today; conservative wording where behavior is rough

## Overview

The gateway exposes three state-machine endpoints for the fs-first FileWrite slice:
- `POST /v1/executions/{id}/execute`
- `POST /v1/executions/{id}/verify`
- `POST /v1/executions/{id}/compensate`

This note documents the current error behavior for each, including edge cases and known rough edges.

## Execute Endpoint

**Route:** `POST /v1/executions/{id}/execute`

**Request body:** `ExecuteExecutionRequest` with optional `payload` JSON

**State guards:**
- Contract state must be `RollbackState::Prepared`
- Execution state must be `ExecutionState::Prepared`, `Authorized`, or `Proposed`

**Error responses:**
| Condition | HTTP Status | Error Code |
|-----------|-------------|------------|
| Execution not found | 404 | NotFound |
| Contract not found | 404 | NotFound |
| No rollback contract | 404 | NotFound |
| Contract not Prepared | 409 | Conflict |
| Execution not in valid states | 409 | Conflict |
| Adapter execute fails | 500 | Internal/AdapterFailure |
| Store write fails | 500 | Internal |

**Known rough edges:**
- No idempotency guarantee for re-running execute on a Running execution
- Mid-execute failure (e.g., partial file write) is not cleaned up atomically
- If execute succeeds but the contract update fails, the contract may be out of sync with execution state

## Verify Endpoint

**Route:** `POST /v1/executions/{id}/verify`

**State guards:**
- Contract state must be `RollbackState::ExecutedAwaitingVerify`
- Execution state must be `ExecutionState::Running` or `AwaitingVerification`

**Error responses:**
| Condition | HTTP Status | Error Code |
|-----------|-------------|------------|
| Execution not found | 404 | NotFound |
| Contract not found | 404 | NotFound |
| Contract not ExecutedAwaitingVerify | 409 | Conflict |
| Execution not in valid states | 409 | Conflict |
| Adapter verify fails | 500 | Internal/AdapterFailure |
| Store write fails | 500 | Internal |

**Post-verify behavior (correct):**
- On verified=true: contract → `RollbackState::Verified`, execution → `ExecutionState::Committed`
- On verified=false: contract → `RollbackState::Failed`, execution → `ExecutionState::Failed`
- `after_hash` on `RollbackTarget::FilePath` is updated and persisted for future inspection

**Known rough edges:**
- Verify on a `Compensated` contract returns 409 (correct — compensate is gated by explicit state guard)
- If verify succeeds but the contract update fails, the contract may reflect an incorrect state

## Compensate Endpoint

**Route:** `POST /v1/executions/{id}/compensate`

**State guards:** YES — explicit state guard added (WS-Compensate)

**Valid states for compensate:**
| Contract state | Execution state |
|----------------|-----------------|
| `ExecutedAwaitingVerify` | `Running` |
| `ExecutedAwaitingVerify` | `AwaitingVerification` |

All other state combinations return HTTP 409 Conflict with `ApiErrorCode::Conflict`.

**Error responses:**
| Condition | HTTP Status | Error Code |
|-----------|-------------|------------|
| Execution not found | 404 | NotFound |
| Contract not found | 404 | NotFound |
| Contract/execution not in valid states | 409 | Conflict |
| Adapter compensate fails | 500 | Internal/AdapterFailure |
| Store write fails | 500 | Internal |

**Post-compensate behavior:**
- Contract → `RollbackState::Compensated`
- Execution → `ExecutionState::Compensated`
- FsAdapter compensate path invokes `rollback()` to restore file from snapshot

**Known rough edges:**
- ~~No state guard~~ — FIXED: explicit state guard now restricts compensate to `ExecutedAwaitingVerify` contract state with matching execution states (`Running` or `AwaitingVerification`)
- Idempotency: calling compensate twice on the same contract returns 409 Conflict (repeat compensate not allowed; state guard is idempotent)
- If compensate succeeds but the contract update fails, the contract may show a non-Compensated state while the file has already been restored

## State Transition Summary

```
Execute:  Prepared + Prepared/Authorized/Proposed → ExecutedAwaitingVerify + Running
Verify:   ExecutedAwaitingVerify + Running/AwaitingVerification → Verified + Committed
                                                                   → Failed + Failed
Compensate: ExecutedAwaitingVerify + Running/AwaitingVerification
            → Compensated + Compensated
            (all other states → 409 Conflict)
```

## Cross-Reference

- Design note: `11-gateway-execute-verify-surface-design-note.md`
- Invalid-state 409 coverage: `12-gateway-execute-verify-invalid-state-409-evidence.md`
- Happy-path evidence: `13-happy-path-execute-verify-evidence.md`
- fs-first foundation: `10-q2-fs-foundation-evidence.md`