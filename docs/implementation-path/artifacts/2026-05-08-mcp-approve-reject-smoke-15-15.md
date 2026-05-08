# MCP Approve/Reject Smoke Evidence — 2026-05-08 (Post-Approve/Reject)

## Status

**PASS (local-only smoke):** `scripts/run_mcp_lifecycle_smoke.sh` completed with `Passed: 15`, `Failed: 0`.

## Scope

This artifact records the post-approve/reject local smoke execution. It supersedes the pre-approve/reject baseline recorded in [`2026-05-08-mcp-live-local-smoke-d1-11.md`](./2026-05-08-mcp-live-local-smoke-d1-11.md) which showed 13/0 before approve/reject tools were wired.

This is **not** production evidence, **not** G2 evidence, **not** operator signoff, and **not** target-host evidence.

## Evidence Base

| Item | Value |
|------|-------|
| Commit | e57bb8f (MCP approval resolution enabled) |
| Smoke result | 15 passed, 0 failed |
| Script | `scripts/run_mcp_lifecycle_smoke.sh` |
| Mode | Local dev ferrumd, auth disabled, sqlite::memory: |

## Command

```bash
bash scripts/run_mcp_lifecycle_smoke.sh
```

## Environment Observed

```text
ferrumd: <repo>/target/release/ferrumd
ferrum-mcp-server: <repo>/target/release/ferrum-mcp-server
mode: dev, auth disabled, sqlite::memory:
```

## Checks Observed (15 Total)

| Check | Result |
|-------|--------|
| MCP initialize | PASS |
| MCP tools/list returns 19 tools (9 read-only + 8 lifecycle + 2 approval) | PASS |
| `ferrum_gate_approve_intent` present in registry | PASS |
| `ferrum_gate_reject_intent` present in registry | PASS |
| approve dispatch with non-existent approval_id returns error (not METHOD_NOT_FOUND) | PASS |
| reject dispatch with non-existent approval_id returns error (not METHOD_NOT_FOUND) | PASS |
| `ferrum_gate_health` reaches local gateway | PASS |
| `ferrum_gate_submit_intent` present in registry | PASS |
| All 8 lifecycle tools present in registry | PASS |
| Unknown tool returns `METHOD_NOT_FOUND (-32601)` | PASS |
| MCP ping | PASS |
| D1.11.1 `ferrum_gate_submit_intent` live-local dispatch | PASS |
| D1.11.2 `ferrum_gate_evaluate_intent` live-local dispatch | PASS |
| D1.11.3 `ferrum_gate_mint_capability` live-local dispatch | PASS |
| D1.11.4 `ferrum_gate_list_intents` live-local dispatch | PASS |

## Change from Pre-Approve/Reject Baseline

| Metric | Pre-Approve/Reject (2026-05-08-mcp-live-local-smoke-d1-11.md) | Post-Approve/Reject (This Artifact) |
|--------|--------------------------------------------------------------|-------------------------------------|
| Tools in registry | 17 (blocked approve/reject) | 19 (approve/reject wired) |
| approve/reject dispatch | NOT_IMPLEMENTED (-32001) | Returns structured error (not METHOD_NOT_FOUND) |
| Total smoke result | 13 passed, 0 failed | 15 passed, 0 failed |

The approve/reject tools now dispatch to the gateway resolve endpoint rather than returning NOT_IMPLEMENTED.

## Non-Claims

This artifact does **not** establish:

- production readiness;
- G2 completion;
- pilot authorization;
- operator signoff;
- live target-host validation;
- multi-node/PostgreSQL readiness;
- full approve/reject backend workflow (dispatch wiring only; full flow requires target context).

## Next Gates

| Gate | Owner | Status |
|------|-------|--------|
| Path 2 target/operator values | Operator/User | Blocked pending real values |
| G2 readiness evidence | Operator/User | Blocked pending target evidence/signoff |
| Production-ready claim | Operator/User | Not claimed |
| MCP error sanitization review | Engineering | Deferred (bounded design first) |
| MCP DLP semantic scanning | Engineering | Deferred (post-v1 unless trigger) |
| HTTP retry/backoff design | Engineering | Deferred (workload trigger required) |

## References

| From | To | Purpose |
|------|----|---------|
| This artifact | [doc89](../89-mcp-server-d1-11-live-local-smoke.md) | D1.11 smoke documentation |
| This artifact | [doc90](../90-mcp-approve-reject-enable-plan.md) | Approve/reject plan |
| This artifact | [doc91](../91-proposal-todo-status-after-mcp-approve-reject.md) | Proposal status |
| This artifact | [run_mcp_lifecycle_smoke.sh](../../scripts/run_mcp_lifecycle_smoke.sh) | Smoke script |
| Pre-approve/reject baseline | [2026-05-08-mcp-live-local-smoke-d1-11.md](./2026-05-08-mcp-live-local-smoke-d1-11.md) | Historical 13/0 baseline |
