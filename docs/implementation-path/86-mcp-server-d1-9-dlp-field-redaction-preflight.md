# D1.9 MCP Server DLP / Field Redaction

## Status: PHASE 1 APPROVED AND IMPLEMENTED (Option B)

## Oracle Verdict

**APPROVED: Option B phased rollout.**

- **Phase 1 minimum approved**: `raw_arguments` and `metadata` only.
- **Redaction mechanism**: Replace matched field values with string `"[REDACTED]"`.
- **Recursive JSON walk**: Object keys checked, arrays recurse, numbers/bools/nulls pass through.
- **Metadata strategy**: Whole-field redaction (no per-key enumeration).
- **No regex/heuristic DLP**: Option A/C deferred to Phase 2.
- **No FirewallContext change**: Option D deferred to future work.
- **No production/G2 claim**: D1.9 Phase 1 is security hardening, not production-ready claim.
- **Provenance emission**: Gateway-owned, not implemented in MCP.

Future Phase 2 fields (per oracle deferred list): `resource_bindings`, `target`, `compensation_plan.args`, `resource_scope`, `result_digest` — not implemented unless doc-only as deferred.

---

## 0. Preflight Scope

Canonical D1.9 is **DLP / field-level redaction** — extending D1.8 output sanitization with context-aware, field-key-aware redaction of sensitive data in MCP responses. D1.9 is **not implemented**. This document is preflight analysis only.

**Constraint**: Do not claim DLP implemented or production-ready/G2-complete. This is draft preflight documentation.

---

## 1. Current State Analysis

### 1.1 What `ferrum-firewall` Actually Provides

| Component | Status | Capability |
|-----------|--------|------------|
| `SemanticFirewall::sanitize_output` | EXISTS | Recursive control-character stripping only |
| `TaintScoringFirewall` | EXISTS | Control-char stripping; field-key-blind |
| `SemanticFirewall::dlp_findings` | NO-OP STUB | Returns `Vec<String>`; advisory only, no transformation |
| `FirewallContext` | EXISTS | Context struct (source, intent, trust_score, is_external, attributes) |
| `FirewallContext` passed to sanitize_output | **NOT PASSED** | `sanitize_output` and `dlp_findings` take no context |
| Field-level DLP | NOT IMPLEMENTED | DLP rules not wired; no struct-aware redaction |
| `ferrum-integrations-mcp` DLP call | ABSENT | D1.8 choke point calls `sanitize_output` only; no DLP/redact call |

### 1.2 D1.8 Choke Point — Current State

D1.8 (oracle-approved 2026-05-07) established a single-point `sanitize_output` call at the MCP output boundary using `TaintScoringFirewall`:

```
handle_tools_call_with_client in lib.rs
    │
    ▼ after rest_mapper::map_tool_to_rest returns
    ▼ TaintScoringFirewall::new().sanitize_output(value)  ← D1.8 APPROVED (Option A)
    ▼ before JsonRpcResponse::success serialization
```

D1.9 would add a **redact/DLP call** alongside or extend the sanitizer at this same choke point.

### 1.3 Sensitive Taxonomy

Primary DLP targets identified for field-level redaction:

| Struct | High-Risk Fields | Rationale |
|--------|-------------------|-----------|
| `ActionProposal` | `raw_arguments`, `metadata` | Raw args expose raw input; metadata may contain internal data |
| `CapabilityLease` | `resource_bindings`, `tool_binding`, `argument_constraints`, `approval_binding`, `metadata` | Bindings expose capability scope and permitted actions |
| `RollbackContract` | `target`, `compensation_plan.args`, `metadata` | Adapter/target expose infrastructure; args expose parameters |
| `IntentEnvelope` | `resource_scope`, `trust_context`, `metadata` | Scope exposes resource access; trust_context is internal |
| `ExecutionRecord` | `metadata`, `result_digest` | Metadata may contain internal data |

**Hardest target**: `JsonMap` (`IndexMap<String, Value>`) — arbitrary key-value metadata with no fixed schema.

### 1.4 No-Over-Redaction Principle

Redaction **must preserve** the following (required for correlation, debugging, and operator visibility):

- UUID IDs (required for correlation across logs/responses)
- `reason` / `message` / `warnings` fields (required for debugging)
- `matched_rule_ids` (required for policy debugging)
- `decision` / `state` / `status` enums (required for workflow state)
- Booleans (required for conditional logic)
- `rollback_contract` structure (required for operator visibility into rollback state)
- `action_type` (required for routing logic)
- `correlation_id` (required for distributed tracing)

