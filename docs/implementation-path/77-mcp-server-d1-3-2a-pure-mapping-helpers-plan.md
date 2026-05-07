# 77 — MCP Server D-1.3.2a Pure Mapping Helpers Plan

> **Status**: Planning documentation only. D-1.3.2a is NOT implemented — this document defines the scope and creates an implementation todo-list for when D1.3.2 is unblocked.
> **D1.3.2 Status**: Blocked by B-MAP-1 through B-MAP-7 (doc 76). This plan (D1.3.2a) is a sub-phase of D1.3.2.
> **Purpose**: Define pure (side-effect-free) mapping helper functions for D1.3.2, enabling type-safe transformations without any HTTP calls, state changes, or governance pipeline execution.
> **Constraint**: Do not claim D1.3.2a is implemented. Do not make any REST/HTTP calls. Do not implement policy evaluation, capability minting, rollback preparation, or provenance emission. Preserve no production/G2/operator claims.

---

## Explicit Non-Claims

- **No D1.3.2a implementation claim.** This is a plan only — D1.3.2 must be unblocked first.
- **No side-effect claim.** D1.3.2a produces only pure functions with no side effects.
- **No HTTP/REST calls.** No `POST`, `GET`, or any network calls are permitted in D1.3.2a.
- **No governance pipeline execution.** Policy evaluation, capability minting, authorization, preparation, execution, verification are all forbidden.
- **No D1.3.2b or D1.3.3+ claim.** These phases remain blocked by B-MAP-1..B-MAP-7.
- **No field stability claim.** All helper functions operate on UNVERIFIED placeholder types — actual field shapes may drift.

---

## 0. Overview

### 0.1 Why D1.3.2a Exists

D1.3.2 is blocked by B-MAP-1..B-MAP-7 (doc 76). However, a subset of the work — pure mapping helpers — can be planned now and implemented safely once unblocked. D1.3.2a defines this safe subset.

**D1.3.2a is purely mechanical:** It transforms data structures without making decisions, calling APIs, or changing state.

### 0.2 What D1.3.2a Is NOT

D1.3.2a does NOT include:
- Any HTTP or REST calls
- Policy evaluation logic
- Capability minting
- Authorization checks
- Rollback preparation
- Provenance emission
- Approval/reject tool execution
- Any mutating behavior

### 0.3 Relationship to Other Phases

```
D1.3.1 (types-only) ✅ Done
       │
       ▼
D1.3.2a (pure mapping helpers) ← THIS PLAN — blocked by D1.3.2 unblock
       │
       ▼
D1.3.2b (API call wiring) ← BLOCKED by B-MAP-1..B-MAP-7
       │
       ▼
D1.3.3+ ← BLOCKED by D1.3.2b and B-MAP-*
```

---

## 1. Allowed and Forbidden Boundaries

### 1.1 D1.3.2a Allowed Scope

**Permitted in D1.3.2a:**

- Define pure helper functions that transform data structures
- Implement `From` trait conversions between types
- Implement parsing/serialization helpers (serde only, no I/O)
- Add unit tests that use mock/in-memory data
- Add `TODO` markers for fields that need explorer confirmation
- Import types from `stage2_types.rs` (but not call any HTTP-capable code)
- Use `anyhow`/`eyre` for error handling in pure functions
- Implement enum variant mapping functions

**What D1.3.2a CANNOT produce:**
- Any observable side effect
- Any network call
- Any change to persistent state
- Any behavior observable outside the process

### 1.2 D1.3.2a Forbidden (Hard Boundaries)

| Forbidden | Reason |
|-----------|--------|
| Calling `POST /v1/intents/compile` | Side effect — creates intent on gateway |
| Calling `POST /v1/proposals/{id}/evaluate` | Side effect — evaluates policy |
| Calling `POST /v1/capabilities/mint` | Side effect — mints capability |
| Calling `POST /v1/executions/authorize` | Side effect — authorizes execution |
| Calling `POST /v1/executions/{id}/prepare` | Side effect — prepares rollback |
| Calling `POST /v1/executions/{id}/execute` | Side effect — executes action |
| Calling `POST /v1/executions/{id}/verify` | Side effect — verifies execution |
| Calling `POST /v1/executions/{id}/compensate` | Side effect — triggers compensation |
| Emitting provenance events | Side effect — writes to provenance ledger |
| Any `#[instrument]` that assumes pipeline state | Observational difference |
| Any mutable static state | Side effect via interior mutability |

