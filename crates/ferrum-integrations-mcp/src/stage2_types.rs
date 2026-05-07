//! # Stage 2 Governance Pipeline Types (D-1.3.1)
//!
//! > **UNVERIFIED**: All DTOs in this module are defined from explorer findings and doc 74 design.
//! > They have NOT been validated against a running gateway with real schemas.
//! > Endpoint paths, field names, and nested structures may drift until blockers (B1-B4) are resolved.
//!
//! ## D1.3.1 Scope (Types-Only)
//!
//! This module defines types for Stage 2 governance pipeline planning:
//! - `ToolCallAction`: Internal MCP-to-governance mapping structure
//! - `IntentCompileRequest`: External REST API DTO for `/v1/intents/compile`
//! - `IntentCompileResponse`: Response DTO containing `proposal_id`
//!
//! ## D1.3.1 Permitted
//!
//! - Define Rust structs for DTOs
//! - Implement `Serialize`/`Deserialize` for serde roundtrip testing
//! - Add unit tests for serialization (no HTTP calls)
//!
//! ## D1.3.1 Forbidden
//!
//! - Calling `POST /v1/intents/compile` or any governance endpoint
//! - Pipeline logic (intent → proposal → capability → execution)
//! - Any mutating behavior

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ToolCallAction (Internal MCP-to-Governance Mapping)
// ---------------------------------------------------------------------------

/// Internal MCP-to-governance mapping structure.
///
/// Per doc 75 §1.2.1: `ToolCallAction` is the **internal** Rust struct that maps
/// MCP tool call fields to governance pipeline concepts.
///
/// This struct is used *inside* the MCP server to collect tool call information
/// before constructing an `IntentCompileRequest` for the REST API.
///
/// ## Fields
///
/// - `intent_id`: Unique identifier for this intent (UUID)
/// - `action_type`: Type of action being requested
/// - `scope`: Authorization scope required
/// - `target`: Target resource or entity
/// - `parameters`: Tool-specific parameters (JSON)
/// - `actor_id`: ID of the actor making the request
///
/// # UNVERIFIED
///
/// Field names and structure may drift based on actual gateway API validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallAction {
    /// Unique identifier for this intent.
    /// Format: UUID v4
    pub intent_id: String,

    /// Type of action being requested.
    /// Examples: "fs_write", "git_push", "http_post", "sql_mutate"
    pub action_type: String,

    /// Authorization scope required for this action.
    /// Format: Dot-separated scope string (e.g., "files:write:/tmp")
    pub scope: String,

    /// Target resource or entity.
    /// Format: Resource URI or identifier
    pub target: String,

    /// Tool-specific parameters as JSON.
    /// Structure depends on the tool being invoked.
    pub parameters: serde_json::Value,

    /// ID of the actor making the request.
    /// Resolved from `ActorIdentity::resolve()`.
    pub actor_id: String,
}

impl ToolCallAction {
    /// Create a new `ToolCallAction` from MCP tool call parameters.
    ///
    /// # Arguments
    ///
    /// * `intent_id` - Unique intent identifier
    /// * `action_type` - Type of action (e.g., "fs_write")
    /// * `scope` - Authorization scope
    /// * `target` - Target resource
    /// * `parameters` - Tool-specific parameters
    /// * `actor_id` - Actor making the request
    #[allow(dead_code)]
    pub fn new(
        intent_id: String,
        action_type: String,
        scope: String,
        target: String,
        parameters: serde_json::Value,
        actor_id: String,
    ) -> Self {
        Self {
            intent_id,
            action_type,
            scope,
            target,
            parameters,
            actor_id,
        }
    }
}

// ---------------------------------------------------------------------------
// IntentCompileRequest (External REST API DTO)
// ---------------------------------------------------------------------------