---

## 2. Implementation Options

### Option A: Field-Key-Blind Replace (No Change to Current API)

**Description**: Extend `sanitize_output` to do regex/heuristic pattern replacement on string values (e.g., credit card numbers, API keys, passwords) without field-key awareness.

**Pros**:
- No trait/signature changes required
- Can catch sensitive data even in unexpected field names
- Simple to implement on top of D1.8

**Cons**:
- Blind — may miss context-specific sensitive data
- May false-positive on non-sensitive data that resembles PII/credentials
- No struct awareness; cannot apply no-over-redaction rules precisely

**Status**: Candidate for quick win; oracle decision required.

---

### Option B: Struct-Aware Redaction (Phase 1 Implemented)

**Description**: Implement field-key-aware redaction that:
1. Knows the structure of sensitive response structs
2. Redacts known sensitive field keys (`raw_arguments`, `metadata` in Phase 1)
3. Preserves no-over-redaction list (UUIDs, messages, warnings, enums, booleans, rollback_contract structure, action_type, correlation_id)

**Pros**:
- Precise control over what is redacted
- Aligns with no-over-redaction principle
- Targets hardest target (`JsonMap`) with whole-field redaction
- Can be implemented incrementally: Phase 1 is `raw_arguments` and `metadata` only

**Cons**:
- Must be kept in sync if response schemas change
- Requires schema knowledge in MCP/output layer
- More complex than Option A

**Status**: **Phase 1 APPROVED AND IMPLEMENTED** in `ferrum-integrations-mcp::redact_sensitive_fields()`.

---

### Option C: Regex/Heuristic DLP Scanning

**Description**: Implement regex-based PII/credential detection (e.g., credit card patterns, AWS keys, JWTs) as a separate scanning pass.

**Pros**:
- Catches known credential formats regardless of field name
- Can be combined with Option B

**Cons**:
- Can be bypassed by novel credential formats
- May have false positives/negatives
- Performance overhead for regex scanning on large responses

**Status**: Complementary to Option B; not a standalone solution.

---

### Option D: Context-Aware / Trait Change

**Description**: Modify `SemanticFirewall` trait to pass `FirewallContext` to `sanitize_output` and `dlp_findings`, enabling context-aware redaction decisions.

**Signature change** (conceptual):
```rust
// Current
fn sanitize_output(&self, value: serde_json::Value) -> serde_json::Value;
fn dlp_findings(&self, value: &serde_json::Value) -> Vec<String>;

// Proposed
fn sanitize_output(&self, value: serde_json::Value, context: &FirewallContext) -> serde_json::Value;
fn dlp_findings(&self, value: &serde_json::Value, context: &FirewallContext) -> Vec<String>;
```

**Pros**:
- Enables context-aware decisions (e.g., different redaction rules for external vs. internal sources)
- Makes `FirewallContext` useful for output sanitization

**Cons**:
- Breaking trait change; requires updating all implementations
- `ferrum-integrations-mcp` would need to construct/pass `FirewallContext` at choke point
- More invasive; significant refactoring

**Status**: **Deferred** — requires significant trait change and breaking API change. Not recommended for initial DLP implementation.

---

### Option E: Advisory `dlp_findings` Only (No Transformation)

**Description**: Keep `dlp_findings` as advisory-only (current no-op state), emit findings as warnings or structured log entries, but perform no actual redaction.

**Pros**:
- No risk of over-redaction
- Provides visibility into potential sensitive data exposure

**Cons**:
- Does not actually redact sensitive data
- Security benefit is limited to logging/monitoring

**Status**: Not recommended as primary approach if redaction is the goal.

---

### Implementation Decision Summary

| Option | Redaction | Context-Aware | Breaking Change | Explorer Recommendation |
|--------|-----------|---------------|-----------------|------------------------|
| A | Pattern-based | No | No | Quick win candidate |
| B | Field-key-aware | No | No | **Start here** |
| C | Regex/heuristic | No | No | Complementary to B |
| D | Any of above | Yes | **Yes** | Deferred |
| E | None (advisory) | N/A | No | Not recommended |

**Explorer recommendation**: Option B targeting `ActionProposal.raw_arguments` and `metadata` (JsonMap) first. Option A as fallback for non-struct fields. Option C as supplementary scanning.

