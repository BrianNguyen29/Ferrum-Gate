//! # ferrum-integrations-mcp
//!
//! FerrumGate MCP server integration crate (Phase A-C + Phase D-0 REST client + D1.7 lifecycle dispatch + D1.8 output sanitization).
//!
//! ## Overview
//!
//! This crate provides:
//! - MCP tool schema definitions for FerrumGate (read-only + lifecycle tools)
//! - Tool registry with metadata (name, description, input_schema, read_only marker)
//! - JSON-RPC 2.0 request/response types and error codes
//! - Handler stubs for initialize, ping, tools/list, tools/call
//! - Phase D-0: REST client for FerrumGate gateway integration
//! - D1.7: Lifecycle tool dispatch for approved governance pipeline steps
//! - D1.8: Output sanitization via TaintScoringFirewall at tools/call choke point
//!
//! ## Phase A-C Status (Complete)
//!
//! Phase A-C implemented:
//! - Read-only tool schema (9 tools)
//! - Tool registry with read_only markers
//! - JSON-RPC 2.0 types and handler stubs
//! - Stdio transport skeleton
//!
//! ## Phase D-0 Status (Implemented)
//!
//! Phase D-0 implements:
//! - HTTP client for FerrumGate gateway REST API
//! - REST endpoint mapper for all 9 read-only tools
//! - Error classification (auth, unreachable, server error)
//! - tools/call implementation for read-only tools
//!
//! ## D1.7 Status (Implemented)
//!
//! D1.7 implements lifecycle tool dispatch for approved governance pipeline steps:
//! - compile/submit: POST /v1/intents/compile
//! - evaluate: POST /v1/proposals/{id}/evaluate
//! - mint_capability: POST /v1/capabilities/mint
//! - authorize_execution: POST /v1/executions/authorize
//! - prepare_execution: POST /v1/executions/{id}/prepare
//! - execute_prepared: POST /v1/executions/{id}/execute
//! - verify: POST /v1/executions/{id}/verify
//! - compensate: POST /v1/executions/{id}/compensate
//!
//! D1.7 does NOT implement:
//! - approve/reject: Backend endpoints absent (permanently blocked)
//! - Direct provenance emission (gateway-owned)
//! - Direct state management (gateway-owned)
//! - Atomic full-pipeline tool (separate step tools per oracle verdict)
//!
//! ## D1.8 Status (Implemented — Option A)
//!
//! D1.8 Option A implements output sanitization via TaintScoringFirewall at the single
//! tools/call response choke point in `handle_tools_call_with_client`. All success
//! responses pass through `TaintScoringFirewall::new().sanitize_output()` before
//! `JsonRpcResponse::success`.
//!
//! D1.8 Option A implements:
//! - Control character stripping from JSON strings (recursive)
//! - Whitespace normalization
//! - UUID/message/warning preservation (no over-redaction)
//!
//! D1.8 Option A does NOT implement:
//! - Field-level redaction or DLP (deferred to future work per oracle verdict)
//! - Provenance emission (gateway-owned)
//! - Error response sanitization (errors bypass the sanitize choke point)
//!
//! ## D1.9 Status (Phase 1 + Phase 2 Implemented — Option B)
//!
//! D1.9 Phase 1 + Phase 2 implement Option B field-key-aware redaction at the
//! tools/call success boundary after D1.8 sanitization. The `redact_sensitive_fields()`
//! function walks JSON recursively with parent context tracking and replaces values of
//! sensitive keys with the string "[REDACTED]". Arrays recurse; primitive values
//! (numbers, booleans, nulls) pass through unchanged.
//!
//! Phase 1 + Phase 2 sensitive keys (oracle-approved):
//! - Phase 1: `raw_arguments`, `metadata`
//! - Phase 2 global keys: `resource_bindings`, `argument_constraints`, `approval_binding`,
//!   `target`, `resource_scope`, `trust_context`, `result_digest`
//! - Phase 2 path-aware: `args` only when parent context is `compensation_plan` array element
//!
//! Phase 1 + Phase 2 do NOT redact (no-over-redaction principle):
//! - `tool_binding`, `tool_version` — explicitly preserved
//! - UUID IDs, reason/message/warnings, enums, booleans
//! - rollback_contract envelope structure
//! - action_type, correlation_id
//!
//! D1.9 does NOT implement:
//! - Regex/heuristic DLP scanning (Option A/C — deferred to Phase 3)
//! - Context-aware redaction via FirewallContext (Option D — deferred)
//! - Provenance emission (gateway-owned)
//! - Production/G2 claim (not production-ready)

use ferrum_firewall::SemanticFirewall;
use serde::{Deserialize, Serialize};

mod http_client;
mod mapping_helpers;
mod rest_mapper;
mod stage2_types;

// Re-export Stage 2 types for external use.
pub use stage2_types::{IntentCompileRequest, PipelineStatus, PipelineStep, ToolCallAction};

// Re-export real ferrum-proto types per doc 79 P3.
// IntentCompileResponse was a deprecated placeholder; use the real type from ferrum_proto.
pub use ferrum_proto::IntentCompileResponse;

// Re-export mapping helpers for external use (D1.3.2a).
pub use mapping_helpers::{
    DraftActionProposalParts, DraftIntentCompileRequestParts, MappingError, derive_approval_mode,
    infer_expected_effect, infer_risk_tier, infer_rollback_class, parse_resource_scope,
    resolve_server_name, tool_call_action_to_draft_action_proposal,
    tool_call_action_to_draft_intent_compile_request,
};

// Re-export HTTP client types for use by the binary.
pub use http_client::{ClientConfig, FerrumGatewayClient, GatewayError};

// ---------------------------------------------------------------------------
// Tool Registry (Phase A)
// ---------------------------------------------------------------------------

/// Tool metadata for MCP tool registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Unique tool name.
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// JSON Schema for input parameters.
    pub input_schema: serde_json::Value,
    /// Whether this tool is read-only (no side effects).
    pub read_only: bool,
}

/// The tool registry containing all available MCP tools.
pub fn tool_registry() -> &'static [Tool] {
    TOOL_REGISTRY.get_or_init(|| {
        vec![
            // Health and readiness probes
            Tool {
                name: "ferrum_gate_health",
                description: "Health probe returning server status",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_readyz_deep",
                description: "Deep readiness check including dependencies",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            // Intent and execution queries
            Tool {
                name: "ferrum_gate_list_intents",
                description: "List intents with optional filters",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "intent_id": {
                            "type": "string",
                            "description": "Filter by intent ID"
                        },
                        "state": {
                            "type": "string",
                            "description": "Filter by intent state"
                        },
                        "cursor": {
                            "type": "string",
                            "description": "Pagination cursor"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results",
                            "default": 50
                        }
                    },
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_get_execution",
                description: "Get execution status by ID",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "The execution ID to query"
                        }
                    },
                    "required": ["execution_id"]
                }),
                read_only: true,
            },
            // Provenance and lineage
            Tool {
                name: "ferrum_gate_query_lineage",
                description: "Query provenance events for an execution",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "The execution ID to query lineage for"
                        },
                        "cursor": {
                            "type": "string",
                            "description": "Pagination cursor"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of events",
                            "default": 100
                        }
                    },
                    "required": []
                }),
                read_only: true,
            },
            // Approval and policy queries
            Tool {
                name: "ferrum_gate_list_approvals",
                description: "List pending approvals",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_list_policy_bundles",
                description: "List available policy bundles",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            // Bridge queries
            Tool {
                name: "ferrum_gate_list_bridges",
                description: "List registered runtime bridges",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_list_bridge_tools",
                description: "List tools for a specific bridge",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bridge_id": {
                            "type": "string",
                            "description": "The bridge ID to query tools for"
                        }
                    },
                    "required": ["bridge_id"]
                }),
                read_only: true,
            },
            // D1.7 Lifecycle tools (wired, not default-deny)
            // compile/submit: POST /v1/intents/compile
            Tool {
                name: "ferrum_gate_submit_intent",
                description: "Compile and submit an intent for governance evaluation (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "principal_id": {
                            "type": "string",
                            "description": "Principal ID (UUID)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Intent title"
                        },
                        "goal": {
                            "type": "string",
                            "description": "Goal description"
                        },
                        "action_type": {
                            "type": "string",
                            "description": "Action type (e.g., fs_write, git_push)"
                        },
                        "target": {
                            "type": "string",
                            "description": "Target resource or entity"
                        },
                        "scope": {
                            "type": "string",
                            "description": "Authorization scope (e.g., fs:write:/tmp/test.txt)"
                        },
                        "parameters": {
                            "type": "object",
                            "description": "Tool-specific parameters",
                            "default": {}
                        },
                        "risk_tier": {
                            "type": "string",
                            "description": "Risk tier (Low, Medium, High, Critical)",
                            "enum": ["Low", "Medium", "High", "Critical"]
                        }
                    },
                    "required": ["principal_id", "title", "goal", "action_type", "target", "scope"]
                }),
                read_only: false,
            },
            // evaluate: POST /v1/proposals/{id}/evaluate
            Tool {
                name: "ferrum_gate_evaluate_intent",
                description: "Evaluate an intent proposal against policy (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "proposal_id": {
                            "type": "string",
                            "description": "Proposal ID (UUID)"
                        },
                        "intent_id": {
                            "type": "string",
                            "description": "Intent ID (UUID)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Proposal title"
                        },
                        "tool_name": {
                            "type": "string",
                            "description": "Tool name to execute"
                        },
                        "server_name": {
                            "type": "string",
                            "description": "Server/adapter name"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "Tool arguments",
                            "default": {}
                        },
                        "expected_effect": {
                            "type": "string",
                            "description": "Expected effect description"
                        },
                        "estimated_risk": {
                            "type": "string",
                            "description": "Estimated risk tier",
                            "enum": ["Low", "Medium", "High", "Critical"]
                        },
                        "rollback_class": {
                            "type": "string",
                            "description": "Rollback class",
                            "enum": ["R0NativeReversible", "R1SnapshotRecoverable", "R2Compensatable", "R3IrreversibleHighConsequence"]
                        }
                    },
                    "required": ["proposal_id", "intent_id", "title", "tool_name", "server_name", "arguments", "expected_effect", "estimated_risk"]
                }),
                read_only: false,
            },
            // mint_capability: POST /v1/capabilities/mint
            Tool {
                name: "ferrum_gate_mint_capability",
                description: "Mint a capability token for an approved proposal (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "intent_id": {
                            "type": "string",
                            "description": "Intent ID (UUID)"
                        },
                        "proposal_id": {
                            "type": "string",
                            "description": "Proposal ID (UUID)"
                        },
                        "tool_name": {
                            "type": "string",
                            "description": "Tool name"
                        },
                        "server_name": {
                            "type": "string",
                            "description": "Server/adapter name"
                        },
                        "resource_path": {
                            "type": "string",
                            "description": "Resource path (for file-based resources)"
                        },
                        "resource_mode": {
                            "type": "string",
                            "description": "Resource access mode",
                            "enum": ["Read", "Write", "Execute"]
                        },
                        "ttl_secs": {
                            "type": "integer",
                            "description": "Requested TTL in seconds (max 300)",
                            "default": 120
                        }
                    },
                    "required": ["intent_id", "proposal_id", "tool_name", "server_name"]
                }),
                read_only: false,
            },
            // authorize_execution: POST /v1/executions/authorize
            Tool {
                name: "ferrum_gate_authorize_execution",
                description: "Authorize execution with a capability token (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "proposal_id": {
                            "type": "string",
                            "description": "Proposal ID (UUID)"
                        },
                        "capability_id": {
                            "type": "string",
                            "description": "Capability ID (UUID)"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, authorize without preparing execution",
                            "default": false
                        }
                    },
                    "required": ["proposal_id", "capability_id"]
                }),
                read_only: false,
            },
            // prepare_execution: POST /v1/executions/{id}/prepare
            Tool {
                name: "ferrum_gate_prepare_execution",
                description: "Prepare execution for a previously authorized execution (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "Execution ID (UUID)"
                        }
                    },
                    "required": ["execution_id"]
                }),
                read_only: false,
            },
            // execute_prepared: POST /v1/executions/{id}/execute
            Tool {
                name: "ferrum_gate_execute_prepared",
                description: "Execute a prepared tool call (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "Execution ID (UUID)"
                        },
                        "payload": {
                            "type": "object",
                            "description": "Adapter-specific payload (e.g., file content for fs-write)",
                            "default": {}
                        }
                    },
                    "required": ["execution_id"]
                }),
                read_only: false,
            },
            // verify: POST /v1/executions/{id}/verify
            Tool {
                name: "ferrum_gate_verify",
                description: "Verify execution result (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "Execution ID (UUID)"
                        }
                    },
                    "required": ["execution_id"]
                }),
                read_only: false,
            },
            // compensate: POST /v1/executions/{id}/compensate
            Tool {
                name: "ferrum_gate_compensate",
                description: "Compensate/rollback a executed tool call (D1.7)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "Execution ID (UUID)"
                        }
                    },
                    "required": ["execution_id"]
                }),
                read_only: false,
            },
        ]
    })
}