/// External REST API DTO for `/v1/intents/compile` endpoint.
///
/// Per doc 75 §1.2.1: `IntentCompileRequest` is the **external** REST API DTO
/// sent to the gateway's compile endpoint. The MCP server *serializes* a
/// `ToolCallAction` into an `IntentCompileRequest` before calling the REST API.
///
/// ## Endpoint
///
/// - **Path**: `POST /v1/intents/compile`
/// - **Auth**: Bearer token required
/// - **Content-Type**: `application/json`
///
/// ## Example
///
/// ```json
/// {
///   "intent_id": "uuid-1234",
///   "action_type": "fs_write",
///   "scope": "files:write:/tmp",
///   "target": "/tmp/test.txt",
///   "parameters": { "content": "hello" },
///   "actor_id": "agent-001"
/// }
/// ```
///
/// # UNVERIFIED
///
/// This DTO is defined from explorer findings. Field names, types, and
/// nested structures may drift until real gateway integration testing confirms
/// actual shapes. Do not assume stability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCompileRequest {
    /// Unique identifier for this intent.
    /// Maps to `ToolCallAction.intent_id`.
    pub intent_id: String,

    /// Type of action being requested.
    /// Maps to `ToolCallAction.action_type`.
    pub action_type: String,

    /// Authorization scope required for this action.
    /// Maps to `ToolCallAction.scope`.
    pub scope: String,

    /// Target resource or entity.
    /// Maps to `ToolCallAction.target`.
    pub target: String,

    /// Tool-specific parameters as JSON.
    /// Maps to `ToolCallAction.parameters`.
    pub parameters: serde_json::Value,

    /// ID of the actor making the request.
    /// Maps to `ToolCallAction.actor_id`.
    pub actor_id: String,
}

impl From<ToolCallAction> for IntentCompileRequest {
    /// Convert internal `ToolCallAction` to external `IntentCompileRequest`.
    ///
    /// This implements the mapping from internal MCP representation to
    /// external REST API DTO as defined in doc 75 §1.2.1.
    fn from(action: ToolCallAction) -> Self {
        Self {
            intent_id: action.intent_id,
            action_type: action.action_type,
            scope: action.scope,
            target: action.target,
            parameters: action.parameters,
            actor_id: action.actor_id,
        }
    }
}

// ---------------------------------------------------------------------------
// IntentCompileResponse (External REST API Response DTO)
// ---------------------------------------------------------------------------

/// Response DTO from `/v1/intents/compile` endpoint.
///
/// **DEPRECATED (D1.3.2b):** This struct is a D1.3.1 placeholder that does NOT
/// match the real gateway `IntentCompileResponse`.
///
/// Per doc 78 (D78-2): The real gateway compile response is:
/// ```json
/// { "envelope": { "intent_id": "...", ... }, "warnings": [] }
/// ```
///
/// The `envelope` contains `intent_id` (not `proposal_id`). The `proposal_id`
/// used for proposal evaluation is generated by the MCP/client BEFORE evaluation,
/// not parsed from the compile output.
///
/// ## D1.3.2b Note
///
/// For D1.3.2b pure/draft mapping, use `generate_proposal_id()` to create
/// a client-side proposal ID before calling proposal evaluation.
///
/// TODO(D1.3.3): Remove this deprecated struct when real REST wiring is implemented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCompileResponse {
    /// Unique identifier for the compiled proposal.
    ///
    /// DEPRECATED: This field does NOT exist in the real gateway response.
    /// The real response is `{ envelope, warnings }`. `proposal_id` is generated
    /// by the client before proposal evaluation, not parsed from compile output.
    #[deprecated(
        since = "D1.3.2b",
        note = "Real gateway response is {{ envelope, warnings }}. proposal_id is client-generated."
    )]
    pub proposal_id: String,
}

/// Generates a client-side proposal ID for governance pipeline evaluation.
///
/// Per doc 78 (D78-2): `proposal_id` is generated by the MCP/client before
/// proposal evaluation, not parsed from the compile response.
///
/// This function is PURE — deterministic, no side effects, no I/O.
#[allow(dead_code)]
pub fn generate_proposal_id() -> String {
    ferrum_proto::ProposalId::new().to_string()
}

// ---------------------------------------------------------------------------
// Pipeline Step and Status Enums (Type-Level Planning)
// ---------------------------------------------------------------------------

/// Governance pipeline step identifiers.
///
/// These represent the sequential stages in the governance pipeline
/// as defined in doc 75 §2.1.
///
/// # UNVERIFIED
///
/// These are type-level identifiers for planning purposes. The actual
/// pipeline implementation (D1.3.2+) is gated and requires B1-B4 resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStep {
    /// Compile intent to proposal (POST /v1/intents/compile)
    Compile,
    /// Evaluate proposal against policy (POST /v1/proposals/{id}/evaluate)
    PolicyEval,
    /// Mint capability token (POST /v1/capabilities/mint)
    CapabilityMint,
    /// Authorize execution (POST /v1/executions/authorize)
    Authorize,
    /// Prepare execution (POST /v1/executions/{id}/prepare)
    Prepare,
    /// Execute prepared action (POST /v1/executions/{id}/execute)
    Execute,
    /// Verify execution result (POST /v1/executions/{id}/verify)
    Verify,
    /// Compensate/Rollback (POST /v1/executions/{id}/compensate)
    Compensate,
}