### 1.3 What Counts as "Pure"

A function is pure for D1.3.2a purposes if:
1. It always returns the same output for the same input (deterministic)
2. It performs no I/O (no network, no file system, no environment reads beyond const items)
3. It does not mutate any state observable outside itself
4. It is safe to call an arbitrary number of times without consequence

**Note**: Reading from `FERRUMD_MCP_AGENT_ID` env var during `ActorIdentity::resolve()` is NOT part of D1.3.2a — that is D1.1/D1.2 infrastructure. D1.3.2a helpers receive values as parameters.

---

## 2. Proposed Helper Functions

### 2.1 Helper: ToolCallAction → DraftIntentCompileRequestParts

**Purpose**: Convert an internal `ToolCallAction` (6 fields) into the 12-field gateway `IntentCompileRequest` structure, with explicit TODO markers for unresolved fields.

**Signature:**
```rust
/// Converts a ToolCallAction into a DraftIntentCompileRequestParts.
/// Returns a struct with all 12 IntentCompileRequest fields filled,
/// with TODO markers on fields that need B-MAP-1 resolution.
///
/// This function is PURE — no HTTP calls, no state changes.
///
/// # Arguments
///
/// * `action` - The ToolCallAction to convert
/// * `principal_id` - Resolved principal ID (from ActorIdentity)
/// * `session_id` - Auto-generated or resolved session ID
///
/// # Returns
///
/// A `DraftIntentCompileRequestParts` with all 12 fields populated.
/// Fields without a clear derivation are marked with `TODO(...)`.
pub fn tool_call_action_to_draft_intent_compile_request(
    action: &ToolCallAction,
    principal_id: String,
    session_id: String,
) -> DraftIntentCompileRequestParts {
    // ... implementation
}

/// Draft parts with explicit TODO markers for unresolved fields.
/// Serialization will produce JSON with TODO strings for missing fields,
/// making it obvious which fields need gateway confirmation.
#[derive(Debug, Clone)]
pub struct DraftIntentCompileRequestParts {
    pub principal_id: Result<String, MappingError>,              // TODO if actor_id mapping unknown
    pub session_id: Option<String>,                            // C3: Option — auto-generated UUID or None if unresolved
    pub channel_id: Option<String>,                            // C3: Option — MCP channel concept undefined
    pub title: String,                                         // Derived: action_type: target
    pub goal: String,                                           // TODO — MCP has no goal concept
    pub agent_plan_summary: Option<String>,                    // C3: Option — MCP has no plan concept; None if unresolved
    pub trusted_context: Option<serde_json::Value>,            // C8: JsonMap stub; None if not applicable
    pub raw_inputs: Vec<serde_json::Value>,                    // C7: IntentInputRef TODO — convert parameters JSON to array
    pub requested_resource_scope: Vec<ResourceSelector>,       // C1: Vec<ResourceSelector>, not single
    pub requested_risk_tier: Option<RiskTier>,                  // C3: Option — inferred from action_type (B-MAP-3)
    pub approval_mode: Option<ApprovalMode>,                    // C3: Option — derived from RiskTier (B-MAP-3)
    pub metadata: serde_json::Value,                            // Empty object {}
}
```

**TODO Policy**: Use `Result<T, MappingError>` where `MappingError` contains a `TODO` variant with the field name and the B-MAP issue reference. This makes TODO markers visible in the type system.

### 2.2 Helper: ToolCallAction → DraftActionProposalParts

**Purpose**: Convert an internal `ToolCallAction` into the 13-field `ActionProposal` structure, with explicit TODO markers for fields that need gateway confirmation.