---

## 3. Risk Table

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Sensitive data leakage via MCP response (primary threat) | Medium | High | Option B field-key-aware redaction; oracle gate |
| Over-redaction breaking client parsing | Medium | Medium | No-over-redaction preservation list; unit tests |
| Under-redaction exposing sensitive JsonMap fields | Medium | High | Explicit JsonMap key enumeration; oracle review |
| `dlp_findings` false positives (advisory noise) | Low | Low | Threshold tuning; logging only |
| `FirewallContext` not passed to sanitizer (current gap) | Confirmed | High | Option D trait change or Option B struct-aware approach |
| Breaking trait change (Option D) | N/A | High | Deferred; avoid for D1.9 |
| DLP bypass via novel field names in JsonMap | Medium | High | JsonMap key enumeration; Option C heuristic scan |
| Performance regression at choke point | Low | Medium | Benchmarks; lazy evaluation |

---

## 4. Test Strategy

### 4.1 Unit Tests (Required)

| Test | Description | Expected Outcome |
|------|-------------|-------------------|
| `raw_arguments` redaction | ActionProposal with `raw_arguments` containing sensitive data | Field redacted; structure preserved |
| `metadata` JsonMap redaction | JsonMap with sensitive key-value pairs | Sensitive key-values redacted; safe keys preserved |
| `resource_bindings` redaction | CapabilityLease with `resource_bindings` | Field redacted |
| `target`/`compensation_plan.args` redaction | RollbackContract with sensitive target/args | Target and args redacted; structure preserved |
| `resource_scope`/`trust_context` redaction | IntentEnvelope with sensitive scope/context | Fields redacted |
| `result_digest` redaction | ExecutionRecord with `result_digest` | Field redacted |
| UUID preservation | Response containing UUID fields | UUIDs intact |
| Message/warnings preservation | Response with `reason`/`message`/`warnings` | Messages intact |
| Enum preservation | Response with `decision`/`state`/`status` | Enums intact |
| Boolean preservation | Response with boolean fields | Booleans intact |
| Rollback contract preservation | Response with `rollback_contract` | Structure preserved; sensitive inner fields redacted |
| Choke-point coverage | All 17 tools return redacted output where applicable | No tool bypasses redaction |
| Empty JSON / no crash | Empty `{}` response | No crash; passes through |
| Malformed JSON | Binary data in text field | Handled gracefully |

### 4.2 Integration Tests (Required Before Oracle Gate)

| Test | Description | Expected Outcome |
|------|-------------|-------------------|
| End-to-end tool call with sensitive data | MCP tool call → redacted JSON response | Sensitive fields redacted |
| Auth-gated tool call | With bearer token | Redaction applied |
| Blocked tool call | Unknown/mutating tool | Error returned (not redacted) |
| External vs. internal context differentiation | If Option D context-aware path chosen | Different redaction levels per context |

### 4.3 Negative Tests (Required)

| Test | Description | Expected Outcome |
|------|-------------|-------------------|
| Over-redaction detection | Known-safe fields (UUIDs, messages, enums) | Fields NOT redacted |
| Under-redaction detection | Known-sensitive fields | Fields redacted |
| JsonMap with non-string values | `metadata` with numbers/arrays | Handled without crash |
| Deeply nested JsonMap | `metadata` with nested objects | Recursive redaction applied |

---

## 5. Oracle Gate Checklist

Items requiring oracle approval before implementation:

- [ ] **Option B confirmed**: Struct-aware redaction targeting `raw_arguments` and `metadata` first
- [ ] **Option A supplementary**: Field-key-blind regex/heuristic for non-struct fields
- [ ] **Option C supplementary**: Regex DLP scanning for known credential formats
- [ ] **Option D deferred**: Context-aware trait change deferred to future phase
- [ ] **No-over-redaction rules confirmed**: Oracle approves preservation list (UUIDs, messages, warnings, enums, booleans, rollback_contract structure, action_type, correlation_id)
- [ ] **JsonMap key enumeration approved**: Oracle approves explicit enumeration of sensitive JsonMap keys
- [ ] **Test coverage approved**: Oracle approves test strategy above
- [ ] **D1.8 choke point extension**: D1.9 redact/DLP call at same choke point as D1.8 (or extending sanitizer)
- [ ] **No production/G2 claim**: Oracle confirms no production-ready or G2-complete claim for D1.9

Oracle verdict to be captured in writing upon approval.

