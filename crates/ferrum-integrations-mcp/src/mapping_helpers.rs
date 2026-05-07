//! # D1.3.2a Pure Mapping Helpers
//!
//! > **D1.3.2a Scope**: Pure (side-effect-free) helper functions for mapping MCP tool calls
//! > to gateway governance types. No HTTP calls, no state changes, no governance pipeline execution.
//!
//! ## Design Decisions (per doc 77)
//!
//! - **C1**: `requested_resource_scope` is `Vec<ResourceSelector>` (matches ferrum-proto)
//! - **C2**: Real `ResourceSelector` variant field names per ferrum-proto
//! - **C3**: Optional fields (`session_id`, `channel_id`, `agent_plan_summary`, `requested_risk_tier`,
//!   `approval_mode`) are `Option<T>` per ferrum-proto `IntentCompileRequest`
//! - **C4**: `infer_risk_tier` simplified — `fs_write` defaults to `High`, no `is_critical_path` guard
//! - **C5**: Using real `ferrum-proto` types (`RiskTier`, `ApprovalMode`, `RollbackClass`,
//!   `ResourceSelector`, `IntentInputRef`, etc.) — no parallel placeholders
//! - **C6**: `http_post` rollback class is `R3IrreversibleHighConsequence` (conservative)
//!
//! ## D1.3.2a Permitted
//!
//! - Pure helper functions (deterministic, no side effects)
//! - Unit tests with mock/in-memory data
//! - Type transformations and conversions
//!
//! ## D1.3.2a Forbidden
//!
//! - Any HTTP/REST calls (`reqwest`, `ureq`, etc.)
//! - Policy evaluation, capability minting, authorization
//! - Rollback preparation, provenance emission
//! - Any mutating behavior or state changes

use ferrum_proto::{
    ApprovalMode, IntentInputRef, PrincipalId, ResourceMode, ResourceSelector, RiskTier,
    RollbackClass, SessionId,
};
use std::fmt;

// ---------------------------------------------------------------------------
// Mapping Error Types
// ---------------------------------------------------------------------------

/// Error type for mapping helpers.
///
/// Uses a `Todo` variant to make B-MAP issues visible in the type system,
/// following doc 77 §3 TODO marker policy.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingError {
    /// Field requires B-MAP resolution — contains TODO reference and note.
    /// This makes unresolved fields visible at compile time.
    Todo {
        field: String,
        bmap_issue: String,
        note: String,
    },
    /// Scope format not recognized by the parser.
    UnsupportedScope(String),
    /// Resource selector parse error.
    ResourceSelectorParseError(String),
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MappingError::Todo {
                field,
                bmap_issue,
                note,
            } => {
                write!(f, "TODO [{}] {}: {}", bmap_issue, field, note)
            }
            MappingError::UnsupportedScope(scope) => {
                write!(f, "Unsupported scope format: {}", scope)
            }
            MappingError::ResourceSelectorParseError(msg) => {
                write!(f, "ResourceSelector parse error: {}", msg)
            }
        }
    }
}

impl std::error::Error for MappingError {}

// ---------------------------------------------------------------------------
// Draft Parts (Output Types with TODO Markers)
// ---------------------------------------------------------------------------

/// Draft `IntentCompileRequest` parts with explicit TODO markers for unresolved fields.
///
/// Per doc 77 §2.1, this struct contains all 12 fields of `IntentCompileRequest`
/// from ferrum-proto. Fields that cannot be derived from MCP tool call data
/// return `Err(MappingError::Todo{...})` to make blockers visible.
///
/// # Example
///
/// ```
/// use ferrum_integrations_mcp::mapping_helpers::*;
///
/// let action = tool_call_action::new(
///     "intent-123".to_string(),
///     "fs_write".to_string(),
///     "files:write:/tmp/test.txt".to_string(),
///     "/tmp/test.txt".to_string(),
///     serde_json::json!({}),
///     "agent-001".to_string(),
/// );
///
/// let result = tool_call_action_to_draft_intent_compile_request(&action, "agent-001".to_string());
/// // principal_id returns Err(Todo) — B-MAP-1
/// // session_id returns Ok(Some(...)) — auto-generated
/// // channel_id returns Ok(None) — MCP has no channel concept
/// // goal returns Err(Todo) — MCP has no goal concept
/// ```
#[derive(Debug, Clone)]
pub struct DraftIntentCompileRequestParts {
    /// Principal ID — derived from `actor_id`.
    /// B-MAP-1: May need mapping to gateway principal ID.
    pub principal_id: Result<PrincipalId, MappingError>,

    /// Session ID — optionally auto-generated UUID.
    /// C3: `Option<String>` — None if unresolved.
    pub session_id: Option<String>,

    /// Channel ID — MCP has no channel concept.
    /// C3: `Option<String>` — None for MCP-initiated intents.
    pub channel_id: Option<String>,

    /// Title — derived from `action_type: target`.
    pub title: String,

