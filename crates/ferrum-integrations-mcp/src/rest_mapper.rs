//! # REST Mapper for MCP Tools
//!
//! Maps MCP `tools/call` requests to FerrumGate REST API endpoints.
//!
//! ## Route Table
//!
//! | MCP Tool | REST Endpoint | Auth Required |
//! |----------|---------------|---------------|
//! | `ferrum_gate_health` | GET /v1/healthz | No |
//! | `ferrum_gate_readyz_deep` | GET /v1/readyz/deep | No |
//! | `ferrum_gate_list_intents` | GET /v1/intents | Yes |
//! | `ferrum_gate_get_execution` | GET /v1/executions/{id} | Yes |
//! | `ferrum_gate_query_lineage` | GET /v1/provenance/query | Yes |
//! | `ferrum_gate_list_approvals` | GET /v1/approvals | Yes |
//! | `ferrum_gate_list_policy_bundles` | GET /v1/policy-bundles | Yes |
//! | `ferrum_gate_list_bridges` | GET /v1/bridges | Yes |
//! | `ferrum_gate_list_bridge_tools` | GET /v1/bridges/{id}/tools | Yes |

use crate::MUTATING_TOOLS;
use crate::ToolsCallResult;
use crate::error_codes::{INVALID_PARAMS, METHOD_NOT_FOUND, NOT_IMPLEMENTED};
use crate::http_client::FerrumGatewayClient;
use crate::http_client::GatewayError;

// ---------------------------------------------------------------------------
// MCP Tool Error
// ---------------------------------------------------------------------------

/// Errors from MCP tool execution.
/// These are converted to JSON-RPC error responses.
#[derive(Debug, Clone)]
pub struct McpToolError {
    /// JSON-RPC error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Optional additional error data.
    pub data: Option<serde_json::Value>,
}

impl McpToolError {
    /// Create an error for a missing required argument.
    pub fn missing_arg(arg_name: &str) -> Self {
        Self {
            code: INVALID_PARAMS,
            message: format!("Missing required argument: {}", arg_name),
            data: None,
        }
    }

    /// Create an error from a gateway error.
    pub fn from_gateway_error(err: &GatewayError) -> Self {
        Self {
            code: err.code(),
            message: err.message().to_string(),
            data: None,
        }
    }

    /// Create an error for an unknown tool.
    pub fn unknown_tool(tool_name: &str) -> Self {
        Self {
            code: METHOD_NOT_FOUND,
            message: format!("Unknown tool: '{}'", tool_name),
            data: None,
        }
    }

    /// Create a not implemented error for mutating tools (default-deny).
    /// Per doc 74 D-1.2, mutating tools return NOT_IMPLEMENTED until governance pipeline is ready.
    pub fn not_implemented(tool_name: &str) -> Self {
        Self {
            code: NOT_IMPLEMENTED,
            message: format!(
                "Tool '{}' is mutating and not implemented in Stage 1",
                tool_name
            ),
            data: None,
        }
    }

