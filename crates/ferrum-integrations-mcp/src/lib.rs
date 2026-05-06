//! # ferrum-integrations-mcp
//!
//! FerrumGate MCP server integration crate (Phase A-C + Phase D-0 read-only REST client).
//!
//! ## Overview
//!
//! This crate provides:
//! - Read-only MCP tool schema definitions for FerrumGate
//! - Tool registry with metadata (name, description, input_schema, read_only marker)
//! - JSON-RPC 2.0 request/response types and error codes
//! - Handler stubs for initialize, ping, tools/list, tools/call
//! - Phase D-0: Read-only REST client for gateway integration
//!
//! ## Phase A-C Status (Complete)
//!
//! Phase A-C implemented:
//! - Read-only tool schema draft (9 tools)
//! - Tool registry proving no mutating tools are present
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
//! Phase D-0 does NOT implement:
//! - Auth middleware (bearer token validation)
//! - Policy evaluation
//! - Capability issuance
//! - Provenance emission
//! - Rollback preparation
//! - Mutating tool execution

use serde::{Deserialize, Serialize};

mod http_client;
mod rest_mapper;

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

/// Set of tool names that are mutating (require governance pipeline).
/// Per doc 74 D-1.2: these tools require intent → policy eval → capability mint → authorize flow.
/// In Stage 1 (D-1.1+D-1.2), these return NOT_IMPLEMENTED (default-deny).
pub const MUTATING_TOOLS: &[&str] = &[
    "ferrum_gate_submit_intent",
    "ferrum_gate_evaluate_intent",
    "ferrum_gate_prepare_execution",
    "ferrum_gate_execute_prepared",
    "ferrum_gate_compensate",
    "ferrum_gate_approve_intent",
    "ferrum_gate_reject_intent",
];

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
            .or_else(|| client_name.filter(|n| !n.is_empty()).map(Self::from_client_info))
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
        Ok(result) => {
            let value = serde_json::to_value(result).unwrap();
            JsonRpcResponse::success(value, id)
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
    fn test_tool_registry_contains_nine_tools() {
        let registry = tool_registry();
        assert_eq!(
            registry.len(),
            9,
            "Tool registry should contain exactly 9 tools"
        );
    }

    #[test]
    fn test_tool_registry_contains_only_read_only_tools() {
        for tool in tool_registry() {
            assert!(
                tool.read_only,
                "Tool '{}' should be marked as read_only=true",
                tool.name
            );
        }
    }

    #[test]
    fn test_mutating_tools_set_contains_expected_tools() {
        // Per doc 74 D-1.2, mutating tools are registered but return NOT_IMPLEMENTED
        let expected_mutating = [
            "ferrum_gate_submit_intent",
            "ferrum_gate_evaluate_intent",
            "ferrum_gate_prepare_execution",
            "ferrum_gate_execute_prepared",
            "ferrum_gate_compensate",
            "ferrum_gate_approve_intent",
            "ferrum_gate_reject_intent",
        ];
        let mutating_set: std::collections::HashSet<_> = MUTATING_TOOLS.iter().copied().collect();
        for expected in expected_mutating {
            assert!(
                mutating_set.contains(expected),
                "MUTATING_TOOLS should contain '{}'",
                expected
            );
        }
        assert_eq!(
            MUTATING_TOOLS.len(),
            7,
            "Should have exactly 7 mutating tools"
        );
    }

    #[test]
    fn test_mutating_tools_not_in_registry() {
        // Mutating tools are NOT in the read-only tool registry
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        for tool_name in MUTATING_TOOLS {
            assert!(
                !registry_names.contains(tool_name),
                "Mutating tool '{}' should NOT be in tool registry",
                tool_name
            );
        }
    }

    #[test]
    fn test_calling_mutating_tool_returns_not_implemented() {
        // Per doc 74 D-1.2, mutating tools return NOT_IMPLEMENTED (default-deny)
        let params = serde_json::json!({
            "name": "ferrum_gate_submit_intent",
            "arguments": {"intent": "test"}
        });
        let response = handle_tools_call(params, None);
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, error_codes::NOT_IMPLEMENTED);
            }
            JsonRpcResponse::Success(_) => {
                panic!("Expected NOT_IMPLEMENTED error for mutating tool")
            }
        }
    }

    #[test]
    fn test_read_only_tools_set_has_all_tools() {
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        let read_only_set: std::collections::HashSet<_> = READ_ONLY_TOOLS.iter().copied().collect();
        assert_eq!(
            registry_names, read_only_set,
            "READ_ONLY_TOOLS should match tool registry names"
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
    fn test_no_mutating_tool_names_in_registry() {
        let mutating_patterns = [
            "submit",
            "evaluate",
            "execute",
            "compensate",
            "rollback",
            "fs_write",
            "git_push",
            "sql_mutate",
            "http_post",
            "create",
            "update",
            "delete",
        ];
        for tool in tool_registry() {
            for pattern in &mutating_patterns {
                assert!(
                    !tool.name.contains(pattern),
                    "Tool '{}' should not contain mutating pattern '{}'",
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
    fn test_tools_list_returns_nine_tools() {
        let response = handle_tools_list(None);
        match response {
            JsonRpcResponse::Success(success) => {
                let result: ToolsListResult =
                    serde_json::from_value(success.result).expect("should parse");
                assert_eq!(result.tools.len(), 9, "tools/list should return 9 tools");
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
                assert_eq!(result.tools.len(), 9);
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
}