**Signature:**
```rust
/// Converts a ToolCallAction into DraftActionProposalParts.
/// Returns a struct with ActionProposal fields populated where possible,
/// with TODO markers on fields that need B-MAP-4/B-MAP-5/B-MAP-6 resolution.
///
/// This function is PURE — no HTTP calls, no state changes.
pub fn tool_call_action_to_draft_action_proposal(
    action: &ToolCallAction,
    title: String,
) -> DraftActionProposalParts {
    // ... implementation
}

#[derive(Debug, Clone)]
pub struct DraftActionProposalParts {
    pub intent_id: String,                           // From ToolCallAction.intent_id
    pub step_index: u32,                             // Default 0 — TODO if sequential semantics unclear
    pub title: String,                               // From DraftIntentCompileRequestParts.title
    pub tool_name: String,                            // From ToolCallAction.action_type
    pub server_name: Result<String, MappingError>,   // TODO — B-MAP-4 resolution undefined
    pub raw_arguments: serde_json::Value,            // From ToolCallAction.parameters
    pub expected_effect: Result<String, MappingError>, // TODO — B-MAP-5 inference undefined
    pub estimated_risk: RiskTier,                     // From RiskTier inference (B-MAP-3)
    pub requested_rollback_class: RollbackClass,      // Inferred from action_type (B-MAP-6)
    pub taint_inputs: Vec<String>,                    // Default empty — TODO if taint concept unclear
    pub metadata: serde_json::Value,                   // From IntentCompileRequest.metadata
}
```

### 2.3 Helper: RiskTier Inference

**Purpose**: Infer `RiskTier` from `action_type` string using a deterministic mapping table.

> **C4: Remove is_critical_path guard; prefer simple mapping.** Path-based risk refinement requires target parameter and is complex. Default fs_write to High with TODO for future path-based refinement.

**Signature:**
```rust
/// Infers RiskTier from an action_type string.
///
/// Returns RiskTier::Low for read-only actions, RiskTier::Medium,
/// RiskTier::High for writes, RiskTier::Critical for irreversible/destructive actions.
///
/// This function is PURE — deterministic mapping with no side effects.
pub fn infer_risk_tier(action_type: &str) -> RiskTier {
    match action_type {
        // Read-only — Low risk
        "fs_read" | "git_fetch" | "http_get" | "db_query" => RiskTier::Low,

        // Non-critical writes — Medium risk (C4: simplified, no path guard)
        "git_push" => RiskTier::Medium,

        // Critical writes — High risk
        // C4: fs_write defaults to High; path-based refinement deferred to future work
        "fs_write" | "git_push" | "http_post" => RiskTier::High,

        // Destructive/irreversible — Critical risk
        "sql_mutate" | "db_mutate" | "fs_delete" | "git_force_push" => RiskTier::Critical,

        // Unknown — Medium risk by default (fail-open for unmapped actions)
        _ => RiskTier::Medium,
    }
}
```

**TODO for future refinement**: Path-based risk refinement (e.g., `/etc` paths → Critical) requires passing `target` as a parameter and is deferred to a future enhancement. Current implementation treats all `fs_write` as High risk.

### 2.4 Helper: RollbackClass Inference

**Purpose**: Infer `RollbackClass` from `action_type` and optional `target` using a deterministic mapping.

> **C6: http_post -> R3IrreversibleHighConsequence.** HTTP POST is treated as high-consequence because compensating requests (DELETE/revert) may not be reliably available.

**Signature:**
```rust
/// Infers RollbackClass from an action_type and optional target.
///
/// Returns:
/// - R0NativeReversible for read-only actions
/// - R1SnapshotRecoverable for file operations
/// - R2Compensatable for git push
/// - R3IrreversibleHighConsequence for http_post (C6), force push, sql mutate, destructive operations
///
/// This function is PURE — deterministic mapping with no side effects.
pub fn infer_rollback_class(action_type: &str, target: Option<&str>) -> RollbackClass {
    match action_type {
        // Read-only — no rollback needed
        "fs_read" | "git_fetch" | "http_get" | "db_query" => RollbackClass::R0NativeReversible,

        // File operations — snapshot recoverable
        "fs_write" => RollbackClass::R1SnapshotRecoverable,
        "fs_delete" => RollbackClass::R1SnapshotRecoverable,

        // Compensatable via reverse operation
        "git_push" => RollbackClass::R2Compensatable,

        // Irreversible or high consequence (C6: http_post -> R3)
        "http_post" => RollbackClass::R3IrreversibleHighConsequence,
        "git_force_push" => RollbackClass::R3IrreversibleHighConsequence,
        "sql_mutate" => RollbackClass::R3IrreversibleHighConsequence,
        "db_mutate" => RollbackClass::R3IrreversibleHighConsequence,

        _ => RollbackClass::R3IrreversibleHighConsequence, // Fail conservative
    }
}
```

### 2.5 Helper: ResourceSelector Parser Stub

**Purpose**: Parse the MCP `scope` string (e.g., `"fs:write:/tmp/test.txt"`) into a `ResourceSelector` tagged enum.