    /// Create an error for invalid arguments.
    #[allow(dead_code)]
    pub fn invalid_args(msg: &str) -> Self {
        Self {
            code: INVALID_PARAMS,
            message: msg.to_string(),
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// REST Mapper
// ---------------------------------------------------------------------------

/// Maps MCP tool calls to FerrumGate REST API endpoints.
///
/// ## Arguments
///
/// Arguments are passed as a JSON object with tool-specific fields.
pub fn map_tool_to_rest(
    tool_name: &str,
    arguments: Option<&serde_json::Value>,
    client: &FerrumGatewayClient,
) -> Result<ToolsCallResult, McpToolError> {
    // Use empty object if no arguments provided
    let empty_args = serde_json::Value::Object(serde_json::Map::new());
    let args = arguments.unwrap_or(&empty_args);

    match tool_name {
        // Health probe - no auth required
        "ferrum_gate_health" => call_health(client),

        // Deep readiness probe - no auth required
        "ferrum_gate_readyz_deep" => call_readyz_deep(client),

        // Protected endpoints below
        "ferrum_gate_list_intents" => call_list_intents(client, args),
        "ferrum_gate_get_execution" => call_get_execution(client, args),
        "ferrum_gate_query_lineage" => call_query_lineage(client, args),
        "ferrum_gate_list_approvals" => call_list_approvals(client),
        "ferrum_gate_list_policy_bundles" => call_list_policy_bundles(client),
        "ferrum_gate_list_bridges" => call_list_bridges(client),
        "ferrum_gate_list_bridge_tools" => call_list_bridge_tools(client, args),

        // Mutating tools - return NOT_IMPLEMENTED (default-deny per doc 74 D-1.2)
        _ if MUTATING_TOOLS.contains(&tool_name) => Err(McpToolError::not_implemented(tool_name)),

        // Unknown tool
        _ => Err(McpToolError::unknown_tool(tool_name)),
    }
}

// ---------------------------------------------------------------------------
// Tool Call Functions
// ---------------------------------------------------------------------------

fn call_health(client: &FerrumGatewayClient) -> Result<ToolsCallResult, McpToolError> {
    match client.health() {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_readyz_deep(client: &FerrumGatewayClient) -> Result<ToolsCallResult, McpToolError> {
    match client.readyz_deep() {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_list_intents(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let intent_id = args.get("intent_id").and_then(|v| v.as_str());
    let state = args.get("state").and_then(|v| v.as_str());
    let cursor = args.get("cursor").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as u32);

    match client.list_intents(intent_id, state, cursor, limit) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_get_execution(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let execution_id = args
        .get("execution_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("execution_id"))?;

    match client.get_execution(execution_id) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_query_lineage(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let execution_id = args.get("execution_id").and_then(|v| v.as_str());
    let cursor = args.get("cursor").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as u32);

    match client.query_lineage(execution_id, cursor, limit) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_list_approvals(client: &FerrumGatewayClient) -> Result<ToolsCallResult, McpToolError> {
    match client.list_approvals() {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_list_policy_bundles(client: &FerrumGatewayClient) -> Result<ToolsCallResult, McpToolError> {
    match client.list_policy_bundles() {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_list_bridges(client: &FerrumGatewayClient) -> Result<ToolsCallResult, McpToolError> {
    match client.list_bridges() {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_list_bridge_tools(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let bridge_id = args
        .get("bridge_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("bridge_id"))?;

    match client.list_bridge_tools(bridge_id) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_error_missing_arg() {
        let err = McpToolError::missing_arg("execution_id");
        assert_eq!(err.code, -32602); // INVALID_PARAMS
        assert!(err.message.contains("execution_id"));
    }

    #[test]
    fn test_mcp_tool_error_unknown_tool() {
        let err = McpToolError::unknown_tool("unknown_tool");
        assert_eq!(err.code, -32601); // METHOD_NOT_FOUND
        assert!(err.message.contains("unknown_tool"));
    }

    #[test]
    fn test_mcp_tool_error_invalid_args() {
        let err = McpToolError::invalid_args("execution_id must be a string");
        assert_eq!(err.code, -32602); // INVALID_PARAMS
    }

    #[test]
    fn test_mcp_tool_error_from_gateway_auth() {
        let gateway_err = GatewayError::auth("Token invalid");
        let err = McpToolError::from_gateway_error(&gateway_err);
        assert_eq!(err.code, -32002); // AUTH_FAILED
        assert_eq!(err.message, "Token invalid");
    }

    #[test]
    fn test_mcp_tool_error_from_gateway_unreachable() {
        let gateway_err = GatewayError::unreachable("Connection refused");
        let err = McpToolError::from_gateway_error(&gateway_err);
        assert_eq!(err.code, -32003); // GATEWAY_UNREACHABLE
    }

    #[test]
    fn test_mcp_tool_error_from_gateway_server_error() {
        let gateway_err = GatewayError::server_error(500, "Internal error");
        let err = McpToolError::from_gateway_error(&gateway_err);
        assert_eq!(err.code, -32004); // SERVER_ERROR
        assert_eq!(err.message, "Internal error");
    }
}
