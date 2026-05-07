# D1.8 MCP Server Output Sanitization

## Status: IMPLEMENTED — Option A (oracle-approved 2026-05-07)

## Oracle Verdict

**APPROVED_OPTION_A**: Single-point `sanitize_output` using `TaintScoringFirewall` at the `handle_tools_call_with_client` choke point.

- Option C field-key-aware redaction **deferred** to future DLP stage.
- No provenance emission from MCP.
- No production/G2 claim.
- Tests required: control-char stripping, UUID preservation, warnings/messages preservation, empty JSON/no crash, nested JSON structures, error/blocked responses pass through.

## Preflight Scope

Canonical D1.8 is **output sanitization** — specifically `ferrum-firewall` integration at the MCP output boundary. D1.8 is **NOT** a lifecycle smoke test. This document captures current state, gaps, candidate implementation options, risk table, test strategy, and approval checklist.

**Constraint**: Do not claim production-ready or G2-complete. This is implementation documentation.

---

## 0. Current State Analysis

### 0.1 What `ferrum-firewall` Actually Provides

| Component | Status | Capability |
|-----------|--------|------------|
| `SemanticFirewall::sanitize_output` | EXISTS | Top-level API entry point |
| `TaintScoringFirewall` | EXISTS | Recursive control-character stripping only |
| `dlp_findings` | NO-OP | Field-level redaction not implemented |
| Field-level DLP | NOT IMPLEMENTED | DLP rules not wired |
| `ferrum-integrations-mcp` firewall dependency | ABSENT | Zero sanitization calls; no `ferrum-firewall` dependency |

### 0.2 Single MCP Output Choke Point

All 17 tool functions (9 read-only + 8 lifecycle) construct `ToolContent.text` with pretty-printed JSON. The single-point sanitization opportunity is:

```
handle_tools_call_with_client in lib.rs
    │
    ▼ after rest_mapper::map_tool_to_rest returns
    ▼ before JsonRpcResponse::success serialization
```

A single `sanitize_output` call at this choke point covers all 17 tools.

### 0.3 Sensitive Candidate Fields

The following fields are sensitive and require consideration during redaction design:

| Struct | Sensitive Fields | Risk |
|--------|-----------------|------|
| `CapabilityLease` | `resource_bindings`, `tool_binding`, `argument_constraints`, `approval_binding`, `metadata` | High — bindings expose capability scope |
| `RollbackContract` | `adapter_key`, `target`, `compensation_plan`, `metadata` | High — adapter/target expose infrastructure |
| `ExecutionRecord` | `metadata`, `result_digest` | Medium — metadata may contain internal data |
| `ActionProposal` | `raw_arguments`, `taint_inputs`, `metadata` | High — raw args expose input |
| `IntentEnvelope` | `resource_scope`, `trust_context`, `metadata` | Medium — scope exposes resource access |
| `ApiError` | `message` | Low — error messages are user-facing |

### 0.4 No-Over-Redaction Principle

Redaction must preserve:
- UUID IDs (required for correlation)
- `reason` / `message` / `warnings` fields (required for debugging)
- `rollback_contract` diagnostics (required for operator visibility)

Control characters should be stripped. Field-level DLP is future work.

---

## 1. Implementation Options

### Option A: Single-Point `sanitize_output` at lib.rs Choke Point

**Description**: Add one `sanitize_output` call in `handle_tools_call_with_client` after `rest_mapper::map_tool_to_rest` returns and before `JsonRpcResponse::success` serialization. Uses `TaintScoringFirewall` for control-char stripping.

**Pros**:
- Single insertion point; covers all 17 tools
- Minimal code change
- Uses existing `SemanticFirewall::sanitize_output`

**Cons**:
- `TaintScoringFirewall` only does control-char stripping
- No field-level redaction
- No DLP
- May over-redact or under-redact depending on `SemanticFirewall` implementation

**Status**: `ferrum-firewall` dependency would need to be added to `ferrum-integrations-mcp`.

### Option C: Field-Key-Aware Redaction

**Description**: Implement field-key-aware redaction in `lib.rs` that:
1. Knows the structure of sensitive response structs
2. Redacts known sensitive field keys
3. Preserves UUIDs, messages, warnings, and diagnostics

**Pros**:
- Precise control over what is redacted
- Aligns with no-over-redaction principle
- Independent of `ferrum-firewall` internals

**Cons**:
- More complex implementation
- Must be kept in sync if response schemas change
- Requires schema knowledge in MCP layer

**Status**: New code in `ferrum-integrations-mcp`.

### Recommended: Option A + C (Hybrid)

Use Option A for control-char stripping via `TaintScoringFirewall`, then apply Option C for field-key-aware redaction. Oracle should make final decision.

---