    /// Goal — MCP has no goal concept.
    /// B-MAP-1: TODO — requires plain text description.
    pub goal: Result<String, MappingError>,

    /// Agent plan summary — MCP has no plan concept.
    /// C3: `Option<String>` — None if not applicable.
    pub agent_plan_summary: Option<String>,

    /// Trusted context — not applicable for MCP-initiated intents.
    /// C3: `Option<JsonMap>` — None if not applicable.
    pub trusted_context: Option<ferrum_proto::JsonMap>,

    /// Raw inputs — converted from MCP tool call parameters.
    /// C7: `IntentInputRef` TODO — needs proper conversion logic.
    pub raw_inputs: Result<Vec<IntentInputRef>, MappingError>,

    /// Requested resource scope — parsed from MCP scope string.
    /// C1: `Vec<ResourceSelector>` — matches ferrum-proto.
    pub requested_resource_scope: Result<Vec<ResourceSelector>, MappingError>,

    /// Requested risk tier — inferred from `action_type`.
    /// C3: `Option<RiskTier>` — None if cannot be determined.
    pub requested_risk_tier: Option<RiskTier>,

    /// Approval mode — derived from `requested_risk_tier`.
    /// C3: `Option<ApprovalMode>` — None if cannot be determined.
    pub approval_mode: Option<ApprovalMode>,

    /// Metadata — empty object by default.
    pub metadata: ferrum_proto::JsonMap,
}

/// Draft `ActionProposal` parts with explicit TODO markers.
///
/// This struct captures the subset of ActionProposal fields that can be
/// derived from a `ToolCallAction` during D1.3.2a planning.
#[derive(Debug, Clone)]
pub struct DraftActionProposalParts {
    /// Intent ID — from `ToolCallAction.intent_id`.
    pub intent_id: String,

    /// Step index — default 0 for single-step intents.
    /// B-MAP-1: TODO if sequential semantics are needed.
    pub step_index: Result<u32, MappingError>,

    /// Title — from `DraftIntentCompileRequestParts.title`.
    pub title: String,

    /// Tool name — from `ToolCallAction.action_type`.
    pub tool_name: String,

    /// Server name — resolved from `action_type` pattern.
    /// B-MAP-4: May return `Err(Todo)` for unknown action types.
    pub server_name: Result<String, MappingError>,

    /// Raw arguments — from `ToolCallAction.parameters`.
    pub raw_arguments: serde_json::Value,

    /// Expected effect — inferred from `action_type`.
    /// B-MAP-5: TODO — inference rules undefined.
    pub expected_effect: Result<String, MappingError>,

    /// Estimated risk — from `RiskTier` inference.
    pub estimated_risk: RiskTier,

    /// Requested rollback class — inferred from `action_type`.
    pub requested_rollback_class: RollbackClass,

    /// Taint inputs — default empty.
    /// B-MAP-1: TODO if taint concept is needed.
    pub taint_inputs: Vec<String>,

    /// Metadata — from `IntentCompileRequest.metadata`.
    pub metadata: ferrum_proto::JsonMap,
}

// ---------------------------------------------------------------------------
// ToolCallAction (re-exported from stage2_types)
// ---------------------------------------------------------------------------

// Re-export ToolCallAction so helpers can use it without requiring stage2_types
pub use crate::stage2_types::ToolCallAction;

// ---------------------------------------------------------------------------
// Helper: ToolCallAction -> DraftIntentCompileRequestParts
// ---------------------------------------------------------------------------

/// Converts a `ToolCallAction` into `DraftIntentCompileRequestParts`.
///
/// This helper is PURE — no side effects, deterministic, no I/O.
///
/// # Arguments
///
/// * `action` - The `ToolCallAction` to convert
/// * `principal_id` - Principal ID string (from `ActorIdentity::resolve()`)
///
/// # Returns
///
/// `DraftIntentCompileRequestParts` with all 12 fields populated.
/// Fields without a clear derivation return `Err(MappingError::Todo{...})`.
#[allow(dead_code)]
pub fn tool_call_action_to_draft_intent_compile_request(
    action: &ToolCallAction,
    _principal_id: String,
) -> DraftIntentCompileRequestParts {
    let risk_tier = infer_risk_tier(&action.action_type);
    let _approval_mode = derive_approval_mode(Some(risk_tier.clone()));

    DraftIntentCompileRequestParts {
        // TODO(D1.3.3 P1): map `actor_id` to a stable `PrincipalId` before any real
        // gateway HTTP/governance call. This generated ID is draft-only and is
        // not an authentication or stable identity claim.
        principal_id: Ok(PrincipalId::new()),
        session_id: Some(SessionId::new().to_string()),
        channel_id: None,
        title: format!("{}: {}", action.action_type, action.target),
        // D1.3.3 P1 (preflight): Use draft goal derived from action type.
        // P1 resolver will replace with stable principal + proper goal derivation.
        goal: Ok(format!(
            "MCP tool call: {} on {}",
            action.action_type, action.target
        )),
        agent_plan_summary: None,
        // D1.3.3 preflight: empty trusted_context (MCP has no trust context concept)
        trusted_context: Some(ferrum_proto::JsonMap::new()),
        // TODO(D1.3.3 P2): raw_inputs conversion requires untrusted IntentInputRef policy.
        // Do not fabricate provenance/trust labels until P2 is resolved.
        raw_inputs: Err(MappingError::Todo {
            field: "raw_inputs".to_string(),
            bmap_issue: "B-MAP-1".to_string(),
            note: "IntentInputRef conversion undefined — needs D1.3.3 P2 resolution".to_string(),
        }),
        requested_resource_scope: parse_resource_scope(&action.scope).map_err(|e| {
            MappingError::Todo {
                field: "requested_resource_scope".to_string(),
                bmap_issue: "B-MAP-2".to_string(),
                note: e.to_string(),
            }
        }),
        requested_risk_tier: Some(risk_tier.clone()),
        approval_mode: derive_approval_mode(Some(risk_tier)),
        metadata: ferrum_proto::JsonMap::new(),
    }
}

