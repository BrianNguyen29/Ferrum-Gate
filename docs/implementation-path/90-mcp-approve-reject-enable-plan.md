# 90 — MCP Approve/Reject Enablement Plan

> **Status**: Implemented locally — gateway resolve endpoint and MCP approve/reject wiring enabled.
> **Purpose**: Define the safe path to enable MCP approve/reject after adding gateway backend mutation endpoints.
> **Scope**: Local implementation plan. No G2, production, or operator signoff claim.

---

## Current State

MCP is locally operational for the implemented scope, and approve/reject has been enabled through the gateway approval resolve endpoint.

Current approval HTTP surface:

| Endpoint | Method | Status |
| --- | --- | --- |
| `/v1/approvals` | GET | implemented |
| `/v1/approvals/{approval_id}` | GET | implemented |
| `/v1/approvals/{approval_id}/resolve` | POST | implemented |

Current MCP behavior:

| MCP tool | Status |
| --- | --- |
| `ferrum_gate_list_approvals` | implemented read-only |
| `ferrum_gate_approve_intent` | implemented; dispatches to gateway resolve endpoint |
| `ferrum_gate_reject_intent` | implemented; dispatches to gateway resolve endpoint |

Direct MCP provenance emission remains forbidden. Gateway must own provenance emission.

---

## Existing Infrastructure Already Ready

### Protocol layer

Existing proto types are already sufficient:

- `ApprovalResolveRequest { actor, approve: bool, reason }`
- `ApprovalState::{Pending, Granted, Denied, Expired}`

No proto/schema type change is required for a minimal implementation.

### Store layer

Existing store capabilities are ready:

- `ApprovalRepo::resolve(approval_id, state)` exists
- SQLite approval repo implements resolve
- write queue supports resolve operations
- approval transition validation exists

Required transition model:

```text
Pending -> Granted
Pending -> Denied
Pending -> Expired
Granted/Denied/Expired -> terminal
```

### Provenance model

Existing event kinds are available:

- `ApprovalRequested`
- `ApprovalGranted`
- `ApprovalDenied`

Gateway-owned emission is required; MCP must not emit provenance directly.

---

## Implementation Plan

### Phase 1 — Gateway Resolve Endpoint

Owner lane: Rust implementation (`fixer`) with architecture review (`oracle`) if scope expands.

Tasks:

1. Add `POST /v1/approvals/{approval_id}/resolve` route.
2. Accept `ApprovalResolveRequest` body.
3. Fetch approval by ID.
4. Return `404` if approval does not exist.
5. Return `409` if approval is already terminal.
6. Return an expired/forbidden result if approval is expired before resolution.
7. Map `approve=true` to `ApprovalState::Granted`.
8. Map `approve=false` to `ApprovalState::Denied`.
9. Call `ApprovalRepo::resolve()` so transition validation remains store-owned.
10. Emit gateway-owned provenance event:
    - approve -> `ApprovalGranted`
    - reject -> `ApprovalDenied`
11. Return updated approval object or envelope.
12. Add OpenAPI path entry using existing `ApprovalResolveRequest` schema.

Acceptance criteria:

- valid pending approval can be granted
- valid pending approval can be denied
- missing approval returns `404`
- already terminal approval returns `409`
- expired approval cannot be granted as pending
- provenance event emitted by gateway, not MCP

### Phase 2 — MCP Wiring

Tasks:

1. Add gateway HTTP client method for approval resolve.
2. Add `ferrum_gate_approve_intent` tool schema.
3. Add `ferrum_gate_reject_intent` tool schema.
4. Remove both tools from `BLOCKED_TOOLS` only after Phase 1 passes.
5. Add rest mapper calls for approve/reject.
6. Preserve DLP redaction and output sanitization choke point.

Recommended MCP input schema:

```text
approval_id: string, required
reason: string, optional
actor: optional only if existing MCP actor resolution can supply default safely
```

Acceptance criteria:

- tools appear in `tools/list`
- approve/reject dispatch to gateway resolve endpoint
- blocked-tool tests are replaced with real dispatch tests
- error handling preserves JSON-RPC error semantics

### Phase 3 — Tests and Smoke

Backend tests:

- approve pending approval -> granted
- reject pending approval -> denied
- resolve missing approval -> 404
- resolve already granted/denied approval -> 409
- expired approval cannot be granted
- provenance event appears after grant/deny

MCP tests:

- mock gateway approve returns success through `handle_tools_call_with_client`
- mock gateway reject returns success through `handle_tools_call_with_client`
- invalid/missing approval id returns appropriate MCP error
- DLP/sanitization still applies to approval responses

Smoke update:

- change current `-32001` approve/reject smoke checks to real dispatch checks only after backend endpoint exists
- update expected tool count if approve/reject become visible in registry

### Phase 4 — Docs, Audit, and Governance Review

Tasks:

1. Update MCP D1.11 smoke documentation.
2. Add approval resolve endpoint to API docs/OpenAPI.
3. Document provenance boundary: MCP calls REST; gateway emits lineage.
4. Run full verification:
   - `cargo fmt --all -- --check`
   - `cargo check --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `bash scripts/run_mcp_lifecycle_smoke.sh`

---

## Risk Controls

| Risk | Control |
| --- | --- |
| double approval | terminal states remain absorbing; return `409` |
| granting expired approval | check expiry before resolve |
| provenance bypass | gateway-only provenance emission |
| MCP side-effect bypass | MCP only calls gateway REST; no direct store/provenance access |
| digest mismatch | existing I6 approval binding validation remains capability gate |
| accidental tool exposure before backend is ready | keep tools in `BLOCKED_TOOLS` until Phase 1 passes |

---

## Go / No-Go

Recommendation: **GO for implementation when explicitly approved**, because proto/store prerequisites already exist and the missing work is bounded.

Do **not** enable MCP approve/reject before gateway endpoint tests pass.

Do **not** claim production/G2 readiness from this work.

---

## Explicit Non-Claims

- This plan does not implement approve/reject.
- This plan does not complete G2.
- This plan does not provide target evidence.
- This plan does not authorize production pilot use.
- This plan does not change operator signoff status.
