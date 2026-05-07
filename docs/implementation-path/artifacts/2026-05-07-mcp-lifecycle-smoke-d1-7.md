# MCP Lifecycle Smoke Evidence (D1.7)

**Document Type:** Local Lifecycle Smoke/Evidence
**Date:** 2026-05-07
**Owner:** Fixer (implementation)
**Status:** Local evidence only - NOT production/G2/operator claim

---

## Naming Conflict Notice

**IMPORTANT:** This artifact is for D1.7 MCP lifecycle tool dispatch smoke, NOT D1.8.

- **D1.7**: MCP lifecycle tool dispatch (submit_intent, evaluate_intent, mint_capability, authorize_execution, prepare_execution, execute_prepared, verify, compensate)
- **D1.8**: Output sanitization (separate design doc, NOT implemented here)

This artifact documents local lifecycle smoke for D1.7 only. Do not conflate with D1.8.

---

## Scope

### In Scope (D1.7 Local Lifecycle Smoke)
- MCP stdio transport handshake (initialize, ping, tools/list)
- Tool registry: 17 tools (9 read-only + 8 lifecycle)
- Lifecycle tools: 8 wired governance pipeline steps
- Blocked tools: approve_intent, reject_intent (backend absent)
- Error handling: METHOD_NOT_FOUND for unknown tools
- Gateway connectivity: health probe via MCP

### Out of Scope
- Production/G2/operator claims
- External target validation
- Output sanitization (D1.8)
- approve/reject tool enablement
- Full lifecycle end-to-end (requires valid intent/proposal chain)

---

## Evidence Collected

### Test Harness
- **Script**: `scripts/run_mcp_lifecycle_smoke.sh`
- **Pattern**: Follows existing `run_local_auth_smoke.sh` template
- **Transport**: Local stdio JSON-RPC with temp ferrumd instance

### Validation Performed
1. MCP Initialize returns protocol version 2024-11-05
2. tools/list returns 17 tools (9 read-only + 8 lifecycle)
3. Blocked tools (ferrum_gate_approve_intent, ferrum_gate_reject_intent) return NOT_IMPLEMENTED (-32001)
4. All 8 lifecycle tools present in registry
5. Unknown tool returns METHOD_NOT_FOUND (-32601)
6. MCP ping returns success

### Captured Behaviors
| Tool | Expected Behavior | Status |
|------|-------------------|--------|
| initialize | Returns protocol version | Local smoke |
| ping | Returns success | Local smoke |
| tools/list | Returns 17 tools | Local smoke |
| ferrum_gate_approve_intent | NOT_IMPLEMENTED (-32001) | Local smoke |
| ferrum_gate_reject_intent | NOT_IMPLEMENTED (-32001) | Local smoke |
| ferrum_gate_submit_intent | In registry (lifecycle wired) | Local smoke |
| ferrum_gate_health | Gateway reachable | Local smoke |

---

## Blocked Tools Behavior

Per oracle verdict, `ferrum_gate_approve_intent` and `ferrum_gate_reject_intent` are **permanently blocked** due to missing backend endpoints. They are NOT in the tool registry and return:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "Tool 'ferrum_gate_approve_intent' is permanently blocked: backend endpoint absent"
  },
  "id": <request_id>
}
```

This is **expected behavior** - these tools will never be implemented until backend endpoints are added.

---

## D1.7 Lifecycle Tool Chain

The 8 lifecycle tools implement the governance pipeline steps:

```
submit_intent (compile)
    ↓
evaluate_intent (proposal evaluation)
    ↓
mint_capability (capability token)
    ↓
authorize_execution (execution authorization)
    ↓
prepare_execution (rollback contract)
    ↓
execute_prepared (tool execution)
    ↓
verify (result verification)
    ↓
compensate (rollback if needed)
```

Each tool maps to a specific REST endpoint on the gateway:
- `POST /v1/intents/compile`
- `POST /v1/proposals/{id}/evaluate`
- `POST /v1/capabilities/mint`
- `POST /v1/executions/authorize`
- `POST /v1/executions/{id}/prepare`
- `POST /v1/executions/{id}/execute`
- `POST /v1/executions/{id}/verify`
- `POST /v1/executions/{id}/compensate`

---

## Limitations

1. **No end-to-end lifecycle**: Full lifecycle requires valid intent/proposal/capability chain
2. **Local only**: Evidence is local, not external target validation
3. **No G2 claim**: This is implementation evidence, not production readiness
4. **No D1.8**: Output sanitization is separate and not covered here

---

## References

- Doc 84: D1.7 tool-dispatch preflight
- `crates/ferrum-integrations-mcp/src/lib.rs`: Tool registry definitions
- `crates/ferrum-integrations-mcp/src/rest_mapper.rs`: REST endpoint mapping
- `crates/ferrum-integrations-mcp/src/bin/ferrum-mcp-server.rs`: Stdio transport
- `scripts/run_local_auth_smoke.sh`: Template for local smoke scripts

---

## Evidence Quality

**Local smoke evidence only** - This validates:
- MCP stdio transport works
- Tool registry correct (17 tools)
- Lifecycle tools wired (8 tools in registry)
- Blocked tools behave correctly (NOT_IMPLEMENTED)
- Error codes correct (METHOD_NOT_FOUND for unknown)

**Does NOT validate:**
- Gateway REST endpoint correctness (requires mock or live gateway)
- Full lifecycle sequence (requires intent/proposal chain)
- External target behavior
- Production deployment