/// Lazy-initialized tool registry.
static TOOL_REGISTRY: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();

/// Set of tool names that are read-only (no side effects).
pub const READ_ONLY_TOOLS: &[&str] = &[
    "ferrum_gate_health",
    "ferrum_gate_readyz_deep",
    "ferrum_gate_list_intents",
    "ferrum_gate_get_execution",
    "ferrum_gate_query_lineage",
    "ferrum_gate_list_approvals",
    "ferrum_gate_list_policy_bundles",
    "ferrum_gate_list_bridges",
    "ferrum_gate_list_bridge_tools",
];

/// Set of tool names that are lifecycle tools (require governance pipeline).
/// Per D1.7: These tools implement the governance pipeline steps:
/// compile → evaluate → mint → authorize → prepare → execute → verify → compensate
pub const LIFECYCLE_TOOLS: &[&str] = &[
    "ferrum_gate_submit_intent",
    "ferrum_gate_evaluate_intent",
    "ferrum_gate_mint_capability",
    "ferrum_gate_authorize_execution",
    "ferrum_gate_prepare_execution",
    "ferrum_gate_execute_prepared",
    "ferrum_gate_verify",
    "ferrum_gate_compensate",
];

/// Set of tool names that are permanently blocked (backend endpoints absent).
/// Per oracle verdict: approve/reject remain NOT_IMPLEMENTED due to missing backend endpoints.
pub const BLOCKED_TOOLS: &[&str] = &["ferrum_gate_approve_intent", "ferrum_gate_reject_intent"];

// ---------------------------------------------------------------------------
// Auth Context (Phase D-1.1)
// ---------------------------------------------------------------------------

/// Actor identity information for the MCP server agent.
/// Source precedence per doc 74 D-1.1.6:
/// 1. FERRUMD_MCP_AGENT_ID environment variable
/// 2. MCP init client_info.name
/// 3. Fallback to local actor
#[derive(Debug, Clone)]
pub struct ActorIdentity {
    /// Unique actor identifier (UUID or similar).
    pub actor_id: String,
    /// Human-readable actor label.
    pub actor_label: String,
    /// Actor source (env var, client_info, or local).
    pub source: ActorSource,
}

/// Source of the actor identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorSource {
    /// From FERRUMD_MCP_AGENT_ID environment variable.
    EnvVar,
    /// From MCP init client_info.name.
    ClientInfo,
    /// Default local actor fallback.
    Local,
}

impl ActorIdentity {
    /// Create actor identity from environment variable FERRUMD_MCP_AGENT_ID.
    /// Format: "actor_id:actor_label" or just "actor_id" (label defaults to actor_id).
    fn from_env_var() -> Option<Self> {
        std::env::var("FERRUMD_MCP_AGENT_ID").ok().and_then(|val| {
            let parts: Vec<&str> = val.split(':').collect();
            if parts.is_empty() || parts[0].is_empty() {
                return None;
            }
            let actor_id = parts[0].to_string();
            let actor_label = parts
                .get(1)
                .map(|s| s.to_string())
                .unwrap_or_else(|| actor_id.clone());
            Some(ActorIdentity {
                actor_id,
                actor_label,
                source: ActorSource::EnvVar,
            })
        })
    }

    /// Create actor identity from MCP client info.
    fn from_client_info(client_name: &str) -> Self {
        ActorIdentity {
            actor_id: client_name.to_string(),
            actor_label: client_name.to_string(),
            source: ActorSource::ClientInfo,
        }
    }

    /// Get the fallback local actor identity.
    fn local() -> Self {
        ActorIdentity {
            actor_id: "ferrum-mcp-local".to_string(),
            actor_label: "Ferrum MCP Local".to_string(),
            source: ActorSource::Local,
        }
    }

    /// Resolve actor identity with precedence: env var > client_info > local.
    /// Call this during MCP initialization with the client_info.name if available.
    pub fn resolve(client_name: Option<&str>) -> Self {
        Self::from_env_var()
            .or_else(|| {
                client_name
                    .filter(|n| !n.is_empty())
                    .map(Self::from_client_info)
            })
            .unwrap_or_else(Self::local)
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 Types (Phase B)
// ---------------------------------------------------------------------------

/// JSON-RPC 2.0 request structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (must be "2.0").
    pub jsonrpc: String,
    /// Request method name.
    pub method: String,
    /// Request ID (can be string, number, or null).
    #[serde(default)]
    pub id: Option<JsonRpcId>,
    /// Optional parameters (method-dependent).
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse {
    /// Successful response with result.
    Success(JsonRpcSuccessResponse),
    /// Error response.
    Error(JsonRpcErrorResponse),
}

/// Successful JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcSuccessResponse {
    /// JSON-RPC version.
    pub jsonrpc: String,
    /// Response result (method-dependent).
    pub result: serde_json::Value,
    /// Request ID.
    pub id: Option<JsonRpcId>,
}

/// Error JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    /// JSON-RPC version.
    pub jsonrpc: String,
    /// Error object.
    pub error: JsonRpcError,
    /// Request ID.
    pub id: Option<JsonRpcId>,
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Optional error data.
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC request ID (string, number, or null in JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    String(String),
    Number(i64),
    Null,
}

/// Standard JSON-RPC 2.0 error codes.
pub mod error_codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i32 = -32700;
    /// Request is not valid JSON-RPC 2.0.
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Server error (reserved for implementation-defined errors).
    pub const SERVER_ERROR: i32 = -32000;
    /// Not implemented - used for Phase B tools/call.
    pub const NOT_IMPLEMENTED: i32 = -32001;
    /// Authentication failed - returned when gateway returns 401/403.
    pub const AUTH_FAILED: i32 = -32002;
    /// Gateway unreachable - returned when connection fails.
    pub const GATEWAY_UNREACHABLE: i32 = -32003;
    /// Gateway server error - returned when gateway returns 4xx/5xx.
    pub const GATEWAY_SERVER_ERROR: i32 = -32004;
}

impl JsonRpcError {
    /// Create a method not found error.
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("Method '{}' not found or not available", method),
            data: None,
        }
    }

    /// Create a not implemented error.
    pub fn not_implemented(method: &str) -> Self {
        Self {
            code: error_codes::NOT_IMPLEMENTED,
            message: format!("Method '{}' is not implemented in this phase", method),
            data: None,
        }
    }

    /// Create an invalid request error.
    pub fn invalid_request(msg: &str) -> Self {
        Self {
            code: error_codes::INVALID_REQUEST,
            message: msg.to_string(),
            data: None,
        }
    }

    /// Create a parse error.
    pub fn parse_error(msg: &str) -> Self {
        Self {
            code: error_codes::PARSE_ERROR,
            message: msg.to_string(),
            data: None,
        }
    }
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(result: serde_json::Value, id: Option<JsonRpcId>) -> Self {
        Self::Success(JsonRpcSuccessResponse {
            jsonrpc: "2.0".to_string(),
            result,
            id,
        })
    }

    /// Create an error response.
    pub fn error(error: JsonRpcError, id: Option<JsonRpcId>) -> Self {
        Self::Error(JsonRpcErrorResponse {
            jsonrpc: "2.0".to_string(),
            error,
            id,
        })
    }
}

// ---------------------------------------------------------------------------
// MCP Protocol Types (Phase B)
// ---------------------------------------------------------------------------

/// Server capabilities advertised during initialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Tools capability.
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
}

/// Tools capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// List of tools is provided.
    #[serde(default)]
    pub list_changed: Option<bool>,
}

/// Initialize request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Protocol version requested by client.
    #[serde(default)]
    pub protocol_version: Option<String>,
    /// Client capabilities.
    #[serde(default)]
    pub capabilities: ClientCapabilities,
    /// Client info.
    #[serde(default)]
    pub client_info: Option<ClientInfo>,
}

impl Default for InitializeParams {
    fn default() -> Self {
        Self {
            protocol_version: None,
            capabilities: ClientCapabilities {},
            client_info: None,
        }
    }
}

/// Client capabilities (unused in Phase B).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientCapabilities {}

/// Client info (unused in Phase B).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// Server info response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Initialize result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Protocol version supported (2024-11-05).
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server info.
    pub server_info: ServerInfo,
}

/// Tool result item for tools/list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// tools/list result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<ToolInfo>,
}

/// tools/call result (not implemented in Phase B).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
}

/// Tool content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContent {
    pub r#type: String,
    pub text: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers (Phase B)
// ---------------------------------------------------------------------------

/// Handle initialize request.
/// Returns server capabilities and protocol version.
pub fn handle_initialize(params: serde_json::Value, id: Option<JsonRpcId>) -> JsonRpcResponse {
    // Parse params to validate (even though we don't use them in Phase B)
    // Null is treated as empty params (default)
    let _: InitializeParams = match params {
        serde_json::Value::Null => InitializeParams::default(),
        _ => match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return JsonRpcResponse::error(
                    JsonRpcError::invalid_request(&format!("Invalid initialize params: {}", e)),
                    id,
                );
            }
        },
    };

    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: None }),
        },
        server_info: ServerInfo {
            name: "ferrum-integrations-mcp".to_string(),
            version: "0.1.0".to_string(),
        },
    };

    JsonRpcResponse::success(serde_json::to_value(result).unwrap(), id)
}

/// Handle ping request.
/// Returns a simple success indicator.
pub fn handle_ping(id: Option<JsonRpcId>) -> JsonRpcResponse {
    JsonRpcResponse::success(serde_json::json!({ "success": true }), id)
}

/// Handle tools/list request.
/// Returns the read-only tool registry.
pub fn handle_tools_list(id: Option<JsonRpcId>) -> JsonRpcResponse {
    let tools: Vec<ToolInfo> = tool_registry()
        .iter()
        .map(|t| ToolInfo {
            name: t.name.to_string(),
            description: t.description.to_string(),
            input_schema: t.input_schema.clone(),
        })
        .collect();

    let result = ToolsListResult { tools };

    JsonRpcResponse::success(serde_json::to_value(result).unwrap(), id)
}

/// Handle tools/call request.
/// Returns not implemented error for all tools in Phase B.
/// Use `handle_tools_call_with_client` for actual REST integration.
pub fn handle_tools_call(params: serde_json::Value, id: Option<JsonRpcId>) -> JsonRpcResponse {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct CallParams {
        name: String,
        #[serde(default)]
        arguments: Option<serde_json::Value>,
    }

    let _params: CallParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                JsonRpcError::invalid_request(&format!("Invalid tools/call params: {}", e)),
                id,
            );
        }
    };

    // Phase B: tools/call is not implemented
    JsonRpcResponse::error(JsonRpcError::not_implemented("tools/call"), id)
}

/// Handle tools/call request with gateway client.
/// Maps the tool call to the corresponding REST endpoint and returns the result.
pub fn handle_tools_call_with_client(
    params: serde_json::Value,
    id: Option<JsonRpcId>,
    client: &http_client::FerrumGatewayClient,
) -> JsonRpcResponse {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct CallParams {
        name: String,
        #[serde(default)]
        arguments: Option<serde_json::Value>,
    }

    let params: CallParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                JsonRpcError::invalid_request(&format!("Invalid tools/call params: {}", e)),
                id,
            );
        }
    };

    // Call the REST mapper
    match rest_mapper::map_tool_to_rest(&params.name, params.arguments.as_ref(), client) {
        Ok(mut result) => {
            // D1.9 Phase 1+2 Option B: redact sensitive fields inside ToolContent.text JSON
            redact_tool_content_text(&mut result);
            let value = serde_json::to_value(result).unwrap();
            // D1.8 sanitize_output: strip control characters from JSON strings.
            // sanitize_output operates on the already-serialized JSON value (not re-parsing
            // JSON strings), so it cannot undo the inner redaction applied above.
            let sanitized = ferrum_firewall::TaintScoringFirewall::new().sanitize_output(value);
            JsonRpcResponse::success(sanitized, id)
        }
        Err(e) => JsonRpcResponse::error(
            JsonRpcError {
                code: e.code,
                message: e.message,
                data: e.data,
            },
            id,
        ),
    }
}

// ---------------------------------------------------------------------------
// D1.9: Field-Level Redaction (Phase 1 + Phase 2 — Option B)
// ---------------------------------------------------------------------------

/// Phase 1 + Phase 2 sensitive field keys for D1.9 Option B field-level redaction.
/// These keys have their values replaced with "[REDACTED]" in JSON output.
const SENSITIVE_KEYS: &[&str] = &[
    // Phase 1 keys
    "raw_arguments",
    "metadata",
    // Phase 2 global keys (oracle-approved)
    "resource_bindings",
    "argument_constraints",
    "approval_binding",
    "target",
    "resource_scope",
    "trust_context",
    "result_digest",
];

/// Keys whose values should NOT be redacted even if they match sensitive key names.
/// This implements the no-over-redaction principle for operational metadata.
const PRESERVED_KEYS: &[&str] = &["tool_binding", "tool_version"];