impl PipelineStep {
    /// Returns the REST endpoint path for this step (without base URL).
    ///
    /// Returns `None` for steps that are internal to the gateway
    /// (CapabilityMint, Authorize) or not yet designed.
    #[allow(dead_code)]
    pub fn endpoint_path(&self) -> Option<&'static str> {
        match self {
            PipelineStep::Compile => Some("/v1/intents/compile"),
            PipelineStep::PolicyEval => Some("/v1/proposals/{proposal_id}/evaluate"),
            PipelineStep::CapabilityMint => Some("/v1/capabilities/mint"),
            PipelineStep::Authorize => Some("/v1/executions/authorize"),
            PipelineStep::Prepare => Some("/v1/executions/{execution_id}/prepare"),
            PipelineStep::Execute => Some("/v1/executions/{execution_id}/execute"),
            PipelineStep::Verify => Some("/v1/executions/{execution_id}/verify"),
            PipelineStep::Compensate => Some("/v1/executions/{execution_id}/compensate"),
        }
    }

    /// Returns the HTTP method for this step.
    #[allow(dead_code)]
    pub const fn http_method(&self) -> &'static str {
        "POST"
    }
}

/// Status of a governance pipeline execution.
///
/// # UNVERIFIED
///
/// Status values are from explorer findings. Actual status values
/// may differ based on real gateway behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    /// Execution completed successfully.
    Completed,
    /// Execution is pending (async, requires polling).
    Pending,
    /// Execution failed.
    Failed,
    /// Execution was denied by policy.
    Denied,
    /// Execution was compensated/rolled back.
    Compensated,
}