> **C2: Real ResourceSelector variant field names** (per ferrum-proto):
> - `FilesystemPath { path, mode, content_hash }`
> - `GitRepository { repo_path, allowed_refs, mode }`
> - `SqliteDatabase { db_path, tables, mode }`
> - `HttpEndpoint { method, base_url, path_prefix, mode }`
> - `EmailDraft { recipient_allowlist, subject_prefix_allowlist, mode }`
> - `McpTool { server_name, tool_name, mode }`

**Signature:**
```rust
/// Parses an MCP scope string into a Vec<ResourceSelector>.
///
/// Scope format: `resource_type:access_mode:resource_path`
/// Returns Vec<ResourceSelector> to match the gateway API (C1).
///
/// Examples:
///   - `"fs:read:/tmp/test.txt"` → vec![FilesystemPath { path: "/tmp/test.txt", mode: "read", content_hash: None }]
///   - `"git:push:origin"` → vec![GitRepository { repo_path: "origin", allowed_refs: vec![], mode: "push" }]
///   - `"sql:mutate:mydb.db"` → vec![SqliteDatabase { db_path: "mydb.db", tables: None, mode: "mutate" }]
///
/// This function is PURE — no side effects.
///
/// Returns `MappingError::UnsupportedScope` if the scope format is not recognized.
pub fn parse_resource_scope(scope: &str) -> Result<Vec<ResourceSelector>, MappingError> {
    let parts: Vec<&str> = scope.split(':').collect();
    if parts.len() < 2 {
        return Err(MappingError::UnsupportedScope(scope.to_string()));
    }

    match (parts[0], parts[1]) {
        ("fs", "read") | ("fs", "write") | ("fs", "delete") => {
            let path = parts[2..].join(":");
            Ok(vec![ResourceSelector::FilesystemPath {
                path,
                mode: parts[1].to_string(),
                content_hash: None,  // C2: content_hash stub — TODO if needed
            }])
        }
        ("git", "push") | ("git", "fetch") | ("git", "force_push") => {
            let repo_path = parts.get(2).unwrap_or(&"origin");
            Ok(vec![ResourceSelector::GitRepository {
                repo_path: repo_path.to_string(),
                allowed_refs: vec![],  // C2: allowed_refs stub — TODO if needed
                mode: parts[1].to_string(),
            }])
        }
        ("sql", "mutate") | ("sql", "query") => {
            let db_path = parts.get(2).unwrap_or(&":memory:");
            Ok(vec![ResourceSelector::SqliteDatabase {
                db_path: db_path.to_string(),
                tables: None,  // C2: tables stub — TODO if needed
                mode: parts[1].to_string(),
            }])
        }
        ("http", "post") | ("http", "get") | ("http", "delete") => {
            let url = parts[2..].join(":");
            Ok(vec![ResourceSelector::HttpEndpoint {
                method: parts[1].to_string().to_uppercase(),
                base_url: url.clone(),
                path_prefix: "/".to_string(),  // C2: path_prefix stub — TODO if needed
                mode: parts[1].to_string(),
            }])
        }
        ("email", "send") => {
            let recipient = parts.get(2).unwrap_or(&"").to_string();
            Ok(vec![ResourceSelector::EmailDraft {
                recipient_allowlist: vec![recipient],  // C2: recipient_allowlist (Vec)
                subject_prefix_allowlist: vec![],      // C2: subject_prefix_allowlist stub — TODO if needed
                mode: "send".to_string(),
            }])
        }
        ("mcp", "tool") => {
            let tool_name = parts.get(2).unwrap_or(&"").to_string();
            Ok(vec![ResourceSelector::McpTool {
                server_name: "default".to_string(),  // C2: server_name stub — TODO if needed
                tool_name,
                mode: "call".to_string(),
            }])
        }
        _ => Err(MappingError::UnsupportedScope(scope.to_string())),
    }
}

#[derive(Debug, Clone)]
pub enum MappingError {
    /// Field requires B-MAP resolution — contains TODO reference
    Todo { field: String, bmap_issue: String, note: String },
    /// Scope format not recognized
    UnsupportedScope(String),
    /// Risk tier cannot be determined
    UnknownRiskTier(String),
    /// Rollback class cannot be determined
    UnknownRollbackClass(String),
    /// Resource selector parse error
    ResourceSelectorParseError(String),
}
```