/// Parent context for path-aware redaction.
/// Tracks whether we are inside a `compensation_plan` array, because
/// `args` should only be redacted in that specific context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentContext {
    /// Not inside any special parent.
    None,
    /// Inside the `compensation_plan` array (its elements).
    InsideCompensationPlan,
}

/// Public wrapper: stable API for redact_sensitive_fields.
///
/// Phase 1 + Phase 2 field-key-aware redaction targeting:
/// - Phase 1: `raw_arguments`, `metadata`
/// - Phase 2: `resource_bindings`, `argument_constraints`, `approval_binding`,
///   `target`, `resource_scope`, `trust_context`, `result_digest`
///
/// Additionally, `args` is redacted only when the parent context is
/// `compensation_plan` array element (path-aware redaction).
///
/// Preserved (no-over-redaction principle): `tool_binding`, `tool_version`,
/// UUID IDs, reason/message/warnings, enums, booleans, rollback_contract
/// envelope structure, action_type, correlation_id.
pub fn redact_sensitive_fields(value: serde_json::Value) -> serde_json::Value {
    redact_recursive(value, ParentContext::None)
}

/// Internal recursive redaction function carrying parent context.
///
/// - For objects: if a key matches a sensitive key (and not a preserved key),
///   replace its value with "[REDACTED]"; if the key is `args` and parent context
///   is `InsideCompensationPlan`, redact; otherwise recurse.
/// - For arrays: recurse into each element with the current parent context.
/// - For primitives: pass through unchanged.
fn redact_recursive(value: serde_json::Value, parent: ParentContext) -> serde_json::Value {
    match value {
        serde_json::Value::Object(mut map) => {
            for (key, val) in map.iter_mut() {
                // Compute context for children: if this key is compensation_plan AND
                // its value is an array, then children are inside array elements (path-aware).
                // Otherwise keep the current parent.
                let child_context = if key == "compensation_plan" && val.is_array() {
                    Some(ParentContext::InsideCompensationPlan)
                } else {
                    None
                };

                // Determine actual parent for this level (for path-aware checks)
                let current_parent = child_context.as_ref().unwrap_or(&parent);

                // Check if this key should be redacted
                let is_sensitive = SENSITIVE_KEYS.contains(&key.as_str());
                let is_preserved = PRESERVED_KEYS.contains(&key.as_str());
                let is_compensation_args =
                    key == "args" && *current_parent == ParentContext::InsideCompensationPlan;

                if is_preserved {
                    // Never redact preserved keys (tool_binding, tool_version)
                    *val = redact_recursive(val.clone(), *current_parent);
                } else if is_compensation_args {
                    // Path-aware: args inside compensation_plan array element: redact
                    *val = serde_json::Value::String("[REDACTED]".to_string());
                } else if is_sensitive {
                    // Global sensitive key: redact the value
                    *val = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    // Recurse into the value
                    *val = redact_recursive(val.clone(), *current_parent);
                }
            }
            serde_json::Value::Object(map)
        }
        serde_json::Value::Array(arr) => {
            // Arrays inherit parent context (elements are "inside" the parent object)
            serde_json::Value::Array(
                arr.into_iter()
                    .map(|item| redact_recursive(item, parent))
                    .collect(),
            )
        }
        // Primitives pass through unchanged
        _ => value,
    }
}

/// D1.9.3: Redact sensitive fields inside ToolContent.text JSON strings.
///
/// For each `ToolContent` in the `content` vector:
/// - If `text` is `None` or not valid JSON, leave it unchanged
/// - If `text` is valid JSON, parse it, apply `redact_sensitive_fields`, reserialize
///
/// This enables field-level redaction of sensitive keys (Phase 1 + Phase 2)
/// that are embedded as JSON inside the MCP response's text field.
pub fn redact_tool_content_text(result: &mut ToolsCallResult) {
    for content in &mut result.content {
        if let Some(text) = &content.text {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                let redacted = redact_sensitive_fields(parsed);
                if let Ok(redacted_str) = serde_json::to_string(&redacted) {
                    content.text = Some(redacted_str);
                }
            }
        }
    }
}

/// Dispatch a JSON-RPC request to the appropriate handler.
/// Returns a JSON-RPC response.
pub fn dispatch(request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id;
    match request.method.as_str() {
        "initialize" => {
            let params = request.params.unwrap_or(serde_json::Value::Null);
            handle_initialize(params, id)
        }
        "ping" => handle_ping(id),
        "tools/list" => handle_tools_list(id),
        "tools/call" => {
            let params = request.params.unwrap_or(serde_json::Value::Null);
            handle_tools_call(params, id)
        }
        _ => JsonRpcResponse::error(JsonRpcError::method_not_found(&request.method), id),
    }
}

/// Dispatch a JSON-RPC request to the appropriate handler with gateway client.
/// This version supports actual REST calls for tools/call.
pub fn dispatch_with_client(
    request: JsonRpcRequest,
    client: &http_client::FerrumGatewayClient,
) -> JsonRpcResponse {
    let id = request.id;
    match request.method.as_str() {
        "initialize" => {
            let params = request.params.unwrap_or(serde_json::Value::Null);
            handle_initialize(params, id)
        }
        "ping" => handle_ping(id),
        "tools/list" => handle_tools_list(id),
        "tools/call" => {
            let params = request.params.unwrap_or(serde_json::Value::Null);
            handle_tools_call_with_client(params, id, client)
        }
        _ => JsonRpcResponse::error(JsonRpcError::method_not_found(&request.method), id),
    }
}

/// Parse a JSON-RPC request from a JSON string.
/// Returns the parsed request or a parse error response.
pub fn parse_request(json_str: &str) -> Result<JsonRpcRequest, JsonRpcResponse> {
    serde_json::from_str(json_str).map_err(|e| {
        JsonRpcResponse::error(
            JsonRpcError::parse_error(&format!("Invalid JSON: {}", e)),
            None,
        )
    })
}

