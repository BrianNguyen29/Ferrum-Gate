# Happy-Path Execute/Verify End-to-End Evidence — fs-first FileWrite

**Date:** 2026-04-11
**Scope:** fs-first FileWrite HTTP flow — full prepare→execute→verify lifecycle via gateway HTTP endpoints

## Overview

`test_execute_and_verify_endpoint_flow_for_file_write` (integration_gateway_flow.rs:4056–4427) is a full happy-path integration test exercising the complete HTTP execute/verify surface for fs-first FileWrite through the gateway router.

This test is the primary integration evidence for the execute/verify HTTP surface on the fs-first FileWrite slice. It is distinct from the 409 invalid-state guard tests in `12-gateway-execute-verify-invalid-state-409-evidence.md` — those cover rejection paths; this covers the success path.

## Test Flow

```
1. Create intent (FileWrite scope, target temp file)
2. Mint capability for file_write tool
3. Authorize execution → creates execution in Proposed state
4. POST /v1/executions/{id}/prepare → creates contract, FsAdapter captures snapshot
   - Contract: adapter_key="fs", state=Prepared
5. POST /v1/executions/{id}/execute with content payload → FsAdapter.execute
   - File written with new content
   - Contract: state=ExecutedAwaitingVerify
   - Execution: state=Running
6. POST /v1/executions/{id}/verify → FsAdapter.verify runs verify_checks
   - Contract: state=Verified
   - Execution: state=Committed
7. File content verified end-to-end
```

## Key Assertions

| Assertion | Location | What it proves |
|-----------|----------|----------------|
| Prepare returns adapter_key="fs" | line 4285 | fs adapter correctly inferred for FileWrite tool |
| Prepare returns contract state=Prepared | line 4289 | contract state guard works |
| Execute returns 200 | line 4322 | HTTP execute endpoint succeeds |
| Execute returns executed=true, result_digest | line 4335–4340 | execute produces expected response fields |
| Contract transitions to ExecutedAwaitingVerify | line 4349 | execute correctly updates contract state |
| File has new content after execute | line 4358 | fs adapter write actually occurred |
| Verify returns 200 | line 4384 | HTTP verify endpoint succeeds |
| Verify returns verified=true | line 4397 | verify produces expected response fields |
| Contract transitions to Verified | line 4407 | verify correctly updates contract state |
| Execution transitions to Committed | line 4420 | full lifecycle state progression confirmed |

## What the Test Proves

- **Full happy-path execute/verify HTTP surface works end-to-end** for fs-first FileWrite
- **State guards exercised correctly** throughout the lifecycle (Prepared → ExecutedAwaitingVerify → Verified; Proposed → Running → Committed)
- **FsAdapter.execute** is reachable via `POST /v1/executions/{id}/execute` and correctly writes file content
- **FsAdapter.verify** is reachable via `POST /v1/executions/{id}/verify` and correctly runs verify_checks
- **File content round-trip confirmed**: original content → written via execute → verified via verify
- **State transitions are persisted**: contract and execution state changes survive through the store

## Conservative Framing

- **fs-first FileWrite only** — git and sqlite adapters are out of scope
- **Single test, single adapter** — not a regression suite; additional coverage exists in fs roundtrip tests (integration_fs_roundtrip.rs)
- **No mid-execute failure testing** — idempotency, retry safety, mid-write failure paths are not exercised here
- **No compensate path in this test** — compensate/restore is exercised in `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035)

## Cross-Reference

- Design note: `11-gateway-execute-verify-surface-design-note.md`
- Invalid-state 409 guard evidence: `12-gateway-execute-verify-invalid-state-409-evidence.md`
- Store+adapter fs-first foundation: `10-q2-fs-foundation-evidence.md`
- Compensate/restore evidence: `10-q2-fs-foundation-evidence.md` (test_compensate_endpoint_restores_file_via_fs_adapter)

## Verification Command

```
cargo test -p ferrum-integration-tests test_execute_and_verify_endpoint_flow_for_file_write
```
Expected: test passes.