### 2.6 Helper: server_name Resolution Stub

**Purpose**: Resolve `server_name` from `action_type` pattern matching.

**Signature:**
```rust
/// Resolves server_name from action_type pattern.
///
/// Returns the adapter/server name that should handle this action type.
///
/// This function is PURE — deterministic pattern matching with no side effects.
pub fn resolve_server_name(action_type: &str) -> Result<String, MappingError> {
    let server_name = match action_type {
        f if f.starts_with("fs_") => "filesystem",
        g if g.starts_with("git_") => "git",
        h if h.starts_with("http_") => "http",
        s if s.starts_with("sql_") || s.starts_with("db_") => "database",
        e if e.starts_with("email_") => "email",
        m if m.starts_with("mcp_") => "mcp",
        _ => return Err(MappingError::Todo {
            field: "server_name".to_string(),
            bmap_issue: "B-MAP-4".to_string(),
            note: format!("Unknown action_type prefix for: {}", action_type),
        }),
    };
    Ok(server_name.to_string())
}
```

### 2.7 Helper: ApprovalMode Derivation from RiskTier

**Purpose**: Derive `ApprovalMode` from `RiskTier` using a deterministic policy table.

**Signature:**
```rust
/// Derives ApprovalMode from RiskTier.
///
/// Policy:
/// - Low    → None (no approval needed)
/// - Medium → DraftOnly (only draft intents need approval)
/// - High   → Required (manual approval required)
/// - Critical → TwoPhaseCommit (two-phase approval process)
///
/// This function is PURE — deterministic policy lookup with no side effects.
pub fn derive_approval_mode(risk_tier: RiskTier) -> ApprovalMode {
    match risk_tier {
        RiskTier::Low => ApprovalMode::None,
        RiskTier::Medium => ApprovalMode::DraftOnly,
        RiskTier::High => ApprovalMode::Required,
        RiskTier::Critical => ApprovalMode::TwoPhaseCommit,
    }
}
```

---

## 3. TODO Marker Policy

### 3.1 Why TODO Markers

D1.3.2a helpers operate on UNVERIFIED placeholder types from D1.3.1. Many fields cannot be correctly populated until B-MAP blockers are resolved.

### 3.2 TODO Marker Format

Use the `MappingError::Todo` variant to carry TODO information:

```rust
Err(MappingError::Todo {
    field: "goal".to_string(),
    bmap_issue: "B-MAP-1".to_string(),
    note: "MCP has no goal concept — gateway requires plain text description".to_string(),
})
```

### 3.3 TODO Classification

| TODO Category | B-MAP Issue | Behavior |
|---------------|-------------|----------|
| `principal_id` mapping | B-MAP-1 | Return `Err(MappingError::Todo{...})` with B-MAP-1 reference |
| `goal` derivation | B-MAP-1 | Return `Err(MappingError::Todo{...})` with B-MAP-1 reference |
| `server_name` resolution | B-MAP-4 | Return `Err(MappingError::Todo{...})` with B-MAP-4 reference |
| `expected_effect` inference | B-MAP-5 | Return `Err(MappingError::Todo{...})` with B-MAP-5 reference |
| `requested_rollback_class` inference | B-MAP-6 | Return `Err(MappingError::Todo{...})` — but use conservative default |

**Note**: `requested_rollback_class` uses a conservative default (R3IrreversibleHighConsequence) even when inference is uncertain, to avoid approving rollback-unprepared actions.

### 3.4 TODO Resolution Path

When the gateway team resolves a B-MAP issue:
1. Update the helper function with the correct derivation logic
2. Remove the `Err(MappingError::Todo{...})` path
3. Add a unit test with the confirmed behavior
4. Update doc 76 B-MAP status from "Open" to "Resolved"

---

## 4. Input/Output Summary

### 4.1 Input Types (from D1.3.1)

| Type | Source | Location |
|------|--------|----------|
| `ToolCallAction` | MCP-internal | `stage2_types.rs` |
| `RiskTier` | Placeholder enum | `stage2_types.rs` (to be added) |
| `ApprovalMode` | Placeholder enum | `stage2_types.rs` (to be added) |
| `RollbackClass` | Placeholder enum | `stage2_types.rs` (to be added) |
| `ResourceSelector` | Placeholder tagged enum | `stage2_types.rs` (to be added) |
| `MappingError` | New error type | This plan (new module) |