## 2. Risk Table

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Secret leakage via MCP response | Medium | High | Field-key-aware redaction before release |
| Control characters in JSON causing parsing issues | Low | Medium | Option A control-char stripping |
| Over-redaction breaking client parsing | Medium | Medium | No-over-redaction; preserve UUIDs/messages |
| Under-redaction exposing sensitive fields | Medium | High | Oracle review; field enumeration |
| `ferrum-firewall` API breaking change | Low | Medium | Pin version; integration test |
| DLP bypass via novel field names | Low | High | Field-key-aware approach; future DLP work |

---

## 3. Test Strategy

### 3.1 Unit Tests (Required)

| Test | Description | Expected Outcome |
|------|-------------|-------------------|
| Control-char stripping | JSON with `\x00-\x1f` chars in text fields | Chars removed; JSON valid |
| UUID preservation | JSON with UUID fields | UUIDs intact |
| Message preservation | JSON with `message`/`reason`/`warning` fields | Messages intact |
| Rollback contract preservation | JSON with `rollback_contract` | Adapter/target redacted; diagnostics preserved |
| Sensitive field redaction | JSON with `metadata`, `resource_bindings`, etc. | Fields redacted per policy |
| Choke-point coverage | All 17 tools return sanitized output | No tool bypasses sanitization |

### 3.2 Integration Tests (Required Before Oracle Gate)

| Test | Description | Expected Outcome |
|------|-------------|-------------------|
| End-to-end tool call | MCP tool call → sanitized JSON response | Output sanitized |
| Auth-gated tool call | With bearer token | Sanitization applied |
| Blocked tool call | Unknown/mutating tool | Error returned (not sanitized) |

### 3.3 Negative tests (Required)

| Test | Description | Expected Outcome |
|------|-------------|-------------------|
| Empty JSON | `{}` response | No crash; passes through |
| Malformed JSON | Binary data in text field | Sanitized or error |
| Very large response | Large JSON blob | Sanitization completes within SLA |

---

## 4. Oracle Gate Checklist

Oracle-approved items (2026-05-07):

- [x] **Option A confirmed**: Single-point `sanitize_output` at lib.rs choke point
- [x] **Option C deferred**: Field-key-aware redaction scope deferred to future DLP stage
- [x] **`ferrum-firewall` dependency**: Approved for `ferrum-integrations-mcp`
- [x] **No-over-redaction rules**: Oracle approves preservation list (UUIDs, messages, warnings, rollback_contract diagnostics)
- [x] **Test coverage**: Oracle approves test strategy above
- [x] **DLP deferral acknowledged**: Field-level DLP is future work; not in scope for D1.8

Oracle verdict captured in writing (this document).

---

## 5. Implementation Phases (Post-Oracle Approval)

### Phase D1.8.1: `ferrum-firewall` Integration (Option A)

| # | Item | Status |
|---|------|--------|
| D1.8.1.1 | Add `ferrum-firewall` dependency to `ferrum-integrations-mcp/Cargo.toml` | **Done** |
| D1.8.1.2 | Implement `sanitize_output` call at lib.rs choke point | **Done** |
| D1.8.1.3 | Add unit tests for control-char stripping | **Done** |
| D1.8.1.4 | Verify all 17 tools covered by single choke point | **Done** |

### Phase D1.8.2: Field-Key-Aware Redaction (Option C)

| # | Item | Status |
|---|------|--------|
| D1.8.2.1 | Enumerate sensitive field keys per struct | Deferred |
| D1.8.2.2 | Implement field-key-aware redaction function | Deferred |
| D1.8.2.3 | Implement no-over-redaction preservation rules | Deferred |
| D1.8.2.4 | Add unit tests for field redaction | Deferred | |

### Phase D1.8.3: Integration and Validation

| # | Item | Status |
|---|------|--------|
| D1.8.3.1 | End-to-end integration test | Future |
| D1.8.3.2 | Auth-gated integration test | Future |
| D1.8.3.3 | Performance/size edge-case tests | Future |
| D1.8.3.4 | Oracle review gate | Future |

---

## 6. Explicit Non-Claims

- **Option A implemented (2026-05-07).** Option C field-key-aware redaction deferred.
- **No production-ready claim.** Output sanitization is a security feature requiring oracle review.
- **No G2-complete claim.** G2.1–G2.8 remain pending.
- **No DLP claim.** Field-level DLP is future work; `dlp_findings` is no-op.
- **`TaintScoringFirewall` is sufficient for Option A.** Control-char stripping only; field-level redaction requires Option C (deferred).
- **No over-redaction safety claim beyond Option A.** Oracle approved no-over-redaction rules for Option A.

---

## 7. References

| From | To | Purpose |
|------|-----|---------|
| This doc | [`74-mcp-server-phase-d1-governance-design.md`](74-mcp-server-phase-d1-governance-design.md) | D1.8 is in governance pipeline (step 11) |
| This doc | [`84-mcp-server-d1-7-tool-dispatch-preflight.md`](84-mcp-server-d1-7-tool-dispatch-preflight.md) | D1.7 preflight for comparison |
| This doc | [`README.md`](README.md) | Reading order entry |

---

*Document created: 2026-05-07. Preflight documentation only. D1.8 implementation is oracle-gated. No production-ready claim.*