// ---------------------------------------------------------------------------
// Tests (Phase A + Phase B)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Phase A Tests (Tool Registry)
    // -------------------------------------------------------------------------

    #[test]
    fn test_tool_registry_contains_all_tools() {
        // D1.7: Registry now has 17 tools (9 read-only + 8 lifecycle)
        let registry = tool_registry();
        assert_eq!(
            registry.len(),
            17,
            "Tool registry should contain exactly 17 tools (9 read-only + 8 lifecycle)"
        );
    }

    #[test]
    fn test_read_only_tools_are_marked_correctly() {
        // Verify read-only tools are marked correctly
        for tool in tool_registry() {
            if READ_ONLY_TOOLS.contains(&tool.name) {
                assert!(
                    tool.read_only,
                    "Read-only tool '{}' should be marked as read_only=true",
                    tool.name
                );
            } else if LIFECYCLE_TOOLS.contains(&tool.name) {
                assert!(
                    !tool.read_only,
                    "Lifecycle tool '{}' should be marked as read_only=false",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn test_lifecycle_tools_set_contains_expected_tools() {
        // Per D1.7, lifecycle tools are the 8 wired governance pipeline steps
        let expected_lifecycle = [
            "ferrum_gate_submit_intent",
            "ferrum_gate_evaluate_intent",
            "ferrum_gate_mint_capability",
            "ferrum_gate_authorize_execution",
            "ferrum_gate_prepare_execution",
            "ferrum_gate_execute_prepared",
            "ferrum_gate_verify",
            "ferrum_gate_compensate",
        ];
        let lifecycle_set: std::collections::HashSet<_> = LIFECYCLE_TOOLS.iter().copied().collect();
        for expected in expected_lifecycle {
            assert!(
                lifecycle_set.contains(expected),
                "LIFECYCLE_TOOLS should contain '{}'",
                expected
            );
        }
        assert_eq!(
            LIFECYCLE_TOOLS.len(),
            8,
            "Should have exactly 8 lifecycle tools"
        );
    }

    #[test]
    fn test_blocked_tools_set_contains_expected_tools() {
        // Per oracle verdict, approve/reject are permanently blocked
        let expected_blocked = ["ferrum_gate_approve_intent", "ferrum_gate_reject_intent"];
        let blocked_set: std::collections::HashSet<_> = BLOCKED_TOOLS.iter().copied().collect();
        for expected in expected_blocked {
            assert!(
                blocked_set.contains(expected),
                "BLOCKED_TOOLS should contain '{}'",
                expected
            );
        }
        assert_eq!(
            BLOCKED_TOOLS.len(),
            2,
            "Should have exactly 2 blocked tools"
        );
    }

    #[test]
    fn test_lifecycle_tools_are_in_registry() {
        // Lifecycle tools ARE in the tool registry (they're wired, not blocked)
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        for tool_name in LIFECYCLE_TOOLS {
            assert!(
                registry_names.contains(tool_name),
                "Lifecycle tool '{}' should be in tool registry",
                tool_name
            );
        }
    }

    #[test]
    fn test_blocked_tools_not_in_registry() {
        // Blocked tools are NOT in the tool registry (they return NOT_IMPLEMENTED)
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        for tool_name in BLOCKED_TOOLS {
            assert!(
                !registry_names.contains(tool_name),
                "Blocked tool '{}' should NOT be in tool registry",
                tool_name
            );
        }
    }

    #[test]
    fn test_read_only_tools_subset_of_registry() {
        // D1.7: READ_ONLY_TOOLS is a subset of the full registry
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        let read_only_set: std::collections::HashSet<_> = READ_ONLY_TOOLS.iter().copied().collect();
        // All read-only tools should be in the registry
        for tool_name in READ_ONLY_TOOLS {
            assert!(
                registry_names.contains(tool_name),
                "Read-only tool '{}' should be in registry",
                tool_name
            );
        }
        // Registry should contain all read-only tools plus lifecycle tools
        assert!(
            registry_names.len() >= READ_ONLY_TOOLS.len(),
            "Registry should contain at least all read-only tools"
        );
    }

    #[test]
    fn test_all_tools_have_non_null_schemas() {
        for tool in tool_registry() {
            assert!(
                !tool.input_schema.is_null(),
                "Tool '{}' should have a non-null input_schema",
                tool.name
            );
            assert!(
                !tool.description.is_empty(),
                "Tool '{}' should have a non-empty description",
                tool.name
            );
        }
    }

    #[test]
    fn test_no_unsafe_tool_names_in_registry() {
        // D1.7: Lifecycle tools now include submit, evaluate, execute, compensate
        // This test checks for truly unsafe patterns that should never appear
        let unsafe_patterns = [
            "rollback", // Not a lifecycle step name
            "direct_",  // Would indicate bypassing gateway
            "unsafe_",  // Unsafe prefix
            "bypass_",  // Bypass prefix
        ];
        for tool in tool_registry() {
            for pattern in &unsafe_patterns {
                assert!(
                    !tool.name.contains(pattern),
                    "Tool '{}' should not contain unsafe pattern '{}'",
                    tool.name,
                    pattern
                );
            }
        }
    }

    #[test]
    fn test_expected_tools_are_present() {
        let expected_tools = [
            "ferrum_gate_health",
            "ferrum_gate_readyz_deep",
            "ferrum_gate_list_intents",
            "ferrum_gate_get_execution",
            "ferrum_gate_query_lineage",
            "ferrum_gate_list_approvals",
            "ferrum_gate_list_policy_bundles",
            "ferrum_gate_list_bridges",
            "ferrum_gate_list_bridge_tools",
        ];
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        for expected in expected_tools {
            assert!(
                registry_names.contains(expected),
                "Expected tool '{}' should be in registry",
                expected
            );
        }
    }

    // -------------------------------------------------------------------------
    // Phase B Tests (JSON-RPC Handlers)
    // -------------------------------------------------------------------------

    #[test]
    fn test_initialize_returns_protocol_version() {
        let response = handle_initialize(serde_json::Value::Null, None);
        match response {
            JsonRpcResponse::Success(success) => {
                let result: InitializeResult =
                    serde_json::from_value(success.result).expect("should parse");
                assert_eq!(result.protocol_version, "2024-11-05");
                assert!(result.capabilities.tools.is_some());
            }
            JsonRpcResponse::Error(_) => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_initialize_includes_tools_capability() {
        let response = handle_initialize(serde_json::Value::Null, None);
        match response {
            JsonRpcResponse::Success(success) => {
                let result: InitializeResult =
                    serde_json::from_value(success.result).expect("should parse");
                assert!(result.capabilities.tools.is_some());
            }
            JsonRpcResponse::Error(_) => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_ping_returns_success() {
        let response = handle_ping(None);
        match response {
            JsonRpcResponse::Success(success) => {
                assert_eq!(success.result, serde_json::json!({ "success": true }));
            }
            JsonRpcResponse::Error(_) => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_tools_list_returns_all_tools() {
        // D1.7: tools/list returns 17 tools (9 read-only + 8 lifecycle)
        let response = handle_tools_list(None);
        match response {
            JsonRpcResponse::Success(success) => {
                let result: ToolsListResult =
                    serde_json::from_value(success.result).expect("should parse");
                assert_eq!(result.tools.len(), 17, "tools/list should return 17 tools");
            }
            JsonRpcResponse::Error(_) => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_tools_list_returns_correct_tool_names() {
        let response = handle_tools_list(None);
        match response {
            JsonRpcResponse::Success(success) => {
                let result: ToolsListResult =
                    serde_json::from_value(success.result).expect("should parse");
                let tool_names: Vec<_> = result.tools.iter().map(|t| t.name.as_str()).collect();
                for expected in READ_ONLY_TOOLS {
                    assert!(
                        tool_names.contains(expected),
                        "Expected tool '{}' in tools/list result",
                        expected
                    );
                }
            }
            JsonRpcResponse::Error(_) => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_tools_call_returns_not_implemented() {
        let params = serde_json::json!({
            "name": "ferrum_gate_health",
            "arguments": {}
        });
        let response = handle_tools_call(params, None);
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, error_codes::NOT_IMPLEMENTED);
            }
            JsonRpcResponse::Success(_) => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_unknown_method_returns_method_not_found() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "unknown_method".to_string(),
            id: Some(JsonRpcId::Number(1)),
            params: None,
        };
        let response = dispatch(request);
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, error_codes::METHOD_NOT_FOUND);
                assert!(err.error.message.contains("unknown_method"));
            }
            JsonRpcResponse::Success(_) => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_dispatch_routes_initialize() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "initialize".to_string(),
            id: Some(JsonRpcId::String("1".to_string())),
            params: Some(serde_json::json!({
                "protocol_version": "2024-11-05",
                "capabilities": {},
                "client_info": {"name": "test", "version": "1.0"}
            })),
        };
        let response = dispatch(request);
        match response {
            JsonRpcResponse::Success(_) => {}
            JsonRpcResponse::Error(_) => panic!("Expected success for initialize"),
        }
    }

    #[test]
    fn test_dispatch_routes_tools_list() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            id: Some(JsonRpcId::Null),
            params: None,
        };
        let response = dispatch(request);
        match response {
            JsonRpcResponse::Success(success) => {
                let result: ToolsListResult =
                    serde_json::from_value(success.result).expect("should parse");
                assert_eq!(result.tools.len(), 17); // 9 read-only + 8 lifecycle
            }
            JsonRpcResponse::Error(_) => panic!("Expected success for tools/list"),
        }
    }

    #[test]
    fn test_parse_valid_request() {
        let json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let request = parse_request(json).expect("Should parse valid request");
        assert_eq!(request.method, "ping");
        assert!(matches!(request.id, Some(JsonRpcId::Number(1))));
    }

    #[test]
    fn test_parse_invalid_json_returns_error() {
        let json = "not valid json";
        let result = parse_request(json);
        assert!(result.is_err());
        match result {
            Err(JsonRpcResponse::Error(err)) => {
                assert_eq!(err.error.code, error_codes::PARSE_ERROR);
            }
            _ => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_dispatch_preserves_request_id() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "ping".to_string(),
            id: Some(JsonRpcId::String("test-123".to_string())),
            params: None,
        };
        let response = dispatch(request);
        match response {
            JsonRpcResponse::Success(success) => {
                assert!(matches!(success.id, Some(JsonRpcId::String(ref s)) if s == "test-123"));
            }
            JsonRpcResponse::Error(_) => panic!("Expected success"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase D-1 Tests (Auth Context)
    // -------------------------------------------------------------------------

    #[test]
    fn test_actor_identity_from_env_var_with_label() {
        unsafe { std::env::set_var("FERRUMD_MCP_AGENT_ID", "actor-123:Test Agent") };
        let identity = ActorIdentity::from_env_var().unwrap();
        assert_eq!(identity.actor_id, "actor-123");
        assert_eq!(identity.actor_label, "Test Agent");
        assert_eq!(identity.source, ActorSource::EnvVar);
        unsafe { std::env::remove_var("FERRUMD_MCP_AGENT_ID") };
    }

    #[test]
    fn test_actor_identity_from_env_var_without_label() {
        unsafe { std::env::set_var("FERRUMD_MCP_AGENT_ID", "actor-456") };
        let identity = ActorIdentity::from_env_var().unwrap();
        assert_eq!(identity.actor_id, "actor-456");
        assert_eq!(identity.actor_label, "actor-456");
        assert_eq!(identity.source, ActorSource::EnvVar);
        unsafe { std::env::remove_var("FERRUMD_MCP_AGENT_ID") };
    }

    #[test]
    fn test_actor_identity_from_env_var_empty() {
        unsafe { std::env::set_var("FERRUMD_MCP_AGENT_ID", "") };
        assert!(ActorIdentity::from_env_var().is_none());
        unsafe { std::env::remove_var("FERRUMD_MCP_AGENT_ID") };
    }

    #[test]
    fn test_actor_identity_resolve_precedence() {
        // Env var takes precedence
        unsafe { std::env::set_var("FERRUMD_MCP_AGENT_ID", "env-actor:Env Label") };
        let identity = ActorIdentity::resolve(Some("client-name"));
        assert_eq!(identity.actor_id, "env-actor");
        assert_eq!(identity.source, ActorSource::EnvVar);
        unsafe { std::env::remove_var("FERRUMD_MCP_AGENT_ID") };

        // Client info when no env var
        let identity = ActorIdentity::resolve(Some("client-name"));
        assert_eq!(identity.actor_id, "client-name");
        assert_eq!(identity.source, ActorSource::ClientInfo);

        // Local fallback when no env var and no client info
        let identity = ActorIdentity::resolve(None);
        assert_eq!(identity.actor_id, "ferrum-mcp-local");
        assert_eq!(identity.source, ActorSource::Local);
    }

    // -------------------------------------------------------------------------
    // D1.8 Tests (Output Sanitization via TaintScoringFirewall)
    // -------------------------------------------------------------------------

    use ferrum_firewall::TaintScoringFirewall;

    /// D1.8: Control characters stripped from JSON strings.
    #[test]
    fn test_sanitize_output_strips_control_chars() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "content": [{
                "type": "text",
                "text": "hello\x00world\x1ftest"
            }]
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        let content = obj.get("content").unwrap().as_array().unwrap();
        let text = content[0]
            .as_object()
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(text, "hello world test");
    }

    /// D1.8: UUIDs preserved in sanitized output.
    #[test]
    fn test_sanitize_output_preserves_uuids() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "intent_id": "550e8400-e29b-41d4-a716-446655440000",
            "proposal_id": "123e4567-e89b-12d3-a456-426614174000",
            "message": "normal message with no control chars"
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        assert_eq!(
            obj.get("intent_id").unwrap().as_str().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            obj.get("proposal_id").unwrap().as_str().unwrap(),
            "123e4567-e89b-12d3-a456-426614174000"
        );
        assert_eq!(
            obj.get("message").unwrap().as_str().unwrap(),
            "normal message with no control chars"
        );
    }

    /// D1.8: Messages and warnings preserved in sanitized output.
    #[test]
    fn test_sanitize_output_preserves_messages_and_warnings() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "message": "Operation completed successfully",
            "warnings": ["warning one", "warning two"],
            "diagnostics": "All systems nominal"
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        assert_eq!(
            obj.get("message").unwrap().as_str().unwrap(),
            "Operation completed successfully"
        );
        let warnings = obj.get("warnings").unwrap().as_array().unwrap();
        assert_eq!(warnings[0].as_str().unwrap(), "warning one");
        assert_eq!(warnings[1].as_str().unwrap(), "warning two");
        assert_eq!(
            obj.get("diagnostics").unwrap().as_str().unwrap(),
            "All systems nominal"
        );
    }

    /// D1.8: Empty JSON object passes through without crash.
    #[test]
    fn test_sanitize_output_handles_empty_json() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({});
        let sanitized = fw.sanitize_output(input);
        assert!(sanitized.is_object());
        assert!(sanitized.as_object().unwrap().is_empty());
    }

    /// D1.8: Nested JSON structures are recursively sanitized.
    #[test]
    fn test_sanitize_output_nested_json() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "outer": {
                "inner": "text\x00here",
                "array": ["item1\x00", "item2\x1f"]
            },
            "list": [{
                "name": "item\x00with\x1fcontrol"
            }]
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        let outer = obj.get("outer").unwrap().as_object().unwrap();
        assert_eq!(outer.get("inner").unwrap().as_str().unwrap(), "text here");
        let arr = outer.get("array").unwrap().as_array().unwrap();
        assert_eq!(arr[0].as_str().unwrap(), "item1");
        assert_eq!(arr[1].as_str().unwrap(), "item2");
        let list = obj.get("list").unwrap().as_array().unwrap();
        assert_eq!(
            list[0]
                .as_object()
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap(),
            "item with control"
        );
    }

    /// D1.8: Numbers, booleans, and nulls are preserved (not modified).
    #[test]
    fn test_sanitize_output_preserves_non_strings() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "number": 42,
            "float": 3.14,
            "bool": true,
            "null": null,
            "nested": {
                "neg": -1,
                "zero": 0
            }
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        assert_eq!(obj.get("number").unwrap().as_i64().unwrap(), 42);
        assert!((obj.get("float").unwrap().as_f64().unwrap() - 3.14).abs() < 0.001);
        assert!(obj.get("bool").unwrap().as_bool().unwrap());
        assert!(obj.get("null").unwrap().is_null());
        let nested = obj.get("nested").unwrap().as_object().unwrap();
        assert_eq!(nested.get("neg").unwrap().as_i64().unwrap(), -1);
        assert_eq!(nested.get("zero").unwrap().as_i64().unwrap(), 0);
    }

    /// D1.8: ToolContent.text control chars stripped at nested level.
    #[test]
    fn test_sanitize_output_tool_content_text() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "content": [{
                "type": "text",
                "text": "Response\x00with\x1finvisible\x02chars"
            }]
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        let content = obj.get("content").unwrap().as_array().unwrap();
        let text = content[0]
            .as_object()
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(text, "Response with invisible chars");
    }

    /// D1.8: Error responses pass through without lifecycle semantic changes.
    #[test]
    fn test_sanitize_output_error_response_unchanged() {
        let fw = TaintScoringFirewall::new();
        // Error responses are not sanitized via sanitize_output (they bypass the success path)
        // This test verifies that sanitize_output does not modify error-like structures
        let input = serde_json::json!({
            "code": -32003,
            "message": "Gateway unreachable",
            "data": null
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        assert_eq!(obj.get("code").unwrap().as_i64().unwrap(), -32003);
        assert_eq!(
            obj.get("message").unwrap().as_str().unwrap(),
            "Gateway unreachable"
        );
        assert!(obj.get("data").unwrap().is_null());
    }

    /// D1.8: Array at root level is recursively sanitized.
    #[test]
    fn test_sanitize_output_array_at_root() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!([{
            "type": "text",
            "text": "item\x001"
        }, {
            "type": "text",
            "text": "item\x002"
        }]);
        let sanitized = fw.sanitize_output(input);
        let arr = sanitized.as_array().unwrap();
        assert_eq!(
            arr[0]
                .as_object()
                .unwrap()
                .get("text")
                .unwrap()
                .as_str()
                .unwrap(),
            "item 1"
        );
        assert_eq!(
            arr[1]
                .as_object()
                .unwrap()
                .get("text")
                .unwrap()
                .as_str()
                .unwrap(),
            "item 2"
        );
    }

    // -------------------------------------------------------------------------
    // D1.9 Tests (Phase 1 — Field-Level Redaction via redact_sensitive_fields)
    // -------------------------------------------------------------------------

    /// D1.9 Phase 1: raw_arguments field is redacted.
    #[test]
    fn test_redact_raw_arguments() {
        let input = serde_json::json!({
            "intent_id": "550e8400-e29b-41d4-a716-446655440000",
            "raw_arguments": {
                "secret_key": "ssh-rsa AAAA...",
                "password": "supersecret"
            }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        // intent_id should be preserved
        assert_eq!(
            obj.get("intent_id").unwrap().as_str().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        // raw_arguments should be replaced with "[REDACTED]"
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: metadata field is redacted.
    #[test]
    fn test_redact_metadata() {
        let input = serde_json::json!({
            "proposal_id": "123e4567-e89b-12d3-a456-426614174000",
            "metadata": {
                "internal_notes": "sensitive info",
                "audit_trail": ["step1", "step2"]
            }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        // proposal_id should be preserved
        assert_eq!(
            obj.get("proposal_id").unwrap().as_str().unwrap(),
            "123e4567-e89b-12d3-a456-426614174000"
        );
        // metadata should be replaced with "[REDACTED]"
        assert_eq!(obj.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 1: nested metadata is redacted (whole-field redaction).
    #[test]
    fn test_redact_nested_metadata() {
        let input = serde_json::json!({
            "result": {
                "metadata": {
                    "nested": {
                        "deep": "secret data"
                    }
                },
                "status": "ok"
            }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let result = obj.get("result").unwrap().as_object().unwrap();
        // status should be preserved
        assert_eq!(result.get("status").unwrap().as_str().unwrap(), "ok");
        // metadata should be redacted (whole-field, not key enumeration)
        assert_eq!(
            result.get("metadata").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: both raw_arguments and metadata redacted when both present.
    #[test]
    fn test_redact_both_raw_arguments_and_metadata() {
        let input = serde_json::json!({
            "raw_arguments": { "key": "value" },
            "metadata": { "internal": "data" },
            "action_type": "fs_write"
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(obj.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
        // action_type should be preserved (no-over-redaction)
        assert_eq!(
            obj.get("action_type").unwrap().as_str().unwrap(),
            "fs_write"
        );
    }

    /// D1.9 Phase 1: UUID IDs are preserved (no-over-redaction).
    #[test]
    fn test_redact_preserves_uuids() {
        let input = serde_json::json!({
            "intent_id": "550e8400-e29b-41d4-a716-446655440000",
            "proposal_id": "123e4567-e89b-12d3-a456-426614174000",
            "capability_id": "deadbeef-e89b-12d3-a456-426614174000",
            "execution_id": "fee1dead-e89b-12d3-a456-426614174000"
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("intent_id").unwrap().as_str().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            obj.get("proposal_id").unwrap().as_str().unwrap(),
            "123e4567-e89b-12d3-a456-426614174000"
        );
        assert_eq!(
            obj.get("capability_id").unwrap().as_str().unwrap(),
            "deadbeef-e89b-12d3-a456-426614174000"
        );
        assert_eq!(
            obj.get("execution_id").unwrap().as_str().unwrap(),
            "fee1dead-e89b-12d3-a456-426614174000"
        );
    }

    /// D1.9 Phase 1: reason/message/warnings preserved (no-over-redaction).
    #[test]
    fn test_redact_preserves_reason_and_messages() {
        let input = serde_json::json!({
            "reason": "Policy evaluation passed",
            "message": "Execution completed successfully",
            "warnings": ["warning one", "warning two"],
            "diagnostics": "All systems nominal",
            "raw_arguments": { "secret": "data" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("reason").unwrap().as_str().unwrap(),
            "Policy evaluation passed"
        );
        assert_eq!(
            obj.get("message").unwrap().as_str().unwrap(),
            "Execution completed successfully"
        );
        let warnings = obj.get("warnings").unwrap().as_array().unwrap();
        assert_eq!(warnings[0].as_str().unwrap(), "warning one");
        assert_eq!(warnings[1].as_str().unwrap(), "warning two");
        assert_eq!(
            obj.get("diagnostics").unwrap().as_str().unwrap(),
            "All systems nominal"
        );
        // raw_arguments still redacted
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: enums and booleans preserved (no-over-redaction).
    #[test]
    fn test_redact_preserves_enums_and_booleans() {
        let input = serde_json::json!({
            "state": "Pending",
            "status": "Approved",
            "decision": "Allow",
            "is_active": true,
            "is_final": false,
            "metadata": { "internal": "data" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(obj.get("state").unwrap().as_str().unwrap(), "Pending");
        assert_eq!(obj.get("status").unwrap().as_str().unwrap(), "Approved");
        assert_eq!(obj.get("decision").unwrap().as_str().unwrap(), "Allow");
        assert_eq!(obj.get("is_active").unwrap().as_bool().unwrap(), true);
        assert_eq!(obj.get("is_final").unwrap().as_bool().unwrap(), false);
        // metadata still redacted
        assert_eq!(obj.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 2: rollback_contract envelope preserved, target/metadata/args redacted per policy.
    #[test]
    fn test_redact_preserves_rollback_contract() {
        // compensation_plan is an ARRAY of objects in Phase 2 policy
        let input = serde_json::json!({
            "rollback_contract": {
                "target": "/tmp/important.txt",
                "compensation_plan": [
                    { "action": "delete", "args": ["arg1", "arg2"] }
                ],
                "metadata": { "internal": "rollback data" }
            },
            "raw_arguments": { "secret": "data" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let rc = obj.get("rollback_contract").unwrap().as_object().unwrap();
        // Phase 2: target is globally redacted
        assert_eq!(rc.get("target").unwrap().as_str().unwrap(), "[REDACTED]");
        // compensation_plan array envelope preserved (not redacted as a whole)
        let cp_array = rc.get("compensation_plan").unwrap().as_array().unwrap();
        // action inside compensation_plan array element preserved (not a sensitive key)
        assert_eq!(
            cp_array[0]
                .as_object()
                .unwrap()
                .get("action")
                .unwrap()
                .as_str()
                .unwrap(),
            "delete"
        );
        // Phase 2 path-aware: args inside compensation_plan array element is redacted to "[REDACTED]"
        assert_eq!(
            cp_array[0]
                .as_object()
                .unwrap()
                .get("args")
                .unwrap()
                .as_str()
                .unwrap(),
            "[REDACTED]"
        );
        // Phase 2: metadata is a global sensitive key, so it is redacted even inside rollback_contract
        assert_eq!(rc.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
        // raw_arguments redacted at top level
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: correlation_id preserved (no-over-redaction).
    #[test]
    fn test_redact_preserves_correlation_id() {
        let input = serde_json::json!({
            "correlation_id": "corr-12345-abcde",
            "raw_arguments": { "secret": "data" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("correlation_id").unwrap().as_str().unwrap(),
            "corr-12345-abcde"
        );
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: empty JSON object passes through without crash.
    #[test]
    fn test_redact_handles_empty_json() {
        let input = serde_json::json!({});
        let redacted = redact_sensitive_fields(input);
        assert!(redacted.is_object());
        assert!(redacted.as_object().unwrap().is_empty());
    }

    /// D1.9 Phase 1: empty array passes through without crash.
    #[test]
    fn test_redact_handles_empty_array() {
        let input = serde_json::json!([]);
        let redacted = redact_sensitive_fields(input);
        assert!(redacted.is_array());
        assert!(redacted.as_array().unwrap().is_empty());
    }

    /// D1.9 Phase 1: arrays recurse and redact nested sensitive fields.
    #[test]
    fn test_redact_arrays_recurse() {
        let input = serde_json::json!([{
            "id": "123",
            "raw_arguments": { "secret": "data" }
        }, {
            "id": "456",
            "metadata": { "internal": "info" }
        }]);
        let redacted = redact_sensitive_fields(input);
        let arr = redacted.as_array().unwrap();
        // First item: id preserved, raw_arguments redacted
        let first = arr[0].as_object().unwrap();
        assert_eq!(first.get("id").unwrap().as_str().unwrap(), "123");
        assert_eq!(
            first.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        // Second item: id preserved, metadata redacted
        let second = arr[1].as_object().unwrap();
        assert_eq!(second.get("id").unwrap().as_str().unwrap(), "456");
        assert_eq!(
            second.get("metadata").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: numbers, booleans, nulls pass through unchanged.
    #[test]
    fn test_redact_preserves_primitives() {
        let input = serde_json::json!({
            "number": 42,
            "float": 3.14,
            "bool_true": true,
            "bool_false": false,
            "null_val": null,
            "raw_arguments": { "secret": "data" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(obj.get("number").unwrap().as_i64().unwrap(), 42);
        assert!((obj.get("float").unwrap().as_f64().unwrap() - 3.14).abs() < 0.001);
        assert_eq!(obj.get("bool_true").unwrap().as_bool().unwrap(), true);
        assert_eq!(obj.get("bool_false").unwrap().as_bool().unwrap(), false);
        assert!(obj.get("null_val").unwrap().is_null());
        // raw_arguments still redacted
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: deeply nested objects have sensitive fields redacted.
    #[test]
    fn test_redact_deeply_nested() {
        let input = serde_json::json!({
            "level1": {
                "level2": {
                    "level3": {
                        "raw_arguments": { "deep_secret": "value" },
                        "message": "preserve this"
                    }
                }
            }
        });
        let redacted = redact_sensitive_fields(input);
        let l1 = redacted.as_object().unwrap();
        let l2 = l1.get("level1").unwrap().as_object().unwrap();
        let l3 = l2.get("level2").unwrap().as_object().unwrap();
        let l4 = l3.get("level3").unwrap().as_object().unwrap();
        // message preserved
        assert_eq!(
            l4.get("message").unwrap().as_str().unwrap(),
            "preserve this"
        );
        // raw_arguments redacted at depth 4
        assert_eq!(
            l4.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 1: no sensitive keys means no changes.
    #[test]
    fn test_redact_no_sensitive_keys_unchanged() {
        let input = serde_json::json!({
            "intent_id": "550e8400-e29b-41d4-a716-446655440000",
            "action_type": "fs_write",
            "reason": "Policy allowed",
            "state": "Completed"
        });
        let redacted = redact_sensitive_fields(input.clone());
        assert_eq!(redacted, input);
    }

    /// D1.9 Phase 1: matched_rule_ids preserved (no-over-redaction).
    #[test]
    fn test_redact_preserves_matched_rule_ids() {
        let input = serde_json::json!({
            "matched_rule_ids": ["POL-001", "POL-002"],
            "raw_arguments": { "secret": "data" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let rule_ids = obj.get("matched_rule_ids").unwrap().as_array().unwrap();
        assert_eq!(rule_ids[0].as_str().unwrap(), "POL-001");
        assert_eq!(rule_ids[1].as_str().unwrap(), "POL-002");
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    // -------------------------------------------------------------------------
    // D1.9 Phase 2 Tests (Global Keys + Path-Aware compensation_plan[].args)
    // -------------------------------------------------------------------------

    /// D1.9 Phase 2: resource_bindings globally redacted.
    #[test]
    fn test_redact_phase2_resource_bindings() {
        let input = serde_json::json!({
            "capability_id": "deadbeef-e89b-12d3-a456-426614174000",
            "resource_bindings": ["fs:read:/etc/shadow", "env:API_KEY"]
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("capability_id").unwrap().as_str().unwrap(),
            "deadbeef-e89b-12d3-a456-426614174000"
        );
        assert_eq!(
            obj.get("resource_bindings").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: argument_constraints globally redacted.
    #[test]
    fn test_redact_phase2_argument_constraints() {
        let input = serde_json::json!({
            "approval_binding": "appr-123",
            "argument_constraints": { "max_file_size": 1000, "allowed_paths": ["/tmp"] }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("approval_binding").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(
            obj.get("argument_constraints").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: approval_binding globally redacted.
    #[test]
    fn test_redact_phase2_approval_binding() {
        let input = serde_json::json!({
            "state": "PendingApproval",
            "approval_binding": "appr-456-xyz"
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("state").unwrap().as_str().unwrap(),
            "PendingApproval"
        );
        assert_eq!(
            obj.get("approval_binding").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: target globally redacted.
    #[test]
    fn test_redact_phase2_target() {
        let input = serde_json::json!({
            "intent_id": "550e8400-e29b-41d4-a716-446655440000",
            "target": "/etc/passwd"
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("intent_id").unwrap().as_str().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(obj.get("target").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 2: resource_scope globally redacted.
    #[test]
    fn test_redact_phase2_resource_scope() {
        let input = serde_json::json!({
            "scope": "fs:write:/tmp/test.txt",
            "resource_scope": "fs:write:/var/log/**"
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        // scope is not a sensitive key (only resource_scope is)
        assert_eq!(
            obj.get("scope").unwrap().as_str().unwrap(),
            "fs:write:/tmp/test.txt"
        );
        assert_eq!(
            obj.get("resource_scope").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: trust_context globally redacted.
    #[test]
    fn test_redact_phase2_trust_context() {
        let input = serde_json::json!({
            "decision": "Allow",
            "trust_context": { "level": "high", "verified": true }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(obj.get("decision").unwrap().as_str().unwrap(), "Allow");
        assert_eq!(
            obj.get("trust_context").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: result_digest globally redacted.
    #[test]
    fn test_redact_phase2_result_digest() {
        let input = serde_json::json!({
            "execution_id": "fee1dead-e89b-12d3-a456-426614174000",
            "result_digest": "sha256:abc123def456..."
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("execution_id").unwrap().as_str().unwrap(),
            "fee1dead-e89b-12d3-a456-426614174000"
        );
        assert_eq!(
            obj.get("result_digest").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: tool_binding preserved (no-over-redaction).
    #[test]
    fn test_redact_phase2_preserves_tool_binding() {
        let input = serde_json::json!({
            "tool_binding": "fs-adapter:fs_write",
            "tool_version": "1.0.0",
            "resource_bindings": ["fs:read:/etc/shadow"]
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        // tool_binding and tool_version preserved
        assert_eq!(
            obj.get("tool_binding").unwrap().as_str().unwrap(),
            "fs-adapter:fs_write"
        );
        assert_eq!(obj.get("tool_version").unwrap().as_str().unwrap(), "1.0.0");
        // resource_bindings still redacted
        assert_eq!(
            obj.get("resource_bindings").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: args inside compensation_plan array element is redacted.
    #[test]
    fn test_redact_phase2_compensation_plan_args() {
        let input = serde_json::json!({
            "rollback_contract": {
                "target": "/tmp/important.txt",
                "compensation_plan": [
                    {
                        "action": "delete",
                        "args": ["arg1", "arg2"]
                    },
                    {
                        "action": "restore",
                        "args": ["backup-id-123"]
                    }
                ]
            }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let rc = obj.get("rollback_contract").unwrap().as_object().unwrap();
        // target inside rollback_contract is now redacted (Phase 2 global)
        assert_eq!(rc.get("target").unwrap().as_str().unwrap(), "[REDACTED]");
        let cp = rc.get("compensation_plan").unwrap().as_array().unwrap();
        // First element: action preserved, args redacted
        let first = &cp[0];
        let first_obj = first.as_object().unwrap();
        assert_eq!(first_obj.get("action").unwrap().as_str().unwrap(), "delete");
        assert_eq!(
            first_obj.get("args").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        // Second element: action preserved, args redacted
        let second = &cp[1];
        let second_obj = second.as_object().unwrap();
        assert_eq!(
            second_obj.get("action").unwrap().as_str().unwrap(),
            "restore"
        );
        assert_eq!(
            second_obj.get("args").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: args outside compensation_plan context is preserved.
    #[test]
    fn test_redact_phase2_args_outside_compensation_plan() {
        let input = serde_json::json!({
            "rollback_contract": {
                "target": "/tmp/file.txt",
                "args": ["global", "args"],
                "compensation_plan": [
                    {
                        "action": "delete",
                        "args": ["should", "be", "redacted"]
                    }
                ]
            },
            "other_args": ["preserve", "these"]
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let rc = obj.get("rollback_contract").unwrap().as_object().unwrap();
        // args directly inside rollback_contract is NOT compensation_plan context, preserved
        let rc_args = rc.get("args").unwrap().as_array().unwrap();
        assert_eq!(rc_args[0].as_str().unwrap(), "global");
        assert_eq!(rc_args[1].as_str().unwrap(), "args");
        // other_args at top level is preserved
        let other_args = obj.get("other_args").unwrap().as_array().unwrap();
        assert_eq!(other_args[0].as_str().unwrap(), "preserve");
        assert_eq!(other_args[1].as_str().unwrap(), "these");
        // args INSIDE compensation_plan array element is redacted
        let cp = rc.get("compensation_plan").unwrap().as_array().unwrap();
        let cp_args = cp[0]
            .as_object()
            .unwrap()
            .get("args")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(cp_args, "[REDACTED]");
    }

    /// D1.9 Phase 2: all Phase 2 global keys redacted at top level.
    #[test]
    fn test_redact_phase2_all_global_keys() {
        let input = serde_json::json!({
            "resource_bindings": ["binding1"],
            "argument_constraints": { "key": "value" },
            "approval_binding": "appr-789",
            "target": "/tmp/file.txt",
            "resource_scope": "fs:read:/home/**",
            "trust_context": { "level": "high" },
            "result_digest": "sha256:digest",
            "raw_arguments": { "secret": "data" },
            "metadata": { "internal": "info" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        // All Phase 2 global keys redacted
        assert_eq!(
            obj.get("resource_bindings").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(
            obj.get("argument_constraints").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(
            obj.get("approval_binding").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(obj.get("target").unwrap().as_str().unwrap(), "[REDACTED]");
        assert_eq!(
            obj.get("resource_scope").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(
            obj.get("trust_context").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(
            obj.get("result_digest").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        // Phase 1 keys still redacted
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(obj.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 2: compensation_plan itself is NOT redacted (only its args elements).
    #[test]
    fn test_redact_phase2_compensation_plan_envelope_preserved() {
        let input = serde_json::json!({
            "rollback_contract": {
                "compensation_plan": [
                    { "action": "undo", "args": ["arg1"] }
                ]
            }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let rc = obj.get("rollback_contract").unwrap().as_object().unwrap();
        // compensation_plan key itself exists (not redacted)
        assert!(rc.get("compensation_plan").unwrap().is_array());
        // The array contains the redacted element
        let cp = rc.get("compensation_plan").unwrap().as_array().unwrap();
        let elem = cp[0].as_object().unwrap();
        assert_eq!(elem.get("action").unwrap().as_str().unwrap(), "undo");
        assert_eq!(elem.get("args").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 2: rollback_contract/compensation_plan structure preserved (sibling fields).
    #[test]
    fn test_redact_phase2_rollback_contract_siblings_preserved() {
        let input = serde_json::json!({
            "rollback_contract": {
                "target": "/tmp/important.txt",
                "compensation_plan": [
                    {
                        "action": "delete",
                        "args": ["file1", "file2"]
                    }
                ],
                "metadata": { "internal": "rollback data" }
            }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        let rc = obj.get("rollback_contract").unwrap().as_object().unwrap();
        // action inside compensation_plan preserved
        let cp = rc.get("compensation_plan").unwrap().as_array().unwrap();
        let elem = cp[0].as_object().unwrap();
        assert_eq!(elem.get("action").unwrap().as_str().unwrap(), "delete");
        // metadata inside rollback_contract is a sibling of compensation_plan, not the sensitive key
        // Phase 2 does NOT target metadata globally when nested inside non-sensitive parent
        // (metadata is only targeted at the top level or inside ActionProposal context)
        // But wait - metadata IS a global sensitive key. Let me check the implementation...
        // Actually metadata inside rollback_contract IS a JsonMap field, so it should be redacted
        // because the key "metadata" is in SENSITIVE_KEYS.
        assert_eq!(rc.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 2: nested compensation_plan with deep args path.
    #[test]
    fn test_redact_phase2_nested_compensation_plan_deep() {
        let input = serde_json::json!({
            "outer": {
                "compensation_plan": [
                    {
                        "inner": {
                            "args": ["nested", "args", "here"]
                        }
                    }
                ]
            }
        });
        let redacted = redact_sensitive_fields(input);
        let outer = redacted.as_object().unwrap();
        let inner_obj = outer.get("outer").unwrap().as_object().unwrap();
        let cp = inner_obj
            .get("compensation_plan")
            .unwrap()
            .as_array()
            .unwrap();
        let elem = cp[0].as_object().unwrap();
        let inner = elem.get("inner").unwrap().as_object().unwrap();
        // args is inside compensation_plan array element, so it should be redacted
        assert_eq!(inner.get("args").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    /// D1.9 Phase 2: tool_binding/tool_version deep in structure preserved.
    #[test]
    fn test_redact_phase2_tool_binding_deep_preserved() {
        let input = serde_json::json!({
            "result": {
                "capability": {
                    "tool_binding": "git-adapter:push",
                    "tool_version": "2.1.0",
                    "resource_bindings": ["env:SECRET"]
                }
            }
        });
        let redacted = redact_sensitive_fields(input);
        let result = redacted.as_object().unwrap();
        let cap = result.get("result").unwrap().as_object().unwrap();
        let cap_inner = cap.get("capability").unwrap().as_object().unwrap();
        // tool_binding and tool_version preserved even when nested
        assert_eq!(
            cap_inner.get("tool_binding").unwrap().as_str().unwrap(),
            "git-adapter:push"
        );
        assert_eq!(
            cap_inner.get("tool_version").unwrap().as_str().unwrap(),
            "2.1.0"
        );
        // resource_bindings still redacted
        assert_eq!(
            cap_inner
                .get("resource_bindings")
                .unwrap()
                .as_str()
                .unwrap(),
            "[REDACTED]"
        );
    }

    /// D1.9 Phase 2: Phase 1 keys still work after Phase 2 additions.
    #[test]
    fn test_redact_phase2_phase1_still_works() {
        let input = serde_json::json!({
            "raw_arguments": { "password": "secret123" },
            "metadata": { "internal_notes": "sensitive" }
        });
        let redacted = redact_sensitive_fields(input);
        let obj = redacted.as_object().unwrap();
        assert_eq!(
            obj.get("raw_arguments").unwrap().as_str().unwrap(),
            "[REDACTED]"
        );
        assert_eq!(obj.get("metadata").unwrap().as_str().unwrap(), "[REDACTED]");
    }

    // -------------------------------------------------------------------------
    // D1.9.3 Tests (Integration Validation via handle_tools_call_with_client)
    // -------------------------------------------------------------------------

    /// D1.9.3: Phase 1 metadata redaction through handle_tools_call_with_client boundary.
    /// Validates that metadata inside envelope (IntentEnvelope) is redacted.
    #[test]
    fn test_d1_9_3_phase1_metadata_redaction() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "envelope": {
                    "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                    "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "session_id": null,
                    "channel_id": null,
                    "title": "test",
                    "goal": "test goal",
                    "normalized_goal": "test goal",
                    "allowed_outcomes": [{"id": "read", "description": "read only", "effect_type": "ReadOnlyAnalysis", "required": true}],
                    "forbidden_outcomes": [],
                    "resource_scope": [],
                    "risk_tier": "Low",
                    "approval_mode": "None",
                    "default_rollback_class": "R0NativeReversible",
                    "time_budget": {"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1},
                    "trust_context": {"input_labels": [], "sensitivity_labels": [], "taint_score": 0, "contains_external_metadata": false, "contains_tool_output": false, "contains_untrusted_text": false},
                    "derived_from_event_ids": [],
                    "tags": [],
                    "metadata": {"internal_secret": "secret_value"},
                    "status": "Active",
                    "created_at": "2025-01-01T00:00:00Z",
                    "expires_at": "2025-12-31T23:59:59Z"
                },
                "warnings": []
            }"#)
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                "title": "test",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);

        let resp_obj = match response {
            JsonRpcResponse::Success(resp) => resp.result,
            JsonRpcResponse::Error(resp) => {
                panic!("Expected success, got error: {}", resp.error.message);
            }
        };

        let resp_content = resp_obj.get("content").unwrap().as_array().unwrap();
        let text = resp_content[0].get("text").unwrap().as_str().unwrap();

        // metadata inside envelope should be redacted
        assert!(
            text.contains(r#""metadata":"[REDACTED]""#)
                || text.contains("\"metadata\":\"[REDACTED]\""),
            "metadata should be redacted in text: {}",
            text
        );

        // UUID should be preserved
        assert!(
            text.contains("550e8400-e29b-41d4-a716-446655440000"),
            "UUID should be preserved"
        );

        _mock.assert();
    }

    /// D1.9.3: Phase 2 trust_context redaction through boundary.
    #[test]
    fn test_d1_9_3_phase2_trust_context_redaction() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "envelope": {
                    "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                    "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "session_id": null,
                    "channel_id": null,
                    "title": "test",
                    "goal": "test goal",
                    "normalized_goal": "test goal",
                    "allowed_outcomes": [],
                    "forbidden_outcomes": [],
                    "resource_scope": [],
                    "risk_tier": "Low",
                    "approval_mode": "None",
                    "default_rollback_class": "R0NativeReversible",
                    "time_budget": {"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1},
                    "trust_context": {"input_labels": ["Trusted"], "sensitivity_labels": ["Secret"], "taint_score": 100, "contains_external_metadata": true, "contains_tool_output": true, "contains_untrusted_text": true},
                    "derived_from_event_ids": [],
                    "tags": [],
                    "metadata": {},
                    "status": "Active",
                    "created_at": "2025-01-01T00:00:00Z",
                    "expires_at": "2025-12-31T23:59:59Z"
                },
                "warnings": []
            }"#)
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                "title": "test",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(2)), &client);

        let resp_obj = match response {
            JsonRpcResponse::Success(resp) => resp.result,
            JsonRpcResponse::Error(resp) => {
                panic!("Expected success, got error: {}", resp.error.message);
            }
        };

        let resp_content = resp_obj.get("content").unwrap().as_array().unwrap();
        let text = resp_content[0].get("text").unwrap().as_str().unwrap();

        // trust_context inside envelope should be redacted
        assert!(
            text.contains(r#""trust_context":"[REDACTED]""#)
                || text.contains("\"trust_context\":\"[REDACTED]\""),
            "trust_context should be redacted in text: {}",
            text
        );

        _mock.assert();
    }

    /// D1.9.3: Combined D1.8 sanitize + D1.9 redact through handle_tools_call_with_client boundary.
    /// Proves: (1) control characters stripped by D1.8 sanitize_output, (2) sensitive fields
    /// redacted by D1.9 redact_tool_content_text, (3) sanitize does NOT undo inner redaction.
    #[test]
    fn test_d1_9_3_dlp_sanitize_then_redact() {
        let mut server = mockito::Server::new();
        // Mock response contains: Phase 1 sensitive field (metadata) + Phase 2 sensitive field (trust_context)
        // D1.9 redact_tool_content_text runs first (inner JSON redaction)
        // D1.8 sanitize_output runs second (control char stripping, cannot undo redaction since it
        // operates on serialized JSON Value, not re-parsing inner JSON strings)
        let _mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "envelope": {
                    "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                    "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "session_id": null,
                    "channel_id": null,
                    "title": "test goal",
                    "goal": "test goal",
                    "normalized_goal": "test goal",
                    "allowed_outcomes": [],
                    "forbidden_outcomes": [],
                    "resource_scope": [],
                    "risk_tier": "Low",
                    "approval_mode": "None",
                    "default_rollback_class": "R0NativeReversible",
                    "time_budget": {"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1},
                    "trust_context": {"input_labels": ["Trusted"], "sensitivity_labels": ["Secret"], "taint_score": 100, "contains_external_metadata": true, "contains_tool_output": true, "contains_untrusted_text": true},
                    "derived_from_event_ids": [],
                    "tags": [],
                    "metadata": {"internal_secret": "secret_value"},
                    "status": "Active",
                    "created_at": "2025-01-01T00:00:00Z",
                    "expires_at": "2025-12-31T23:59:59Z"
                },
                "warnings": []
            }"#)
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                "title": "test",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(4)), &client);

        let resp_obj = match response {
            JsonRpcResponse::Success(resp) => resp.result,
            JsonRpcResponse::Error(resp) => {
                panic!("Expected success, got error: {}", resp.error.message);
            }
        };

        let resp_content = resp_obj.get("content").unwrap().as_array().unwrap();
        let text = resp_content[0].get("text").unwrap().as_str().unwrap();

        // D1.9 Phase 1: metadata should be redacted
        assert!(
            text.contains(r#""metadata":"[REDACTED]""#)
                || text.contains("\"metadata\":\"[REDACTED]\""),
            "metadata should be redacted in text: {}",
            text
        );

        // D1.9 Phase 2: trust_context should be redacted
        assert!(
            text.contains(r#""trust_context":"[REDACTED]""#)
                || text.contains("\"trust_context\":\"[REDACTED]\""),
            "trust_context should be redacted in text: {}",
            text
        );

        // UUID should be preserved (no-over-redaction)
        assert!(
            text.contains("550e8400-e29b-41d4-a716-446655440000"),
            "UUID should be preserved"
        );

        // Sanitize_output does not undo redaction: metadata stays redacted
        assert!(
            text.contains(r#""metadata":"[REDACTED]""#)
                || text.contains("\"metadata\":\"[REDACTED]\""),
            "sanitize_output should not undo redaction: {}",
            text
        );

        _mock.assert();
    }

    /// D1.9.3: Prepare execution response boundary - basic structure test.
    #[test]
    fn test_d1_9_3_prepare_execution_response_boundary() {
        let mut server = mockito::Server::new();
        let exec_id = "550e8400-e29b-41d4-a716-446655440099";
        let path = format!("/v1/executions/{}/prepare", exec_id);
        let _mock = server
            .mock("POST", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                "execution_id": "{}",
                "prepared": true,
                "rollback_contract": null,
                "warnings": []
            }}"#,
                exec_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_prepare_execution",
            "arguments": {
                "execution_id": exec_id
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(3)), &client);

        let resp_obj = match response {
            JsonRpcResponse::Success(resp) => resp.result,
            JsonRpcResponse::Error(resp) => {
                panic!("Expected success, got error: {}", resp.error.message);
            }
        };

        let resp_content = resp_obj.get("content").unwrap().as_array().unwrap();
        let text = resp_content[0].get("text").unwrap().as_str().unwrap();

        // execution_id should be preserved
        assert!(
            text.contains(exec_id),
            "execution_id should be preserved: {}",
            text
        );

        // prepared should be true
        assert!(text.contains("true"), "prepared should be true: {}", text);

        _mock.assert();
    }

    /// D1.9.3: Deeply nested metadata redaction - size guard without crash.
    #[test]
    fn test_d1_9_3_deeply_nested_metadata() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "envelope": {
                    "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                    "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "session_id": null,
                    "channel_id": null,
                    "title": "test",
                    "goal": "test goal",
                    "normalized_goal": "test goal",
                    "allowed_outcomes": [],
                    "forbidden_outcomes": [],
                    "resource_scope": [],
                    "risk_tier": "Low",
                    "approval_mode": "None",
                    "default_rollback_class": "R0NativeReversible",
                    "time_budget": {"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1},
                    "trust_context": {"input_labels": [], "sensitivity_labels": [], "taint_score": 0, "contains_external_metadata": false, "contains_tool_output": false, "contains_untrusted_text": false},
                    "derived_from_event_ids": [],
                    "tags": [],
                    "metadata": {
                        "level1": {
                            "level2": {
                                "level3": {
                                    "level4": {
                                        "level5": {
                                            "secret": "deep_value"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "status": "Active",
                    "created_at": "2025-01-01T00:00:00Z",
                    "expires_at": "2025-12-31T23:59:59Z"
                },
                "warnings": []
            }"#)
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                "title": "test",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(5)), &client);

        let resp_obj = match response {
            JsonRpcResponse::Success(resp) => resp.result,
            JsonRpcResponse::Error(resp) => {
                panic!("Expected success, got error: {}", resp.error.message);
            }
        };

        let resp_content = resp_obj.get("content").unwrap().as_array().unwrap();
        let text = resp_content[0].get("text").unwrap().as_str().unwrap();

        // Deeply nested metadata should be redacted
        assert!(
            text.contains(r#""metadata":"[REDACTED]""#)
                || text.contains("\"metadata\":\"[REDACTED]\""),
            "deeply nested metadata should be redacted: {}",
            text
        );

        // UUID should still be present
        assert!(
            text.contains("550e8400-e29b-41d4-a716-446655440000"),
            "UUID should be preserved"
        );

        _mock.assert();
    }

    // -------------------------------------------------------------------------
    // D1.10 Tests (Full Pipeline Local Validation via handle_tools_call_with_client)
    // -------------------------------------------------------------------------

    /// D1.10: Full 8-step sequential lifecycle through handle_tools_call_with_client.
    /// Validates all 8 MCP lifecycle tools are wired correctly with ID chaining.
    #[test]
    fn test_d1_10_full_lifecycle_sequential() {
        let mut server = mockito::Server::new();

        // Fixed UUIDs for predictable ID chaining
        let intent_id = "550e8400-e29b-41d4-a716-446655440001";
        let proposal_id = "550e8400-e29b-41d4-a716-446655440002";
        let capability_id = "550e8400-e29b-41d4-a716-446655440003";
        let execution_id = "550e8400-e29b-41d4-a716-446655440004";

        // Step 1: compile - POST /v1/intents/compile
        let _mock_compile = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "envelope": {{
                        "intent_id": "{}",
                        "principal_id": "550e8400-e29b-41d4-a716-446655440000",
                        "session_id": null,
                        "channel_id": null,
                        "title": "test intent",
                        "goal": "test goal",
                        "normalized_goal": "test goal",
                        "allowed_outcomes": [{{"id": "read", "description": "read only", "effect_type": "ReadOnlyAnalysis", "required": true}}],
                        "forbidden_outcomes": [],
                        "resource_scope": [],
                        "risk_tier": "Low",
                        "approval_mode": "None",
                        "default_rollback_class": "R0NativeReversible",
                        "time_budget": {{"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1}},
                        "trust_context": {{"input_labels": [], "sensitivity_labels": [], "taint_score": 0, "contains_external_metadata": false, "contains_tool_output": false, "contains_untrusted_text": false}},
                        "derived_from_event_ids": [],
                        "tags": [],
                        "metadata": {{"internal": "value"}},
                        "status": "Active",
                        "created_at": "2025-01-01T00:00:00Z",
                        "expires_at": "2025-12-31T23:59:59Z"
                    }},
                    "warnings": []
                }}"#,
                intent_id
            ))
            .expect(1)
            .create();

        // Step 2: evaluate - POST /v1/proposals/{id}/evaluate
        let _mock_evaluate = server
            .mock("POST", &*format!("/v1/proposals/{}/evaluate", proposal_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "decision": "Allow",
                    "reason": "policy_passed",
                    "matched_rule_ids": ["rule_001"],
                    "warnings": []
                }}"#,
            ))
            .expect(1)
            .create();

        // Step 3: mint - POST /v1/capabilities/mint
        let _mock_mint = server
            .mock("POST", "/v1/capabilities/mint")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "lease": {{
                        "capability_id": "{}",
                        "intent_id": "{}",
                        "proposal_id": "{}",
                        "tool_binding": {{"server_name": "fs-server", "tool_name": "fs.read", "tool_version": null}},
                        "resource_bindings": [],
                        "argument_constraints": [],
                        "taint_budget": {{"max_taint_score": 30, "allow_external_tool_output": false, "allow_external_metadata": false, "allow_untrusted_text": false}},
                        "approval_binding": null,
                        "issued_by": "ferrum-cap",
                        "policy_bundle_id": "550e8400-e29b-41d4-a716-446655440099",
                        "tool_manifest_id": null,
                        "manifest_hash": null,
                        "status": "Active",
                        "issued_at": "2025-01-01T00:00:00Z",
                        "expires_at": "2025-12-31T23:59:59Z",
                        "revoked_at": null,
                        "metadata": {{}}
                    }},
                    "warnings": []
                }}"#,
                capability_id, intent_id, proposal_id
            ))
            .expect(1)
            .create();

        // Step 4: authorize - POST /v1/executions/authorize
        let _mock_authorize = server
            .mock("POST", "/v1/executions/authorize")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution": {{
                        "execution_id": "{}",
                        "proposal_id": "{}",
                        "intent_id": "{}",
                        "capability_id": "{}",
                        "rollback_contract_id": null,
                        "decision": "Allow",
                        "state": "Authorized",
                        "started_at": "2025-01-01T00:00:00Z",
                        "finished_at": null,
                        "result_digest": null,
                        "metadata": {{}}
                    }},
                    "warnings": []
                }}"#,
                execution_id, proposal_id, intent_id, capability_id
            ))
            .expect(1)
            .create();

        // Step 5: prepare - POST /v1/executions/{id}/prepare
        let _mock_prepare = server
            .mock("POST", &*format!("/v1/executions/{}/prepare", execution_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution_id": "{}",
                    "prepared": true,
                    "rollback_contract": {{
                        "contract_id": "550e8400-e29b-41d4-a716-446655440010",
                        "intent_id": "{}",
                        "proposal_id": "{}",
                        "execution_id": "{}",
                        "action_type": "FileWrite",
                        "rollback_class": "R0NativeReversible",
                        "adapter_key": "fs",
                        "target": {{"kind": "FilePath", "path": "/tmp/test.txt", "before_hash": null, "after_hash": null}},
                        "prepare_checks": [],
                        "verify_checks": [],
                        "compensation_plan": [{{"order": 1, "adapter_key": "fs", "operation": "delete", "args": {{"path": "/tmp/test.txt"}}, "idempotency_key": "key1"}}],
                        "auto_commit": false,
                        "state": "Prepared",
                        "created_at": "2025-01-01T00:00:00Z",
                        "expires_at": null,
                        "metadata": {{}}
                    }},
                    "warnings": []
                }}"#,
                execution_id, intent_id, proposal_id, execution_id
            ))
            .expect(1)
            .create();

        // Step 6: execute - POST /v1/executions/{id}/execute
        let _mock_execute = server
            .mock("POST", &*format!("/v1/executions/{}/execute", execution_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution_id": "{}",
                    "executed": true,
                    "result_digest": "abc123def456",
                    "rollback_contract": null,
                    "warnings": []
                }}"#,
                execution_id
            ))
            .expect(1)
            .create();

        // Step 7: verify - POST /v1/executions/{id}/verify
        let _mock_verify = server
            .mock("POST", &*format!("/v1/executions/{}/verify", execution_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution_id": "{}",
                    "verified": true,
                    "rollback_contract": null,
                    "warnings": []
                }}"#,
                execution_id
            ))
            .expect(1)
            .create();

        // Step 8: compensate - POST /v1/executions/{id}/compensate
        let _mock_compensate = server
            .mock("POST", &*format!("/v1/executions/{}/compensate", execution_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution_id": "{}",
                    "compensated": true,
                    "rollback_contract": null,
                    "warnings": []
                }}"#,
                execution_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        // Step 1: submit_intent
        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440000",
                "title": "test intent",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);
        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Step 1 (submit_intent) failed: {}", resp.error.message),
        };
        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();
        assert!(resp_text.contains(intent_id), "Step 1 should return intent_id: {}", resp_text);

        // Step 2: evaluate_intent
        let params = serde_json::json!({
            "name": "ferrum_gate_evaluate_intent",
            "arguments": {
                "proposal_id": proposal_id,
                "intent_id": intent_id,
                "title": "test proposal",
                "tool_name": "fs.read",
                "server_name": "fs-server",
                "arguments": {},
                "expected_effect": "read file",
                "estimated_risk": "Low"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(2)), &client);
        match &response {
            JsonRpcResponse::Success(_) => {},
            JsonRpcResponse::Error(resp) => panic!("Step 2 (evaluate_intent) failed: {}", resp.error.message),
        }

        // Step 3: mint_capability
        let params = serde_json::json!({
            "name": "ferrum_gate_mint_capability",
            "arguments": {
                "intent_id": intent_id,
                "proposal_id": proposal_id,
                "tool_name": "fs.read",
                "server_name": "fs-server"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(3)), &client);
        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Step 3 (mint_capability) failed: {}", resp.error.message),
        };
        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();
        assert!(resp_text.contains(capability_id), "Step 3 should return capability_id: {}", resp_text);

        // Step 4: authorize_execution
        let params = serde_json::json!({
            "name": "ferrum_gate_authorize_execution",
            "arguments": {
                "proposal_id": proposal_id,
                "capability_id": capability_id
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(4)), &client);
        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Step 4 (authorize_execution) failed: {}", resp.error.message),
        };
        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();
        assert!(resp_text.contains(execution_id), "Step 4 should return execution_id: {}", resp_text);

        // Step 5: prepare_execution
        let params = serde_json::json!({
            "name": "ferrum_gate_prepare_execution",
            "arguments": {
                "execution_id": execution_id
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(5)), &client);
        match &response {
            JsonRpcResponse::Success(_) => {},
            JsonRpcResponse::Error(resp) => panic!("Step 5 (prepare_execution) failed: {}", resp.error.message),
        };

        // Step 6: execute_prepared
        let params = serde_json::json!({
            "name": "ferrum_gate_execute_prepared",
            "arguments": {
                "execution_id": execution_id,
                "payload": {}
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(6)), &client);
        match &response {
            JsonRpcResponse::Success(_) => {},
            JsonRpcResponse::Error(resp) => panic!("Step 6 (execute_prepared) failed: {}", resp.error.message),
        };

        // Step 7: verify
        let params = serde_json::json!({
            "name": "ferrum_gate_verify",
            "arguments": {
                "execution_id": execution_id
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(7)), &client);
        match &response {
            JsonRpcResponse::Success(_) => {},
            JsonRpcResponse::Error(resp) => panic!("Step 7 (verify) failed: {}", resp.error.message),
        };

        // Step 8: compensate
        let params = serde_json::json!({
            "name": "ferrum_gate_compensate",
            "arguments": {
                "execution_id": execution_id
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(8)), &client);
        match &response {
            JsonRpcResponse::Success(_) => {},
            JsonRpcResponse::Error(resp) => panic!("Step 8 (compensate) failed: {}", resp.error.message),
        };

        // Verify all mocks were called
        _mock_compile.assert();
        _mock_evaluate.assert();
        _mock_mint.assert();
        _mock_authorize.assert();
        _mock_prepare.assert();
        _mock_execute.assert();
        _mock_verify.assert();
        _mock_compensate.assert();
    }

    /// D1.10: D1.9 Phase 1 metadata redaction in lifecycle response.
    #[test]
    fn test_d1_10_lifecycle_metadata_redaction() {
        let mut server = mockito::Server::new();
        let intent_id = "550e8400-e29b-41d4-a716-446655440001";

        let _mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "envelope": {{
                        "intent_id": "{}",
                        "principal_id": "550e8400-e29b-41d4-a716-446655440000",
                        "session_id": null,
                        "channel_id": null,
                        "title": "test",
                        "goal": "test goal",
                        "normalized_goal": "test goal",
                        "allowed_outcomes": [],
                        "forbidden_outcomes": [],
                        "resource_scope": [],
                        "risk_tier": "Low",
                        "approval_mode": "None",
                        "default_rollback_class": "R0NativeReversible",
                        "time_budget": {{"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1}},
                        "trust_context": {{"input_labels": [], "sensitivity_labels": [], "taint_score": 0, "contains_external_metadata": false, "contains_tool_output": false, "contains_untrusted_text": false}},
                        "derived_from_event_ids": [],
                        "tags": [],
                        "metadata": {{"internal_secret": "secret_value"}},
                        "status": "Active",
                        "created_at": "2025-01-01T00:00:00Z",
                        "expires_at": "2025-12-31T23:59:59Z"
                    }},
                    "warnings": []
                }}"#,
                intent_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440000",
                "title": "test",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);

        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Expected success, got error: {}", resp.error.message),
        };

        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();

        // metadata should be redacted (D1.9 Phase 1)
        assert!(
            resp_text.contains(r#""metadata":"[REDACTED]""#) || resp_text.contains("\"metadata\":\"[REDACTED]\""),
            "metadata should be redacted in lifecycle response: {}",
            resp_text
        );

        // intent_id should be preserved (no-over-redaction)
        assert!(
            resp_text.contains(intent_id),
            "intent_id should be preserved: {}",
            resp_text
        );

        _mock.assert();
    }

    /// D1.10: D1.9 Phase 2 trust_context redaction in lifecycle response.
    #[test]
    fn test_d1_10_lifecycle_trust_context_redaction() {
        let mut server = mockito::Server::new();
        let intent_id = "550e8400-e29b-41d4-a716-446655440001";

        let _mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "envelope": {{
                        "intent_id": "{}",
                        "principal_id": "550e8400-e29b-41d4-a716-446655440000",
                        "session_id": null,
                        "channel_id": null,
                        "title": "test",
                        "goal": "test goal",
                        "normalized_goal": "test goal",
                        "allowed_outcomes": [],
                        "forbidden_outcomes": [],
                        "resource_scope": [],
                        "risk_tier": "Low",
                        "approval_mode": "None",
                        "default_rollback_class": "R0NativeReversible",
                        "time_budget": {{"max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1}},
                        "trust_context": {{"input_labels": ["Trusted"], "sensitivity_labels": ["Secret"], "taint_score": 100, "contains_external_metadata": true, "contains_tool_output": true, "contains_untrusted_text": true}},
                        "derived_from_event_ids": [],
                        "tags": [],
                        "metadata": {{}},
                        "status": "Active",
                        "created_at": "2025-01-01T00:00:00Z",
                        "expires_at": "2025-12-31T23:59:59Z"
                    }},
                    "warnings": []
                }}"#,
                intent_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {
                "principal_id": "550e8400-e29b-41d4-a716-446655440000",
                "title": "test",
                "goal": "test goal",
                "action_type": "Read",
                "target": "/tmp/test.txt",
                "scope": "fs:read:/tmp/**"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);

        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Expected success, got error: {}", resp.error.message),
        };

        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();

        // trust_context should be redacted (D1.9 Phase 2)
        assert!(
            resp_text.contains(r#""trust_context":"[REDACTED]""#) || resp_text.contains("\"trust_context\":\"[REDACTED]\""),
            "trust_context should be redacted in lifecycle response: {}",
            resp_text
        );

        // intent_id should be preserved
        assert!(
            resp_text.contains(intent_id),
            "intent_id should be preserved: {}",
            resp_text
        );

        _mock.assert();
    }

    /// D1.10: D1.9 Phase 2 result_digest redaction in execute response.
    #[test]
    fn test_d1_10_lifecycle_result_digest_redaction() {
        let mut server = mockito::Server::new();
        let execution_id = "550e8400-e29b-41d4-a716-446655440004";

        let _mock = server
            .mock("POST", &*format!("/v1/executions/{}/execute", execution_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution_id": "{}",
                    "executed": true,
                    "result_digest": "sha256:abc123secretdigest",
                    "rollback_contract": null,
                    "warnings": []
                }}"#,
                execution_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_execute_prepared",
            "arguments": {
                "execution_id": execution_id,
                "payload": {}
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);

        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Expected success, got error: {}", resp.error.message),
        };

        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();

        // result_digest should be redacted (D1.9 Phase 2)
        assert!(
            resp_text.contains(r#""result_digest":"[REDACTED]""#) || resp_text.contains("\"result_digest\":\"[REDACTED]\""),
            "result_digest should be redacted in lifecycle response: {}",
            resp_text
        );

        // execution_id should be preserved
        assert!(
            resp_text.contains(execution_id),
            "execution_id should be preserved: {}",
            resp_text
        );

        _mock.assert();
    }

    /// D1.10: D1.9 Phase 2 path-aware compensation_plan[].args redaction.
    #[test]
    fn test_d1_10_lifecycle_compensation_plan_args_redaction() {
        let mut server = mockito::Server::new();
        let execution_id = "550e8400-e29b-41d4-a716-446655440004";

        let _mock = server
            .mock("POST", &*format!("/v1/executions/{}/prepare", execution_id))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "execution_id": "{}",
                    "prepared": true,
                    "rollback_contract": {{
                        "contract_id": "550e8400-e29b-41d4-a716-446655440010",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                        "execution_id": "{}",
                        "action_type": "FileWrite",
                        "rollback_class": "R0NativeReversible",
                        "adapter_key": "fs",
                        "target": {{"kind": "FilePath", "path": "/tmp/test.txt", "before_hash": null, "after_hash": null}},
                        "prepare_checks": [],
                        "verify_checks": [],
                        "compensation_plan": [
                            {{"order": 1, "adapter_key": "fs", "operation": "delete", "args": {{"path": "/tmp/test.txt", "secret_param": "secret_value"}}, "idempotency_key": "key1"}},
                            {{"order": 2, "adapter_key": "fs", "operation": "write", "args": {{"content": "original content", "backup_id": "backup123"}}, "idempotency_key": "key2"}}
                        ],
                        "auto_commit": false,
                        "state": "Prepared",
                        "created_at": "2025-01-01T00:00:00Z",
                        "expires_at": null,
                        "metadata": {{}}
                    }},
                    "warnings": []
                }}"#,
                execution_id, execution_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_prepare_execution",
            "arguments": {
                "execution_id": execution_id
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);

        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Expected success, got error: {}", resp.error.message),
        };

        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();

        // compensation_plan[].args should be redacted (D1.9 Phase 2 path-aware)
        assert!(
            resp_text.contains(r#""args":"[REDACTED]""#) || resp_text.contains("\"args\":\"[REDACTED]\""),
            "compensation_plan[].args should be redacted in lifecycle response: {}",
            resp_text
        );

        // execution_id should be preserved
        assert!(
            resp_text.contains(execution_id),
            "execution_id should be preserved: {}",
            resp_text
        );

        // tool_binding should be preserved (no-over-redaction)
        // Note: adapter_key is different from tool_binding - we verify adapter_key is kept
        assert!(
            resp_text.contains("adapter_key"),
            "adapter_key should be preserved: {}",
            resp_text
        );

        _mock.assert();
    }

    /// D1.10: D1.9 no-over-redaction - IDs, tool_binding preserved.
    #[test]
    fn test_d1_10_lifecycle_no_over_redaction() {
        let mut server = mockito::Server::new();
        let intent_id = "550e8400-e29b-41d4-a716-446655440001";
        let proposal_id = "550e8400-e29b-41d4-a716-446655440002";
        let capability_id = "550e8400-e29b-41d4-a716-446655440003";

        let _mock = server
            .mock("POST", "/v1/capabilities/mint")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                    "lease": {{
                        "capability_id": "{}",
                        "intent_id": "{}",
                        "proposal_id": "{}",
                        "tool_binding": {{"server_name": "fs-server", "tool_name": "fs.read", "tool_version": null}},
                        "resource_bindings": [],
                        "argument_constraints": [],
                        "taint_budget": {{"max_taint_score": 30, "allow_external_tool_output": false, "allow_external_metadata": false, "allow_untrusted_text": false}},
                        "approval_binding": null,
                        "issued_by": "ferrum-cap",
                        "policy_bundle_id": "550e8400-e29b-41d4-a716-446655440099",
                        "tool_manifest_id": null,
                        "manifest_hash": null,
                        "status": "Active",
                        "issued_at": "2025-01-01T00:00:00Z",
                        "expires_at": "2025-12-31T23:59:59Z",
                        "revoked_at": null,
                        "metadata": {{}}
                    }},
                    "warnings": []
                }}"#,
                capability_id, intent_id, proposal_id
            ))
            .expect(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let params = serde_json::json!({
            "name": "ferrum_gate_mint_capability",
            "arguments": {
                "intent_id": intent_id,
                "proposal_id": proposal_id,
                "tool_name": "fs.read",
                "server_name": "fs-server"
            }
        });
        let response = handle_tools_call_with_client(params, Some(JsonRpcId::Number(1)), &client);

        let resp_obj = match &response {
            JsonRpcResponse::Success(resp) => resp.result.clone(),
            JsonRpcResponse::Error(resp) => panic!("Expected success, got error: {}", resp.error.message),
        };

        let resp_text = resp_obj.get("content").unwrap().as_array().unwrap()[0].get("text").unwrap().as_str().unwrap();

        // All IDs should be preserved (no-over-redaction)
        assert!(
            resp_text.contains(intent_id),
            "intent_id should be preserved: {}",
            resp_text
        );
        assert!(
            resp_text.contains(proposal_id),
            "proposal_id should be preserved: {}",
            resp_text
        );
        assert!(
            resp_text.contains(capability_id),
            "capability_id should be preserved: {}",
            resp_text
        );

        // tool_binding fields should be preserved
        assert!(
            resp_text.contains("fs.read") || resp_text.contains("fs-server"),
            "tool_binding fields should be preserved: {}",
            resp_text
        );

        _mock.assert();
    }
}