---

## 6. Implementation Phases (Post-Oracle Approval)

### Phase D1.9.1: Struct-Aware Redaction (Option B) — PHASE 1 IMPLEMENTED

| # | Item | Status |
|---|------|--------|
| D1.9.1.1 | Enumerate sensitive field keys per struct | Future |
| D1.9.1.2 | Implement field-key-aware redaction for `ActionProposal.raw_arguments` | **IMPLEMENTED** |
| D1.9.1.3 | Implement field-key-aware redaction for `metadata` JsonMap | **IMPLEMENTED** |
| D1.9.1.4 | Extend to `CapabilityLease.resource_bindings` | Future (Phase 2) |
| D1.9.1.5 | Extend to `RollbackContract.target`/`compensation_plan.args` | Future (Phase 2) |
| D1.9.1.6 | Extend to `IntentEnvelope.resource_scope`/`trust_context` | Future (Phase 2) |
| D1.9.1.7 | Extend to `ExecutionRecord.metadata`/`result_digest` | Future (Phase 2) |
| D1.9.1.8 | Add unit tests for field redaction | **IMPLEMENTED** |

### Phase D1.9.2: Supplementary Pattern Matching (Option A + C)

| # | Item | Status |
|---|------|--------|
| D1.9.2.1 | Implement regex/heuristic DLP scanning (Option C) | Future |
| D1.9.2.2 | Implement field-key-blind replace for non-struct fields (Option A) | Future |
| D1.9.2.3 | Integrate with Option B at choke point | Future |

### Phase D1.9.3: Integration and Validation

| # | Item | Status |
|---|------|--------|
| D1.9.3.1 | End-to-end integration test | Future |
| D1.9.3.2 | Auth-gated integration test | Future |
| D1.9.3.3 | Performance/size edge-case tests | Future |
| D1.9.3.4 | Oracle review gate | Future |

---

## 7. Implementation Status (Post-Phase 1)

### Phase 1 Implemented (D1.9.1.2, D1.9.1.3, D1.9.1.8)
- **`redact_sensitive_fields()` function** in `ferrum-integrations-mcp::lib.rs`
- Calls after D1.8 `sanitize_output` at tools/call success boundary
- Redacts `raw_arguments` and `metadata` keys with `"[REDACTED]"`
- Recursive JSON walk: objects checked by key, arrays recurse, primitives pass through
- Whole-field redaction for `metadata` (no per-key enumeration in Phase 1)
- Unit tests covering: raw_arguments, metadata, nested, UUIDs, messages, enums, booleans, rollback_contract, arrays, primitives, deep nesting

### Phase 1 Does NOT Implement
- **Regex/heuristic DLP scanning** (Option A/C — deferred to Phase 2)
- **Additional sensitive keys** (resource_bindings, target, compensation_plan.args, resource_scope, result_digest — deferred to Phase 2)
- **`FirewallContext`-aware redaction** (Option D — deferred)
- **`dlp_findings` stub** remains no-op (not used in Phase 1)
- **Provenance emission** (gateway-owned)
- **Production/G2 claim** (security hardening only, not production-ready)

### Phase 1 Explicit Claims
- D1.9 Phase 1 implements Option B field-key-aware redaction for `raw_arguments` and `metadata`
- No-over-redaction principle observed: UUIDs, reason/message/warnings, enums, booleans, rollback_contract structure, action_type, correlation_id preserved
- D1.8 `sanitize_output` still applies before redaction
- Tests verify redaction and preservation behavior

---

## 8. References

| From | To | Purpose |
|------|-----|---------|
| This doc | [`85-mcp-server-d1-8-output-sanitization-preflight.md`](85-mcp-server-d1-8-output-sanitization-preflight.md) | D1.9 builds on D1.8; D1.8 choke point is approved |
| This doc | [`74-mcp-server-phase-d1-governance-design.md`](74-mcp-server-phase-d1-governance-design.md) | D1.9 is in governance pipeline |
| This doc | [`README.md`](README.md) | Reading order entry |
| This doc | Proto: `ferrum_proto::IntentCompileRequest`, `ferrum_proto::PipelineStatus` | Phase 1 targets ActionProposal.raw_arguments/metadata (JsonMap fields) |

---

*Document created: 2026-05-07. Updated: Phase 1 implemented per oracle verdict 2026-05-07. D1.9 Phase 1 is security hardening, not production-ready claim.*