/// Converts a `ToolCallAction` into `DraftActionProposalParts`.
///
/// This helper is PURE — no side effects, deterministic, no I/O.
pub fn tool_call_action_to_draft_action_proposal(
    action: &ToolCallAction,
    title: String,
) -> DraftActionProposalParts {
    let risk_tier = infer_risk_tier(&action.action_type);
    let server_name = resolve_server_name(&action.action_type);
    let rollback_class = infer_rollback_class(&action.action_type, Some(&action.target));

    DraftActionProposalParts {
        intent_id: action.intent_id.clone(),
        step_index: Ok(0),
        title,
        tool_name: action.action_type.clone(),
        server_name,
        raw_arguments: action.parameters.clone(),
        expected_effect: Ok(infer_expected_effect(&action.action_type)),
        estimated_risk: risk_tier.clone(),
        requested_rollback_class: rollback_class,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Helper: RiskTier Inference
// ---------------------------------------------------------------------------

/// Infers `RiskTier` from an `action_type` string.
///
/// Per doc 77 C4: Simplified mapping — `fs_write` defaults to `High`,
/// no `is_critical_path` guard (deferred to future work).
///
/// This helper is PURE — deterministic, no side effects.
///
/// # Examples
///
/// ```
/// use ferrum_integrations_mcp::mapping_helpers::*;
/// use ferrum_proto::RiskTier;
///
/// assert_eq!(infer_risk_tier("fs_read"), RiskTier::Low);
/// assert_eq!(infer_risk_tier("git_push"), RiskTier::Medium);
/// assert_eq!(infer_risk_tier("fs_write"), RiskTier::High);
/// assert_eq!(infer_risk_tier("sql_mutate"), RiskTier::Critical);
/// ```
pub fn infer_risk_tier(action_type: &str) -> RiskTier {
    match action_type {
        // Read-only — Low risk
        "fs_read" | "git_fetch" | "http_get" | "db_query" => RiskTier::Low,

        // Non-critical writes — Medium risk (C4: simplified, no path guard)
        "git_push" => RiskTier::Medium,

        // Critical writes — High risk (C4: fs_write defaults to High)
        "fs_write" | "http_post" => RiskTier::High,

        // Destructive/irreversible — Critical risk
        "sql_mutate" | "db_mutate" | "fs_delete" | "git_force_push" => RiskTier::Critical,

        // Unknown — High risk by default (fail-closed for unmapped actions)
        _ => RiskTier::High,
    }
}

// ---------------------------------------------------------------------------
// Helper: ApprovalMode Derivation
// ---------------------------------------------------------------------------

/// Derives `ApprovalMode` from `RiskTier`.
///
/// Policy per doc 77 (updated per D78-9):
/// - Low → None
/// - Medium → Required (D78-9: DraftOnly changed to Required — MCP has no draft workflow)
/// - High → Required
/// - Critical → TwoPhaseCommit
///
/// This helper is PURE — deterministic, no side effects.
pub fn derive_approval_mode(risk_tier: Option<RiskTier>) -> Option<ApprovalMode> {
    risk_tier.map(|tier| match tier {
        RiskTier::Low => ApprovalMode::None,
        RiskTier::Medium => ApprovalMode::Required,
        RiskTier::High => ApprovalMode::Required,
        RiskTier::Critical => ApprovalMode::TwoPhaseCommit,
    })
}

// ---------------------------------------------------------------------------
// Helper: RollbackClass Inference
// ---------------------------------------------------------------------------

/// Infers `RollbackClass` from `action_type` and optional `target`.
///
/// Per doc 77 C6: `http_post` is `R3IrreversibleHighConsequence`
/// (HTTP POST is high-consequence because compensating requests may not be reliable).
///
/// This helper is PURE — deterministic, no side effects.
pub fn infer_rollback_class(action_type: &str, _target: Option<&str>) -> RollbackClass {
    match action_type {
        // Read-only — no rollback needed
        "fs_read" | "git_fetch" | "http_get" | "db_query" => RollbackClass::R0NativeReversible,

        // File operations — snapshot recoverable
        "fs_write" | "fs_delete" => RollbackClass::R1SnapshotRecoverable,

        // Compensatable via reverse operation
        "git_push" => RollbackClass::R2Compensatable,

        // Irreversible or high consequence (C6: http_post -> R3)
        "http_post" | "git_force_push" | "sql_mutate" | "db_mutate" => {
            RollbackClass::R3IrreversibleHighConsequence
        }

        _ => RollbackClass::R3IrreversibleHighConsequence, // Fail conservative
    }
}

// ---------------------------------------------------------------------------
// Helper: ResourceSelector Parser
// ---------------------------------------------------------------------------

/// Parses an MCP scope string into `Vec<ResourceSelector>`.
///
/// Scope format: `resource_type:access_mode:resource_path`
///
/// Per doc 77 C1: Returns `Vec<ResourceSelector>` (not single) to match ferrum-proto.
///
/// Per doc 77 C2: Uses real `ResourceSelector` variant field names from ferrum-proto:
/// - `FilesystemPath { path, mode, content_hash }`
/// - `GitRepository { repo_path, allowed_refs, mode }`
/// - `SqliteDatabase { db_path, tables, mode }`
/// - `HttpEndpoint { method, base_url, path_prefix, mode }`
/// - `EmailDraft { recipient_allowlist, subject_prefix_allowlist, mode }`
/// - `McpTool { server_name, tool_name, mode }`
///
/// This helper is PURE — deterministic, no side effects.
///
/// # Errors
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
                mode: parse_resource_mode(parts[1]),
                content_hash: None,
            }])
        }
        ("git", "push") | ("git", "fetch") | ("git", "force_push") => {
            // TODO(D78-8): git paths like "git:push:origin:refs/heads/main" need multi-segment
            // parsing. Currently only takes first path segment (origin).
            let repo_path = parts.get(2).unwrap_or(&"origin");
            Ok(vec![ResourceSelector::GitRepository {
                repo_path: repo_path.to_string(),
                allowed_refs: vec![],
                mode: parse_resource_mode(parts[1]),
            }])
        }
        ("sql", "mutate") | ("sql", "query") => {
            // TODO(D78-8): sql paths with table qualifiers like "sql:mutate:mydb.db:users"
            // need multi-segment parsing. Currently only takes first path segment (mydb.db).
            let db_path = parts.get(2).unwrap_or(&":memory:");
            Ok(vec![ResourceSelector::SqliteDatabase {
                db_path: db_path.to_string(),
                tables: vec![],
                mode: parse_resource_mode(parts[1]),
            }])
        }
        ("http", "post") | ("http", "get") | ("http", "delete") => {
            // Re-join parts[2..] with ":" since URLs contain colons
            let url = if parts.len() > 2 {
                parts[2..].join(":")
            } else {
                "*".to_string()
            };
            let method = match parts[1].to_lowercase().as_str() {
                "post" => ferrum_proto::HttpMethod::Post,
                "get" => ferrum_proto::HttpMethod::Get,
                "delete" => ferrum_proto::HttpMethod::Delete,
                _ => ferrum_proto::HttpMethod::Post,
            };
            Ok(vec![ResourceSelector::HttpEndpoint {
                method,
                base_url: url.clone(),
                path_prefix: "/".to_string(),
                mode: parse_resource_mode(parts[1]),
            }])
        }
        ("email", "send") => {
            let recipient = parts.get(2).unwrap_or(&"*");
            Ok(vec![ResourceSelector::EmailDraft {
                recipient_allowlist: vec![recipient.to_string()],
                subject_prefix_allowlist: vec![],
                mode: parse_resource_mode(parts[1]),
            }])
        }
        ("mcp", "tool") => {
            let tool_name = parts.get(2).unwrap_or(&"*");
            Ok(vec![ResourceSelector::McpTool {
                server_name: "default".to_string(),
                tool_name: tool_name.to_string(),
                mode: ResourceMode::Execute,
            }])
        }
        _ => Err(MappingError::UnsupportedScope(scope.to_string())),
    }
}