### 4.2 Output Types (New in D1.3.2a)

| Type | Description |
|------|-------------|
| `DraftIntentCompileRequestParts` | All 12 IntentCompileRequest fields with TODO markers |
| `DraftActionProposalParts` | All 13 ActionProposal fields with TODO markers |
| `MappingError` | Error type with `Todo` variant for B-MAP references |

### 4.3 New Module Structure

> **C5: Recommend importing real ferrum-proto types rather than parallel placeholders.** Instead of defining `RiskTier`, `ApprovalMode`, `RollbackClass`, `ResourceSelector` as new placeholder types in `stage2_types.rs`, prefer importing the canonical types from `ferrum-proto` (or equivalent gateway crate). This avoids drift between MCP server types and gateway types. If ferrum-proto types are unavailable, document the exact field names and enum variants being used.

```
crates/ferrum-integrations-mcp/src/
├── stage2_types.rs        # D1.3.1 types (placeholder — to be replaced by ferrum-proto imports)
├── mapping_helpers.rs      # NEW: D1.3.2a pure helpers
│   ├── DraftIntentCompileRequestParts
│   ├── DraftActionProposalParts
│   ├── MappingError (with MappingError::Todo variant)
│   ├── tool_call_action_to_draft_intent_compile_request()
│   ├── tool_call_action_to_draft_action_proposal()
│   ├── infer_risk_tier()
│   ├── infer_rollback_class()
│   ├── parse_resource_scope()
│   ├── resolve_server_name()
│   └── derive_approval_mode()
└── lib.rs
```

**Dependency decision (C5):** When implementing D1.3.2a, evaluate:
1. Can `ferrum-proto` types be imported directly? If yes, use them.
2. If `ferrum-proto` is unavailable, define types with exact field names matching doc 76 E2-E5.
3. Do NOT create parallel placeholder types that may drift from gateway schema.

---

## 5. Test Plan

### 5.1 Unit Tests Only

D1.3.2a tests use only mock/in-memory data — no HTTP calls, no real gateway.

### 5.2 Test Cases

| Test | Description | Expected Result |
|------|-------------|-----------------|
| `test_tool_call_action_to_draft_intent_compile_request_fs_write` | Convert `fs_write` action | 12 fields populated, `principal_id` returns TODO error |
| `test_tool_call_action_to_draft_action_proposal` | Convert `fs_write` action | 13 fields populated, `server_name` returns Ok("filesystem") |
| `test_infer_risk_tier_low` | `fs_read` | `RiskTier::Low` |
| `test_infer_risk_tier_medium` | `http_post` | `RiskTier::Medium` |
| `test_infer_risk_tier_high` | `sql_mutate` | `RiskTier::High` |
| `test_infer_risk_tier_critical` | `git_force_push` | `RiskTier::Critical` |
| `test_infer_rollback_class_read_only` | `fs_read` | `RollbackClass::R0NativeReversible` |
| `test_infer_rollback_class_file_write` | `fs_write` | `RollbackClass::R1SnapshotRecoverable` |
| `test_infer_rollback_class_git_push` | `git_push` | `RollbackClass::R2Compensatable` |
| `test_infer_rollback_class_sql_mutate` | `sql_mutate` | `RollbackClass::R3IrreversibleHighConsequence` |
| `test_parse_resource_scope_fs` | `"fs:write:/tmp/test.txt"` | `FilesystemPath { path: "/tmp/test.txt", access_mode: "write" }` |
| `test_parse_resource_scope_git` | `"git:push:origin"` | `GitRepository { repo: "origin", ... }` |
| `test_parse_resource_scope_unsupported` | `"unknown:format"` | `Err(MappingError::UnsupportedScope(...))` |
| `test_resolve_server_name_fs` | `"fs_write"` | `Ok("filesystem")` |
| `test_resolve_server_name_unknown` | `"unknown_tool"` | `Err(MappingError::Todo{field: "server_name", ...})` |
| `test_derive_approval_mode_low` | `RiskTier::Low` | `ApprovalMode::None` |
| `test_derive_approval_mode_critical` | `RiskTier::Critical` | `ApprovalMode::TwoPhaseCommit` |
| `test_all_action_types_have_risk_tier` | Enumerate all known action types | No panic, always returns RiskTier |
| `test_all_action_types_have_rollback_class` | Enumerate all known action types | No panic, always returns RollbackClass |

