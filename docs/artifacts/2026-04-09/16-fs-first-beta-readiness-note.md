# fs-first Beta Readiness Note — 2026-04-11

**Date:** 2026-04-11
**Scope:** fs-first FileWrite slice — beta readiness assessment
**Purpose:** Compact release-style status note for the fs-first FileWrite beta slice

## Status: BETA

The fs-first FileWrite slice is **ready for beta evaluation** with the following scope:

### Confirmed Working

| Capability | Evidence |
|------------|----------|
| HTTP prepare (creates contract, captures snapshot) | `test_execute_and_verify_endpoint_flow_for_file_write` (integration_gateway_flow.rs:4056–4427) |
| HTTP execute (writes file via FsAdapter) | Same test; `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035) |
| HTTP verify (runs verify_checks, transitions to Verified/Committed) | Same test + 4 invalid-state 409 tests |
| HTTP compensate (restores file via FsAdapter rollback) | `test_compensate_endpoint_restores_file_via_fs_adapter` |
| verify-after-compensate correctly rejected with 409 | `test_verify_after_compensate_returns_409` |
| GET /v1/executions/{id} includes rollback contract for inspection | `ExecutionDetailResponse` in api.rs; `get_execution` in server.rs |
| Store-level persistence (metadata, plan, checks survive SQLite round-trip) | 7 integration tests in `integration_fs_roundtrip.rs` + 3 unit tests in `rollback.rs` |
| 409 invalid-state guards on execute/verify | 4 integration tests in `integration_gateway_flow.rs` |

### Out of Scope for Beta

| Item | Status |
|------|--------|
| Git adapter (ferrum-adapter-git) | Not started |
| SQLite adapter (ferrum-adapter-sqlite) | Not started |
| Compensate state guard (accepts any contract state) | RESOLVED — explicit state guard added to `compensate_execution` in `server.rs`; restricts to `ExecutedAwaitingVerify` contract state with matching execution states (`Running` or `AwaitingVerification`) |
| Idempotency guarantees for execute/verify | Not implemented |
| Mid-execute failure atomic cleanup | Not implemented |

### Beta Exit Criteria

~~Compensate state guard~~ — RESOLVED. The following remain as future improvements (not required for beta):
2. **Idempotency**: Guarantee execute is safe to re-call idempotently on Running state
3. **Atomic cleanup**: Handle mid-execute failures with proper rollback

## Artifact Cross-Reference

- Error model: `15-gateway-execute-verify-compensate-error-model.md`
- Design note: `11-gateway-execute-verify-surface-design-note.md`
- Invalid-state 409 coverage: `12-gateway-execute-verify-invalid-state-409-evidence.md`
- Happy-path evidence: `13-happy-path-execute-verify-evidence.md`
- fs-first foundation: `10-q2-fs-foundation-evidence.md`
- v2 readiness: `14-v2-readiness-note.md`