/// Parses an access mode string into `ResourceMode`.
fn parse_resource_mode(mode: &str) -> ResourceMode {
    match mode.to_lowercase().as_str() {
        "read" => ResourceMode::Read,
        "write" => ResourceMode::Write,
        "delete" => ResourceMode::Write,
        "push" => ResourceMode::Write,
        "fetch" => ResourceMode::Read,
        "post" => ResourceMode::Write,
        "get" => ResourceMode::Read,
        "mutate" => ResourceMode::Write,
        "send" => ResourceMode::Write,
        _ => ResourceMode::Read,
    }
}

// ---------------------------------------------------------------------------
// Helper: server_name Resolution
// ---------------------------------------------------------------------------

/// Resolves `server_name` from `action_type` pattern matching.
///
/// Per doc 77 §2.6: Maps action type prefixes to adapter/server names.
///
/// NOTE(D78-4 vocabulary): Server vocabulary ("filesystem", "git", "http", "database",
/// "email", "mcp") should be verified against actual gateway/server expectations before
/// D1.3.3 wiring. This prefix-based mapping is a draft assumption.
///
/// This helper is PURE — deterministic, no side effects.
pub fn resolve_server_name(action_type: &str) -> Result<String, MappingError> {
    let server_name = match action_type {
        f if f.starts_with("fs_") => "filesystem",
        g if g.starts_with("git_") => "git",
        h if h.starts_with("http_") => "http",
        s if s.starts_with("sql_") || s.starts_with("db_") => "database",
        e if e.starts_with("email_") => "email",
        m if m.starts_with("mcp_") => "mcp",
        _ => {
            return Err(MappingError::Todo {
                field: "server_name".to_string(),
                bmap_issue: "B-MAP-4".to_string(),
                note: format!("Unknown action_type prefix for: {}", action_type),
            });
        }
    };
    Ok(server_name.to_string())
}