### 5.3 Test Infrastructure

- Use `#[cfg(test)]` module in `mapping_helpers.rs`
- Use `serde_json::json!` for mock data
- No external dependencies beyond what's already in `Cargo.toml`

---

## 6. Risk Controls

### 6.1 How D1.3.2a Prevents Side Effects

| Risk | Control |
|------|---------|
| Accidental HTTP call | No `reqwest`, `ureq`, or HTTP client in `mapping_helpers.rs` |
| Accidental state mutation | No `static mut`, no interior mutability primitives |
| Mistakenly shipping "done" code | TODO markers on all unresolved fields — clearly visible |
| B-MAP blockers bypassed | Helper functions return `Err` for unresolved fields, cannot silently proceed |
| Forgetting B-MAP issues | `MappingError::Todo` carries explicit B-MAP reference |

### 6.2 Compiler-Enforced Safety

- `mapping_helpers.rs` can be compiled as a separate crate or module
- No `pub fn` in `mapping_helpers.rs` may call any function that performs I/O
- Clippy lint `disallowed-methods` can be added to block `reqwest::`, `ureq::` imports

### 6.3 Audit Plan

Before D1.3.2b proceeds, audit `mapping_helpers.rs` for:
1. Any import of `reqwest`, `ureq`, `hyper`, `actix_web`, or other HTTP clients
2. Any use of `std::net`, `tokio::net`, or other networking primitives
3. Any `unsafe` blocks
4. Any `thread_local` or `static mut` state

---

## 7. Non-Goals (Explicit)

The following are **NOT** in scope for D1.3.2a:

| Item | Reason |
|------|--------|
| **Any HTTP/REST calls** | Forbidden — D1.3.2b scope |
| **Policy evaluation logic** | Blocked by B-MAP-3 |
| **Capability minting** | Blocked by B-MAP-1..B-MAP-3 |
| **Authorization logic** | Blocked by B-MAP-1..B-MAP-3 |
| **Rollback preparation** | Blocked by B-MAP-6 |
| **Provenance emission** | Blocked by B-MAP-7 |
| **Approval/reject tool implementation** | Blocked by B1 in doc 75 |
| **Integration tests with real gateway** | Blocked — D1.3.2a has no network calls |
| **Production G2 criteria** | Operator-owned — out of MCP scope |

---

## 8. Ordered Implementation Todo-List

### D1.3.2a Tasks

| # | Task | Status | Blocker | Notes |
|---|------|--------|---------|-------|
| D1.3.2a.1 | Define `MappingError` enum with `MappingError::Todo` variant | Future | D1.3.2 unblocked | New module `mapping_helpers.rs` |
| D1.3.2a.2 | Define `DraftIntentCompileRequestParts` struct | Future | D1.3.2 unblocked | 12 fields, Result/TODO for unresolved |
| D1.3.2a.3 | Define `DraftActionProposalParts` struct | Future | D1.3.2 unblocked | 13 fields, Result/TODO for unresolved |
| D1.3.2a.4 | Implement `infer_risk_tier()` pure helper | Future | D1.3.2 unblocked | Deterministic mapping table |
| D1.3.2a.5 | Implement `infer_rollback_class()` pure helper | Future | D1.3.2 unblocked | Deterministic mapping table |
| D1.3.2a.6 | Implement `derive_approval_mode()` pure helper | Future | D1.3.2 unblocked | RiskTier → ApprovalMode policy |
| D1.3.2a.7 | Implement `parse_resource_scope()` pure helper | Future | D1.3.2 unblocked | Returns UnsupportedScope for unknown formats |
| D1.3.2a.8 | Implement `resolve_server_name()` pure helper | Future | D1.3.2 unblocked | Returns Todo error for unknown prefixes |
| D1.3.2a.9 | Implement `tool_call_action_to_draft_intent_compile_request()` | Future | D1.3.2a.1..D1.3.2a.8 | Composes all field mappings |
| D1.3.2a.10 | Implement `tool_call_action_to_draft_action_proposal()` | Future | D1.3.2a.1..D1.3.2a.8 | Composes all field mappings |
| D1.3.2a.11 | Add unit tests for all helpers | Future | D1.3.2a.9..D1.3.2a.10 | Mock data only, no HTTP |
| D1.3.2a.12 | Add clippy lint to forbid HTTP client imports | Future | D1.3.2a.11 | Prevent accidental HTTP in helpers |

