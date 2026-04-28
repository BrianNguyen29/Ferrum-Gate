# Execute/Verify Invalid-State 409 Coverage — fs-first FileWrite Evidence

**Date:** 2026-04-11
**Scope:** fs-first FileWrite slice — gateway-level execute/verify HTTP endpoint invalid-state guard coverage

## Overview

The gateway router (`server.rs:155–162`) exposes `POST /v1/executions/{id}/execute` and `POST /v1/executions/{id}/verify` endpoints with explicit state guards that return HTTP 409 Conflict for invalid state transitions.

This note records the integration-level evidence confirming that both endpoints correctly reject invalid-state calls with 409 and `ApiErrorCode::Conflict`.

## State Guards Under Test

### Execute endpoint (`POST /v1/executions/{id}/execute`)
- **Valid execution states:** `ExecutionState::Prepared`, `Authorized`, or `Proposed`
- **Contract state required:** `RollbackState::Prepared`
- Returns 409 Conflict if execution is already `Running` or `Committed`

### Verify endpoint (`POST /v1/executions/{id}/verify`)
- **Valid execution states:** `ExecutionState::Running` or `AwaitingVerification`
- **Contract state required:** `RollbackState::ExecutedAwaitingVerify`
- Returns 409 Conflict if contract is still `Prepared` or already `Verified`

## Evidence: 4 Passing Integration Tests

All four tests are in `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`:

| Test | Line | Guard Tested | Expected Result |
|------|------|-------------|----------------|
| `test_execute_already_running_returns_409` | 4436 | Execute called when execution is `Running` | 409 Conflict |
| `test_execute_already_committed_returns_409` | 4632 | Execute called when execution is `Committed` | 409 Conflict |
| `test_verify_contract_not_executed_returns_409` | 4827 | Verify called when contract is still `Prepared` | 409 Conflict |
| `test_verify_already_verified_returns_409` | 5001 | Verify called when contract is already `Verified` | 409 Conflict |

Each test:
1. Exercises the full HTTP endpoint path via `tower::ServiceExt::oneshot`
2. Asserts `response.status() == StatusCode::CONFLICT`
3. Deserializes the error body and asserts `ApiErrorCode::Conflict`

## What the Tests Prove

- **Execute state guard:** The execute endpoint correctly rejects re-execution on `Running` and `Committed` states, enforcing exactly-once semantics at the HTTP boundary.
- **Verify state guard:** The verify endpoint correctly rejects premature verification (before execute) and duplicate verification (after already verified), enforcing the execute→verify ordering contract.
- **Error fidelity:** Both endpoints return structured `ApiError` with `Conflict` code and a non-empty message, providing actionable feedback for callers.

## Conservative Framing

- Scope is **fs-first FileWrite** only — git/sqlite adapters are out of scope.
- These tests exercise the HTTP endpoint layer; adapter-level state guard unit tests are separate.
- No claims about idempotency, retry safety, or mid-execute failure handling.

## Cross-Reference

- Design note for execute/verify surface: `11-gateway-execute-verify-surface-design-note.md`
- Store-layer invalid-state enforcement (G2): `02-q1-p2-g2-store-integrity-evidence.md`
- fs-first foundation slice: `10-q2-fs-foundation-evidence.md`
