# D1.11 MCP Live-Local Smoke Evidence — 2026-05-08

## Status

**PASS (local-only smoke):** `scripts/run_mcp_lifecycle_smoke.sh` completed with `Passed: 13`, `Failed: 0`.

## Scope

This artifact records one local execution of the D1.7 + D1.11 MCP lifecycle smoke script against a locally started dev-mode `ferrumd` with auth disabled and in-memory SQLite.

This is **not** production evidence, **not** G2 evidence, **not** operator signoff, and **not** target-host evidence.

## Command

```bash
bash scripts/run_mcp_lifecycle_smoke.sh
```

## Environment Observed

```text
ferrumd: /home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify/target/release/ferrumd
ferrum-mcp-server: /home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify/target/release/ferrum-mcp-server
base_url: http://127.0.0.1:18080
config: generated temporary ferrumgate.mcp-smoke.toml
mode: dev, auth disabled, sqlite::memory:
```

## Result Summary

```text
MCP LIFECYCLE SMOKE TESTS (D1.7 + D1.11)
Passed: 13
Failed: 0
MCP LIFECYCLE SMOKE: ALL CHECKS PASSED
```

## Checks Observed

| Check | Result |
|-------|--------|
| MCP initialize | PASS |
| MCP tools/list returns 17 tools | PASS |
| `ferrum_gate_approve_intent` remains blocked with `NOT_IMPLEMENTED (-32001)` | PASS |
| `ferrum_gate_reject_intent` remains blocked with `NOT_IMPLEMENTED (-32001)` | PASS |
| `ferrum_gate_health` reaches local gateway | PASS |
| `ferrum_gate_submit_intent` present in registry | PASS |
| All 8 lifecycle tools present in registry | PASS |
| Unknown tool returns `METHOD_NOT_FOUND (-32601)` | PASS |
| MCP ping | PASS |
| D1.11.1 `ferrum_gate_submit_intent` live-local dispatch | PASS |
| D1.11.2 `ferrum_gate_evaluate_intent` live-local dispatch | PASS |
| D1.11.3 `ferrum_gate_mint_capability` live-local dispatch | PASS |
| D1.11.4 `ferrum_gate_list_intents` live-local dispatch | PASS |

## D1.11 Lifecycle Dispatch Details

The D1.11 lifecycle dispatch checks returned JSON-RPC `result` responses in this local run:

```text
D1.11.1 submit_intent returns result (intent_id: 6d7575ba-f10c-4c14-82ca-c909aa6b5836)
D1.11.2 evaluate_intent returns result
D1.11.3 mint_capability returns result
D1.11.4 list_intents returns result
```

No D1.11 soft-pass gateway errors were needed in this run.

## Important Correction During This Run

The first live run exposed a smoke-script bug: multiline JSON payloads caused MCP JSON parse errors (`-32700`), and the script incorrectly soft-passed unexpected errors. The script was corrected before recording this evidence:

- D1.11 JSON payloads are now single-line JSON-RPC params.
- `-32700` invalid JSON / parse errors are treated as fatal failures.
- Unexpected errors are fatal failures, not soft-pass.

The passing result above is from the corrected script.

## Non-Claims

This artifact does **not** establish:

- production readiness;
- G2 completion;
- pilot authorization;
- operator signoff;
- live target-host validation;
- multi-node/PostgreSQL readiness;
- approval/reject backend endpoint availability.

Approval/reject MCP tools remain intentionally blocked until backend mutation endpoints exist.

## Next Gates

| Gate | Owner | Status |
|------|-------|--------|
| Path 2 target/operator values | Operator/User | Blocked pending real values |
| G2 readiness evidence | Operator/User | Blocked pending target evidence/signoff |
| Production-ready claim | Operator/User | Not claimed |
| Optional future MCP hardening | Engineering | Deferred |