### Post-D1.3.2a (BLOCKED by B-MAP-1..B-MAP-7)

| Phase | Tasks | Blocker |
|-------|-------|---------|
| D1.3.2b | Wire helpers to actual REST API calls | B-MAP-1..B-MAP-7 resolved |
| D1.3.3 | Implement submit_intent tool | D1.3.2b complete |
| D1.3.4 | Implement evaluate_intent tool | D1.3.3 complete |
| D1.3.5 | Implement prepare_execution tool | D1.3.4 complete |
| D1.3.7 | Implement execute_prepared/compensate tools | D1.3.5 complete |

---

## 9. Commit and Verify Plan

### 9.1 Commit Strategy

D1.3.2a changes should be committed as a single logical unit with the message:
```
feat(mcp): add D1.3.2a pure mapping helpers (doc 77)

- Add mapping_helpers.rs module with pure transformation functions
- Define DraftIntentCompileRequestParts and DraftActionProposalParts
- Define MappingError with Todo variant for B-MAP references
- Add unit tests for all helpers (mock data only)
- No HTTP calls, no state changes — pure functions only
```

### 9.2 Verification Steps

1. **Compilation check**: `cargo check -p ferrum-integrations-mcp`
2. **Test check**: `cargo test -p ferrum-integrations-mcp --lib`
3. **No HTTP imports**: `rg 'reqwest|ureq|hyper' crates/ferrum-integrations-mcp/src/mapping_helpers.rs` → should return no matches
4. **TODO markers present**: `rg 'MappingError::Todo' crates/ferrum-integrations-mcp/src/mapping_helpers.rs` → should return matches
5. **Clippy clean**: `cargo clippy -p ferrum-integrations-mcp --lib -- -D warnings`

---

## 10. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`76-mcp-server-d1-action-proposal-mapping-design.md`](76-mcp-server-d1-action-proposal-mapping-design.md) | D1.3.2 blocked by B-MAP-1..B-MAP-7 |
| This doc | [`75-mcp-server-phase-d1-stage2-governance-pipeline-plan.md`](75-mcp-server-phase-d1-stage2-governance-pipeline-plan.md) | D1.3.2 implementation plan context |
| This doc | [`74-mcp-server-phase-d1-governance-design.md`](74-mcp-server-phase-d1-governance-design.md) | ToolCallAction definition |
| This doc | [`stage2_types.rs`](../../crates/ferrum-integrations-mcp/src/stage2_types.rs) | D1.3.1 placeholder types |
| This doc | [`README.md`](README.md) | Reading order entry |

---

## 11. Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| D1.3.2a as pure helpers only | 2026-05-07 | Enables type-safe transformation work before B-MAP blockers resolved |
| TODO markers via MappingError::Todo | 2026-05-07 | Makes B-MAP issues visible in type system, not just comments |
| Conservative defaults for rollback | 2026-05-07 | Use R3IrreversibleHighConsequence when uncertain — fail safe |
| No HTTP imports in mapping_helpers.rs | 2026-05-07 | Compiler-enforced boundary against accidental side effects |
| D1.3.2b+ remain gated | 2026-05-07 | Cannot wire helpers to HTTP until B-MAP-1..B-MAP-7 resolved |
| C1: requested_resource_scope is Vec<ResourceSelector> | 2026-05-07 | Gateway API accepts array of resource selectors |
| C2: Real ResourceSelector variant field names | 2026-05-07 | FilesystemPath{path,mode,content_hash}, GitRepository{repo_path,allowed_refs,mode}, etc. |
| C3: Optional fields for session_id, channel_id, agent_plan_summary, requested_risk_tier, approval_mode | 2026-05-07 | These fields may not have defaults — use Option<T> |
| C4: Simplify infer_risk_tier — remove is_critical_path guard | 2026-05-07 | Path-based refinement deferred; fs_write defaults to High |
| C5: Prefer importing ferrum-proto types over parallel placeholders | 2026-05-07 | Avoid drift between MCP server and gateway types |
| C6: http_post -> R3IrreversibleHighConsequence | 2026-05-07 | HTTP POST is high-consequence; compensating request may not be reliable |

---

*Document created: 2026-05-07. Planning documentation only. D-1.3.2a implementation is blocked by D1.3.2 unblock. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