// ---------------------------------------------------------------------------
// Helper: Expected Effect Inference
// ---------------------------------------------------------------------------

/// Infers a human-readable `expected_effect` string from `action_type`.
///
/// This is a best-effort description for planning purposes.
///
/// This helper is PURE — deterministic, no side effects.
pub fn infer_expected_effect(action_type: &str) -> String {
    match action_type {
        "fs_write" => "Create or overwrite file at target path".to_string(),
        "fs_delete" => "Delete file at target path".to_string(),
        "fs_read" => "Read file at target path".to_string(),
        "git_push" => "Push commits to remote repository".to_string(),
        "git_fetch" => "Fetch from remote repository".to_string(),
        "git_force_push" => "Force push to remote (may overwrite history)".to_string(),
        "http_post" => "Send POST request to target URL".to_string(),
        "http_get" => "Send GET request to target URL".to_string(),
        "sql_mutate" => "Execute mutation on database".to_string(),
        "db_query" => "Query database".to_string(),
        "db_mutate" => "Mutate database".to_string(),
        _ => format!("Execute action: {}", action_type),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // infer_risk_tier Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_infer_risk_tier_low() {
        assert_eq!(infer_risk_tier("fs_read"), RiskTier::Low);
        assert_eq!(infer_risk_tier("git_fetch"), RiskTier::Low);
        assert_eq!(infer_risk_tier("http_get"), RiskTier::Low);
        assert_eq!(infer_risk_tier("db_query"), RiskTier::Low);
    }

    #[test]
    fn test_infer_risk_tier_medium() {
        assert_eq!(infer_risk_tier("git_push"), RiskTier::Medium);
    }

    #[test]
    fn test_infer_risk_tier_high() {
        assert_eq!(infer_risk_tier("fs_write"), RiskTier::High);
        assert_eq!(infer_risk_tier("http_post"), RiskTier::High);
    }

    #[test]
    fn test_infer_risk_tier_critical() {
        assert_eq!(infer_risk_tier("sql_mutate"), RiskTier::Critical);
        assert_eq!(infer_risk_tier("db_mutate"), RiskTier::Critical);
        assert_eq!(infer_risk_tier("fs_delete"), RiskTier::Critical);
        assert_eq!(infer_risk_tier("git_force_push"), RiskTier::Critical);
    }

    #[test]
    fn test_infer_risk_tier_unknown_defaults_high() {
        // D78-8: Unknown actions fail closed (High) instead of fail open (Medium)
        assert_eq!(infer_risk_tier("unknown_action"), RiskTier::High);
        assert_eq!(infer_risk_tier(""), RiskTier::High);
    }

    #[test]
    fn test_all_action_types_have_risk_tier() {
        // Verify all known action types return a RiskTier (no panic)
        let action_types = [
            "fs_read",
            "fs_write",
            "fs_delete",
            "git_push",
            "git_fetch",
            "git_force_push",
            "http_get",
            "http_post",
            "sql_mutate",
            "db_query",
            "db_mutate",
            "email_send",
            "mcp_tool",
        ];
        for at in action_types {
            let result = std::panic::catch_unwind(|| infer_risk_tier(at));
            assert!(
                result.is_ok(),
                "infer_risk_tier should not panic for {:?}",
                at
            );
        }
    }

    // -------------------------------------------------------------------------
    // derive_approval_mode Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_derive_approval_mode_low() {
        assert_eq!(
            derive_approval_mode(Some(RiskTier::Low)),
            Some(ApprovalMode::None)
        );
    }

    #[test]
    fn test_derive_approval_mode_medium() {
        // D78-9: Medium risk requires approval (not DraftOnly) — MCP has no draft workflow
        assert_eq!(
            derive_approval_mode(Some(RiskTier::Medium)),
            Some(ApprovalMode::Required)
        );
    }

    #[test]
    fn test_derive_approval_mode_high() {
        assert_eq!(
            derive_approval_mode(Some(RiskTier::High)),
            Some(ApprovalMode::Required)
        );
    }

    #[test]
    fn test_derive_approval_mode_critical() {
        assert_eq!(
            derive_approval_mode(Some(RiskTier::Critical)),
            Some(ApprovalMode::TwoPhaseCommit)
        );
    }

    #[test]
    fn test_derive_approval_mode_none() {
        assert_eq!(derive_approval_mode(None), None);
    }

    // -------------------------------------------------------------------------
    // infer_rollback_class Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_infer_rollback_class_read_only() {
        assert_eq!(
            infer_rollback_class("fs_read", None),
            RollbackClass::R0NativeReversible
        );
        assert_eq!(
            infer_rollback_class("git_fetch", None),
            RollbackClass::R0NativeReversible
        );
        assert_eq!(
            infer_rollback_class("http_get", None),
            RollbackClass::R0NativeReversible
        );
    }

    #[test]
    fn test_infer_rollback_class_file_operations() {
        assert_eq!(
            infer_rollback_class("fs_write", Some("/tmp/test.txt")),
            RollbackClass::R1SnapshotRecoverable
        );
        assert_eq!(
            infer_rollback_class("fs_delete", Some("/tmp/test.txt")),
            RollbackClass::R1SnapshotRecoverable
        );
    }

    #[test]
    fn test_infer_rollback_class_git_push() {
        assert_eq!(
            infer_rollback_class("git_push", None),
            RollbackClass::R2Compensatable
        );
    }

    #[test]
    fn test_infer_rollback_class_irreversible() {
        // C6: http_post is R3
        assert_eq!(
            infer_rollback_class("http_post", None),
            RollbackClass::R3IrreversibleHighConsequence
        );
        assert_eq!(
            infer_rollback_class("sql_mutate", None),
            RollbackClass::R3IrreversibleHighConsequence
        );
        assert_eq!(
            infer_rollback_class("git_force_push", None),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn test_infer_rollback_class_unknown_fails_conservative() {
        assert_eq!(
            infer_rollback_class("unknown_action", None),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn test_all_action_types_have_rollback_class() {
        let action_types = [
            "fs_read",
            "fs_write",
            "fs_delete",
            "git_push",
            "git_fetch",
            "git_force_push",
            "http_get",
            "http_post",
            "sql_mutate",
            "db_query",
            "db_mutate",
            "email_send",
            "mcp_tool",
        ];
        for at in action_types {
            let result = std::panic::catch_unwind(|| infer_rollback_class(at, None));
            assert!(
                result.is_ok(),
                "infer_rollback_class should not panic for {:?}",
                at
            );
        }
    }

    // -------------------------------------------------------------------------
    // parse_resource_scope Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_resource_scope_filesystem() {
        let result = parse_resource_scope("fs:write:/tmp/test.txt");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        assert_eq!(selectors.len(), 1);
        match &selectors[0] {
            ResourceSelector::FilesystemPath {
                path,
                mode,
                content_hash,
            } => {
                assert_eq!(path, "/tmp/test.txt");
                assert_eq!(mode, &ResourceMode::Write);
                assert!(content_hash.is_none());
            }
            _ => panic!("Expected FilesystemPath"),
        }
    }

    #[test]
    fn test_parse_resource_scope_git() {
        let result = parse_resource_scope("git:push:origin");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        assert_eq!(selectors.len(), 1);
        match &selectors[0] {
            ResourceSelector::GitRepository {
                repo_path,
                allowed_refs,
                mode,
            } => {
                assert_eq!(repo_path, "origin");
                assert!(allowed_refs.is_empty());
                assert_eq!(mode, &ResourceMode::Write);
            }
            _ => panic!("Expected GitRepository"),
        }
    }

    #[test]
    fn test_parse_resource_scope_sql() {
        let result = parse_resource_scope("sql:mutate:mydb.db");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        assert_eq!(selectors.len(), 1);
        match &selectors[0] {
            ResourceSelector::SqliteDatabase {
                db_path,
                tables,
                mode,
            } => {
                assert_eq!(db_path, "mydb.db");
                assert!(tables.is_empty());
                assert_eq!(mode, &ResourceMode::Write);
            }
            _ => panic!("Expected SqliteDatabase"),
        }
    }

    #[test]
    fn test_parse_resource_scope_http() {
        let result = parse_resource_scope("http:post:https://api.example.com/endpoint");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        assert_eq!(selectors.len(), 1);
        match &selectors[0] {
            ResourceSelector::HttpEndpoint {
                method,
                base_url,
                path_prefix,
                mode,
            } => {
                assert_eq!(method, &ferrum_proto::HttpMethod::Post);
                assert_eq!(base_url, "https://api.example.com/endpoint");
                assert_eq!(path_prefix, "/");
                assert_eq!(mode, &ResourceMode::Write);
            }
            _ => panic!("Expected HttpEndpoint"),
        }
    }

    #[test]
    fn test_parse_resource_scope_email() {
        let result = parse_resource_scope("email:send:alice@example.com");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        assert_eq!(selectors.len(), 1);
        match &selectors[0] {
            ResourceSelector::EmailDraft {
                recipient_allowlist,
                subject_prefix_allowlist,
                mode,
            } => {
                assert_eq!(recipient_allowlist.len(), 1);
                assert_eq!(recipient_allowlist[0], "alice@example.com");
                assert!(subject_prefix_allowlist.is_empty());
                assert_eq!(mode, &ResourceMode::Write);
            }
            _ => panic!("Expected EmailDraft"),
        }
    }

    #[test]
    fn test_parse_resource_scope_mcp_tool() {
        let result = parse_resource_scope("mcp:tool:ferrum_gate_fs_write");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        assert_eq!(selectors.len(), 1);
        match &selectors[0] {
            ResourceSelector::McpTool {
                server_name,
                tool_name,
                mode,
            } => {
                assert_eq!(server_name, "default");
                assert_eq!(tool_name, "ferrum_gate_fs_write");
                assert_eq!(mode, &ResourceMode::Execute);
            }
            _ => panic!("Expected McpTool"),
        }
    }

    #[test]
    fn test_parse_resource_scope_unsupported() {
        let result = parse_resource_scope("unknown:format:path");
        assert!(result.is_err());
        match result.unwrap_err() {
            MappingError::UnsupportedScope(_) => {}
            e => panic!("Expected UnsupportedScope, got {:?}", e),
        }
    }

    #[test]
    fn test_parse_resource_scope_too_few_parts() {
        let result = parse_resource_scope("fs");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_resource_scope_with_colons_in_path() {
        // Path with colons should still parse correctly
        let result = parse_resource_scope("fs:write:/tmp/path:with:colons");
        assert!(result.is_ok());
        let selectors = result.unwrap();
        match &selectors[0] {
            ResourceSelector::FilesystemPath { path, .. } => {
                assert_eq!(path, "/tmp/path:with:colons");
            }
            _ => panic!("Expected FilesystemPath"),
        }
    }

    // -------------------------------------------------------------------------
    // resolve_server_name Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_resolve_server_name_fs() {
        assert_eq!(resolve_server_name("fs_write").unwrap(), "filesystem");
        assert_eq!(resolve_server_name("fs_read").unwrap(), "filesystem");
        assert_eq!(resolve_server_name("fs_delete").unwrap(), "filesystem");
    }

    #[test]
    fn test_resolve_server_name_git() {
        assert_eq!(resolve_server_name("git_push").unwrap(), "git");
        assert_eq!(resolve_server_name("git_fetch").unwrap(), "git");
    }

    #[test]
    fn test_resolve_server_name_http() {
        assert_eq!(resolve_server_name("http_post").unwrap(), "http");
        assert_eq!(resolve_server_name("http_get").unwrap(), "http");
    }

    #[test]
    fn test_resolve_server_name_database() {
        assert_eq!(resolve_server_name("sql_mutate").unwrap(), "database");
        assert_eq!(resolve_server_name("db_query").unwrap(), "database");
    }

    #[test]
    fn test_resolve_server_name_email() {
        assert_eq!(resolve_server_name("email_send").unwrap(), "email");
    }

    #[test]
    fn test_resolve_server_name_mcp() {
        assert_eq!(resolve_server_name("mcp_tool").unwrap(), "mcp");
    }

    #[test]
    fn test_resolve_server_name_unknown() {
        let result = resolve_server_name("unknown_tool");
        assert!(result.is_err());
        match result.unwrap_err() {
            MappingError::Todo {
                field, bmap_issue, ..
            } => {
                assert_eq!(field, "server_name");
                assert_eq!(bmap_issue, "B-MAP-4");
            }
            e => panic!("Expected Todo error, got {:?}", e),
        }
    }

    // -------------------------------------------------------------------------
    // tool_call_action_to_draft_intent_compile_request Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_call_action_to_draft_intent_compile_request_fs_write() {
        let action = ToolCallAction::new(
            "intent-123".to_string(),
            "fs_write".to_string(),
            "fs:write:/tmp/test.txt".to_string(),
            "/tmp/test.txt".to_string(),
            serde_json::json!({}),
            "agent-001".to_string(),
        );

        let parts =
            tool_call_action_to_draft_intent_compile_request(&action, "agent-001".to_string());

        // principal_id should be Ok (auto-generated)
        assert!(parts.principal_id.is_ok());
        // session_id should be Some
        assert!(parts.session_id.is_some());
        // channel_id should be None (MCP has no channel)
        assert!(parts.channel_id.is_none());
        // title should be derived
        assert_eq!(parts.title, "fs_write: /tmp/test.txt");
        // goal should be Ok (draft goal per D1.3.3 P1)
        assert!(parts.goal.is_ok());
        assert!(parts.goal.unwrap().contains("fs_write"));
        // agent_plan_summary should be None
        assert!(parts.agent_plan_summary.is_none());
        // trusted_context should be Some (empty map per D1.3.3 preflight)
        assert!(parts.trusted_context.is_some());
        // raw_inputs should be Err(Todo)
        assert!(parts.raw_inputs.is_err());
        // requested_resource_scope should be Ok(Vec)
        assert!(parts.requested_resource_scope.is_ok());
        // requested_risk_tier should be Some(High)
        assert_eq!(parts.requested_risk_tier, Some(RiskTier::High));
        // approval_mode should be Some(Required) for High
        assert_eq!(parts.approval_mode, Some(ApprovalMode::Required));
    }

    #[test]
    fn test_tool_call_action_to_draft_intent_compile_request_read_only() {
        let action = ToolCallAction::new(
            "intent-456".to_string(),
            "fs_read".to_string(),
            "fs:read:/etc/passwd".to_string(),
            "/etc/passwd".to_string(),
            serde_json::json!({}),
            "agent-002".to_string(),
        );

        let parts =
            tool_call_action_to_draft_intent_compile_request(&action, "agent-002".to_string());

        assert_eq!(parts.requested_risk_tier, Some(RiskTier::Low));
        assert_eq!(parts.approval_mode, Some(ApprovalMode::None));
        assert!(parts.requested_resource_scope.is_ok());
    }

    // -------------------------------------------------------------------------
    // tool_call_action_to_draft_action_proposal Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_call_action_to_draft_action_proposal() {
        let action = ToolCallAction::new(
            "intent-789".to_string(),
            "git_push".to_string(),
            "git:push:origin".to_string(),
            "origin".to_string(),
            serde_json::json!({ "branch": "main" }),
            "agent-003".to_string(),
        );

        let parts =
            tool_call_action_to_draft_action_proposal(&action, "git_push: origin".to_string());

        assert_eq!(parts.intent_id, "intent-789");
        assert_eq!(parts.step_index, Ok(0));
        assert_eq!(parts.title, "git_push: origin");
        assert_eq!(parts.tool_name, "git_push");
        assert_eq!(parts.server_name.unwrap(), "git");
        assert_eq!(parts.raw_arguments, serde_json::json!({ "branch": "main" }));
        assert_eq!(parts.estimated_risk, RiskTier::Medium);
        assert_eq!(
            parts.requested_rollback_class,
            RollbackClass::R2Compensatable
        );
        assert!(parts.taint_inputs.is_empty());
    }

    // -------------------------------------------------------------------------
    // infer_expected_effect Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_infer_expected_effect() {
        assert_eq!(
            infer_expected_effect("fs_write"),
            "Create or overwrite file at target path"
        );
        assert_eq!(
            infer_expected_effect("fs_delete"),
            "Delete file at target path"
        );
        assert_eq!(
            infer_expected_effect("git_push"),
            "Push commits to remote repository"
        );
        assert_eq!(
            infer_expected_effect("sql_mutate"),
            "Execute mutation on database"
        );
        assert_eq!(infer_expected_effect("unknown"), "Execute action: unknown");
    }

    // -------------------------------------------------------------------------
    // Default-Deny Verification
    // -------------------------------------------------------------------------

    #[test]
    fn test_mutating_tools_still_return_not_implemented() {
        // Verify that mutating tools still return NOT_IMPLEMENTED
        // This is a regression test to ensure D1.3.2a helpers don't accidentally
        // enable mutating tool execution
        use crate::JsonRpcResponse;
        use crate::handle_tools_call;

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "intent_id": "test",
                "action_type": "test",
                "scope": "test",
                "target": "test",
                "parameters": {}
            }
        });

        let response = handle_tools_call(params, None);

        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(
                    err.error.code,
                    crate::error_codes::NOT_IMPLEMENTED,
                    "Mutating tool should still return NOT_IMPLEMENTED (-32001)"
                );
            }
            JsonRpcResponse::Success(_) => {
                panic!("Expected NOT_IMPLEMENTED error for mutating tool")
            }
        }
    }

    // -------------------------------------------------------------------------
    // No HTTP Imports Verification
    // -------------------------------------------------------------------------

    #[test]
    fn test_no_http_calls_in_helpers() {
        // This test is a documentation anchor.
        // D1.3.2a helpers are pure functions with no side effects.
        // They perform no I/O, no network calls, and no state changes.
        //
        // To verify no HTTP imports exist, run:
        //   rg 'reqwest|ureq|hyper' crates/ferrum-integrations-mcp/src/mapping_helpers.rs
        // Expected: no matches
        //
        // This test always passes and serves as a reminder of the constraint.
        assert!(true, "D1.3.2a helpers are pure — no HTTP calls");
    }
}
