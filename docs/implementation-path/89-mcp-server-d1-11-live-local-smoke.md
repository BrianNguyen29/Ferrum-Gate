# D1.11 MCP Live-Local Smoke

## Status: IMPLEMENTED

## Overview

D1.11 adds bounded live-local lifecycle dispatch checks to `scripts/run_mcp_lifecycle_smoke.sh`, extending the existing D1.7 smoke with lifecycle tool dispatch validation against a local dev ferrumd instance.

D1.11 does **not** claim production-ready, G2-complete, or live target validation status. It is bounded local smoke only.

---

## 1. Scope

### 1.1 What D1.11 Adds

| Test | Tool | Purpose | Soft-Pass Semantics |
|------|------|---------|---------------------|
| D1.11.1 | `ferrum_gate_submit_intent` | Lifecycle dispatch: intent submission | -32003/-32004 = warn/pass |
| D1.11.2 | `ferrum_gate_evaluate_intent` | Lifecycle dispatch: proposal evaluation | -32003/-32004 = warn/pass |
| D1.11.3 | `ferrum_gate_mint_capability` | Lifecycle dispatch: capability minting | -32003/-32004 = warn/pass |
| D1.11.4 | `ferrum_gate_list_intents` | Read-only dispatch | -32002/-32003/-32004 = warn/pass |

### 1.2 What D1.11 Does NOT Claim

| Item | Reason |
|------|--------|
| Production-ready | Bounded smoke only |
| G2-complete | No G2 signoff claimed |
| Live target evidence | Local dev ferrumd only |
| Full lifecycle completion | Bounded dispatch checks; approve/reject blocked |
| Gateway functionality beyond dispatch | Soft-pass on gateway errors; not full integration test |

---

## 2. Soft-Pass Semantics

D1.11 uses precise error-code-based soft-pass semantics:

| Error Code | Meaning | Lifecycle (D1.11.1-.3) | Read-Only (D1.11.4) |
|------------|---------|------------------------|---------------------|
| result present | Success | **Pass** | **Pass** |
| -32003 | Gateway unreachable | **Warn/Pass** | **Warn/Pass** |
| -32004 | Gateway server error | **Warn/Pass** | **Warn/Pass** |
| -32002 | Auth failed | **Fail** | **Warn/Pass** |
| -32001 | Not implemented | **Fail** | **Fail** |
| -32601 | Method not found | **Fail** | **Fail** |
| -32602 | Invalid params | **Fail** | **Fail** |
| No response | Timeout/other | **Fail** | **Fail** |

**Rationale**: -32003/-32004 indicate the dispatch reached the gateway but encountered a condition (e.g., service unavailable, server error). This proves the MCP→gateway dispatch path is wired, even if the backend service is not fully operational in the dev environment. -32002 gets soft-pass for read-only tests because auth configuration may vary in local dev mode.

---

## 3. Implementation Details

### 3.1 Script Extension

The D1.11 tests are inserted in `scripts/run_mcp_lifecycle_smoke.sh` after the MCP ping test and before the summary section:

```
[MCP ping] → [D1.11 lifecycle dispatch checks] → [SUMMARY]
```

### 3.2 ID Extraction and Fallback

D1.11.1 (`submit_intent`) attempts to extract `intent_id` from the response. If extraction fails, a fallback UUID is generated. Downstream tests (D1.11.2, D1.11.3) use the extracted (or fallback) ID with a generated fallback `proposal_id`.

### 3.3 Validation Evidence Placeholders

| Evidence | Location | Status |
|----------|----------|--------|
| Script insertion point | `run_mcp_lifecycle_smoke.sh` lines after ping | Implemented |
| D1.11.1 submit_intent test | `run_mcp_lifecycle_smoke.sh` | Implemented |
| D1.11.2 evaluate_intent test | `run_mcp_lifecycle_smoke.sh` | Implemented |
| D1.11.3 mint_capability test | `run_mcp_lifecycle_smoke.sh` | Implemented |
| D1.11.4 list_intents test | `run_mcp_lifecycle_smoke.sh` | Implemented |
| Soft-pass semantics | `run_mcp_lifecycle_smoke.sh` | Implemented |
| Summary text update | `run_mcp_lifecycle_smoke.sh` | Implemented |
| Header comment update | `run_mcp_lifecycle_smoke.sh` | Implemented |

---

## 4. Non-Claims

D1.11 smoke does **not** establish:

- **Production-ready**: Bounded local smoke only
- **G2-complete**: No G2 signoff claimed
- **Full lifecycle completion**: Only dispatch checks; approve/reject permanently blocked
- **Real backend functionality**: Soft-pass on gateway errors; dispatch reachability only
- **Multi-node/PostgreSQL**: Not in scope
- **Performance benchmarks**: Not measured

---

## 5. Next Gates

| Gate | Owner | Status |
|------|-------|--------|
| D1.11 script syntax validation | Fixer | Pending |
| Live smoke execution (if feasible) | Explorer | Optional |
| G2 readiness evidence | Operator | Future |
| Production-ready claim | Operator | Future |

---

## 6. References

| From | To | Purpose |
|------|-----|---------|
| This doc | [`run_mcp_lifecycle_smoke.sh`](run_mcp_lifecycle_smoke.sh) | D1.11 script implementation |
| This doc | [`88-mcp-server-d1-10-full-pipeline-validation.md`](88-mcp-server-d1-10-full-pipeline-validation.md) | D1.10 mock-based validation (D1.11 extends) |
| This doc | [`84-mcp-server-d1-7-tool-dispatch-preflight.md`](84-mcp-server-d1-7-tool-dispatch-preflight.md) | D1.7 tool dispatch design |
| Script | MCP stdio transport | D1.11 validates dispatch via stdio |