impl std::fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStatus::Completed => write!(f, "completed"),
            PipelineStatus::Pending => write!(f, "pending"),
            PipelineStatus::Failed => write!(f, "failed"),
            PipelineStatus::Denied => write!(f, "denied"),
            PipelineStatus::Compensated => write!(f, "compensated"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (Serialization Roundtrip Only - No HTTP Calls)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // ToolCallAction Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_call_action_serialization_roundtrip() {
        let action = ToolCallAction::new(
            "intent-123".to_string(),
            "fs_write".to_string(),
            "files:write:/tmp".to_string(),
            "/tmp/test.txt".to_string(),
            serde_json::json!({ "content": "hello world" }),
            "agent-001".to_string(),
        );

        // Serialize to JSON
        let json = serde_json::to_string(&action).expect("Should serialize");

        // Deserialize back
        let deserialized: ToolCallAction = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(deserialized.intent_id, "intent-123");
        assert_eq!(deserialized.action_type, "fs_write");
        assert_eq!(deserialized.scope, "files:write:/tmp");
        assert_eq!(deserialized.target, "/tmp/test.txt");
        assert_eq!(deserialized.actor_id, "agent-001");
        assert_eq!(
            deserialized.parameters,
            serde_json::json!({ "content": "hello world" })
        );
    }

    #[test]
    fn test_tool_call_action_from_json() {
        let json = r#"{
            "intent_id": "uuid-test",
            "action_type": "git_push",
            "scope": "git:push:origin",
            "target": "origin",
            "parameters": { "branch": "main", "force": false },
            "actor_id": "agent-002"
        }"#;

        let action: ToolCallAction = serde_json::from_str(json).expect("Should deserialize");

        assert_eq!(action.intent_id, "uuid-test");
        assert_eq!(action.action_type, "git_push");
        assert_eq!(action.scope, "git:push:origin");
        assert_eq!(action.target, "origin");
        assert!(action.parameters.is_object());
        assert_eq!(action.actor_id, "agent-002");
    }

    #[test]
    fn test_tool_call_action_empty_parameters() {
        let action = ToolCallAction::new(
            "intent-456".to_string(),
            "http_post".to_string(),
            "http:post:*".to_string(),
            "https://api.example.com".to_string(),
            serde_json::json!({}),
            "agent-003".to_string(),
        );

        let json = serde_json::to_string(&action).expect("Should serialize");
        let deserialized: ToolCallAction = serde_json::from_str(&json).expect("Should deserialize");

        assert!(deserialized.parameters.is_object());
        assert_eq!(deserialized.parameters, serde_json::json!({}));
    }

    // -------------------------------------------------------------------------
    // IntentCompileRequest Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_intent_compile_request_serialization_roundtrip() {
        let request = IntentCompileRequest {
            intent_id: "intent-789".to_string(),
            action_type: "sql_mutate".to_string(),
            scope: "db:write:users".to_string(),
            target: "users table".to_string(),
            parameters: serde_json::json!({
                "query": "INSERT INTO users (name) VALUES ($1)",
                "params": ["Alice"]
            }),
            actor_id: "agent-004".to_string(),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&request).expect("Should serialize");

        // Verify JSON structure
        assert!(json.contains("\"intent_id\":\"intent-789\""));
        assert!(json.contains("\"action_type\":\"sql_mutate\""));
        assert!(json.contains("\"scope\":\"db:write:users\""));

        // Deserialize back
        let deserialized: IntentCompileRequest =
            serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(deserialized.intent_id, "intent-789");
        assert_eq!(deserialized.action_type, "sql_mutate");
        assert_eq!(deserialized.scope, "db:write:users");
        assert_eq!(deserialized.target, "users table");
        assert_eq!(deserialized.actor_id, "agent-004");
    }

    #[test]
    fn test_intent_compile_request_from_json() {
        let json = r#"{
            "intent_id": "compile-123",
            "action_type": "fs_read",
            "scope": "files:read:/etc",
            "target": "/etc/passwd",
            "parameters": { "encoding": "utf-8" },
            "actor_id": "agent-005"
        }"#;

        let request: IntentCompileRequest = serde_json::from_str(json).expect("Should deserialize");

        assert_eq!(request.intent_id, "compile-123");
        assert_eq!(request.action_type, "fs_read");
        assert_eq!(request.scope, "files:read:/etc");
        assert_eq!(request.target, "/etc/passwd");
        assert_eq!(request.actor_id, "agent-005");
    }

    // -------------------------------------------------------------------------
    // IntentCompileResponse Tests (DEPRECATED — tests deprecated struct)
    // -------------------------------------------------------------------------

    #[test]
    #[allow(deprecated)]
    fn test_intent_compile_response_serialization_roundtrip() {
        // DEPRECATED: This tests the D1.3.1 placeholder, not real gateway behavior.
        // Real response is { envelope, warnings } per doc 78 D78-2.
        let response = IntentCompileResponse {
            proposal_id: "proposal-abc".to_string(),
        };

        let json = serde_json::to_string(&response).expect("Should serialize");
        assert!(json.contains("\"proposal_id\":\"proposal-abc\""));

        let deserialized: IntentCompileResponse =
            serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(deserialized.proposal_id, "proposal-abc");
    }

    #[test]
    #[allow(deprecated)]
    fn test_intent_compile_response_from_json() {
        // DEPRECATED: This tests the D1.3.1 placeholder, not real gateway behavior.
        let json = r#"{"proposal_id": "uuid-from-gateway"}"#;

        let response: IntentCompileResponse =
            serde_json::from_str(json).expect("Should deserialize");

        assert_eq!(response.proposal_id, "uuid-from-gateway");
    }

    // -------------------------------------------------------------------------
    // Client-Generated proposal_id Tests (D1.3.2b)
    // -------------------------------------------------------------------------

    #[test]
    fn test_generate_proposal_id_produces_uuid() {
        // Per doc 78 D78-2: proposal_id is client-generated before evaluation
        let id = generate_proposal_id();
        // UUID v4 format: 8-4-4-4-12 hex digits
        assert!(id.len() == 36, "UUID should be 36 characters");
        assert!(
            id.chars().filter(|c| *c == '-').count() == 4,
            "UUID should have 4 hyphens"
        );
    }

    #[test]
    fn test_generate_proposal_id_unique() {
        // Each call should produce a unique ID
        let id1 = generate_proposal_id();
        let id2 = generate_proposal_id();
        assert_ne!(id1, id2, "Each call should generate a unique ID");
    }

    // -------------------------------------------------------------------------
    // ToolCallAction -> IntentCompileRequest Conversion Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_call_action_to_intent_compile_request() {
        let action = ToolCallAction::new(
            "intent-convert".to_string(),
            "test_action".to_string(),
            "test:scope".to_string(),
            "test-target".to_string(),
            serde_json::json!({ "key": "value" }),
            "test-actor".to_string(),
        );

        let request: IntentCompileRequest = action.into();

        assert_eq!(request.intent_id, "intent-convert");
        assert_eq!(request.action_type, "test_action");
        assert_eq!(request.scope, "test:scope");
        assert_eq!(request.target, "test-target");
        assert_eq!(request.parameters, serde_json::json!({ "key": "value" }));
        assert_eq!(request.actor_id, "test-actor");
    }

    #[test]
    fn test_serialize_then_convert_roundtrip() {
        // Create action, serialize, deserialize as different type
        let action = ToolCallAction::new(
            "roundtrip-test".to_string(),
            "http_get".to_string(),
            "http:get:*".to_string(),
            "https://api.example.com/data".to_string(),
            serde_json::json!({ "headers": {} }),
            "agent-roundtrip".to_string(),
        );

        // Serialize action to JSON
        let action_json = serde_json::to_string(&action).expect("Should serialize");

        // Deserialize as IntentCompileRequest directly
        let request: IntentCompileRequest =
            serde_json::from_str(&action_json).expect("Should deserialize from same JSON");

        assert_eq!(request.intent_id, "roundtrip-test");
        assert_eq!(request.action_type, "http_get");
        assert_eq!(request.actor_id, "agent-roundtrip");
    }

    // -------------------------------------------------------------------------
    // PipelineStep Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_pipeline_step_endpoint_path() {
        assert_eq!(
            PipelineStep::Compile.endpoint_path(),
            Some("/v1/intents/compile")
        );
        assert_eq!(
            PipelineStep::PolicyEval.endpoint_path(),
            Some("/v1/proposals/{proposal_id}/evaluate")
        );
        assert_eq!(
            PipelineStep::CapabilityMint.endpoint_path(),
            Some("/v1/capabilities/mint")
        );
        assert_eq!(
            PipelineStep::Prepare.endpoint_path(),
            Some("/v1/executions/{execution_id}/prepare")
        );
        assert_eq!(
            PipelineStep::Execute.endpoint_path(),
            Some("/v1/executions/{execution_id}/execute")
        );
        assert_eq!(
            PipelineStep::Compensate.endpoint_path(),
            Some("/v1/executions/{execution_id}/compensate")
        );
    }

    #[test]
    fn test_pipeline_step_http_method() {
        for step in [
            PipelineStep::Compile,
            PipelineStep::PolicyEval,
            PipelineStep::CapabilityMint,
            PipelineStep::Authorize,
            PipelineStep::Prepare,
            PipelineStep::Execute,
            PipelineStep::Verify,
            PipelineStep::Compensate,
        ] {
            assert_eq!(step.http_method(), "POST");
        }
    }

    #[test]
    fn test_pipeline_step_serialization() {
        let step = PipelineStep::Execute;
        let json = serde_json::to_string(&step).expect("Should serialize");
        assert_eq!(json, "\"execute\"");

        let deserialized: PipelineStep = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized, PipelineStep::Execute);
    }

    // -------------------------------------------------------------------------
    // PipelineStatus Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_pipeline_status_serialization() {
        let statuses = [
            (PipelineStatus::Completed, "\"completed\""),
            (PipelineStatus::Pending, "\"pending\""),
            (PipelineStatus::Failed, "\"failed\""),
            (PipelineStatus::Denied, "\"denied\""),
            (PipelineStatus::Compensated, "\"compensated\""),
        ];

        for (status, expected_json) in statuses {
            let json = serde_json::to_string(&status).expect("Should serialize");
            assert_eq!(json, expected_json);

            let deserialized: PipelineStatus =
                serde_json::from_str(&json).expect("Should deserialize");
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn test_pipeline_status_display() {
        assert_eq!(PipelineStatus::Completed.to_string(), "completed");
        assert_eq!(PipelineStatus::Pending.to_string(), "pending");
        assert_eq!(PipelineStatus::Failed.to_string(), "failed");
        assert_eq!(PipelineStatus::Denied.to_string(), "denied");
        assert_eq!(PipelineStatus::Compensated.to_string(), "compensated");
    }

    // -------------------------------------------------------------------------
    // Default-Deny Verification (No HTTP Calls)
    // -------------------------------------------------------------------------

    /// Verify that mutating tools still return NOT_IMPLEMENTED.
    /// This test ensures no behavioral change in D1.3.1 types-only implementation.
    #[test]
    fn test_mutating_tools_still_return_not_implemented() {
        use crate::JsonRpcResponse;
        use crate::handle_tools_call;

        // Verify that calling a mutating tool still returns NOT_IMPLEMENTED
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
                    "Mutating tool should still return NOT_IMPLEMENTED (-32001) in D1.3.1"
                );
            }
            JsonRpcResponse::Success(_) => {
                panic!("Expected NOT_IMPLEMENTED error for mutating tool in D1.3.1")
            }
        }
    }
}
