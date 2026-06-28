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
//! | `ferrum_gate_query_lineage` | GET /v1/provenance/lineage/{execution_id} | Yes |
//! | `ferrum_gate_list_approvals` | GET /v1/approvals | Yes |
//! | `ferrum_gate_list_policy_bundles` | GET /v1/policy-bundles | Yes |
//! | `ferrum_gate_list_bridges` | GET /v1/bridges | Yes |
//! | `ferrum_gate_list_bridge_tools` | GET /v1/bridges/{id}/tools | Yes |
//!
//! ## D1.7 Lifecycle Tools
//!
//! | MCP Tool | REST Endpoint | Notes |
//! |----------|---------------|-------|
//! | `ferrum_gate_submit_intent` | POST /v1/intents/compile | D1.7 wired |
//! | `ferrum_gate_evaluate_intent` | POST /v1/proposals/{id}/evaluate | D1.7 wired |
//! | `ferrum_gate_mint_capability` | POST /v1/capabilities/mint | D1.7 wired |
//! | `ferrum_gate_authorize_execution` | POST /v1/executions/authorize | D1.7 wired |
//! | `ferrum_gate_prepare_execution` | POST /v1/executions/{id}/prepare | D1.7 wired |
//! | `ferrum_gate_execute_prepared` | POST /v1/executions/{id}/execute | D1.7 wired |
//! | `ferrum_gate_verify` | POST /v1/executions/{id}/verify | D1.7 wired |
//! | `ferrum_gate_compensate` | POST /v1/executions/{id}/compensate | D1.7 wired |
//!
//! ## D1.9 Approval Tools
//!
//! | MCP Tool | REST Endpoint | Notes |
//! |----------|---------------|-------|
//! | `ferrum_gate_approve_intent` | POST /v1/approvals/{id}/resolve | D1.9 wired |
//! | `ferrum_gate_reject_intent` | POST /v1/approvals/{id}/resolve | D1.9 wired |

use crate::BLOCKED_TOOLS;
use crate::ToolsCallResult;
use crate::error_codes::{INVALID_PARAMS, METHOD_NOT_FOUND, NOT_IMPLEMENTED};
use crate::http_client::FerrumGatewayClient;
use crate::http_client::GatewayError;

// Helper to parse a UUID string into a strong_id type (IntentId, ProposalId, etc.)
fn parse_uuid_into_id<T: Copy>(
    s: &str,
    constructor: impl Fn(uuid::Uuid) -> T,
) -> Result<T, McpToolError> {
    uuid::Uuid::parse_str(s)
        .map(constructor)
        .map_err(|_| McpToolError::invalid_args(&format!("Invalid UUID format: {}", s)))
}

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
    #[allow(dead_code)] // Reserved for future use when mutating tools are implemented
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

    /// Create a blocked error for tools with permanently absent backend endpoints.
    /// Per oracle verdict: approve/reject remain blocked due to missing backend endpoints.
    pub fn blocked(tool_name: &str) -> Self {
        Self {
            code: NOT_IMPLEMENTED,
            message: format!(
                "Tool '{}' is permanently blocked: backend endpoint absent",
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

        // Protected read-only endpoints
        "ferrum_gate_list_intents" => call_list_intents(client, args),
        "ferrum_gate_get_execution" => call_get_execution(client, args),
        "ferrum_gate_query_lineage" => call_query_lineage(client, args),
        "ferrum_gate_list_approvals" => call_list_approvals(client),
        "ferrum_gate_list_policy_bundles" => call_list_policy_bundles(client),
        "ferrum_gate_list_bridges" => call_list_bridges(client),
        "ferrum_gate_list_bridge_tools" => call_list_bridge_tools(client, args),

        // D1.7 Lifecycle tools (wired to gateway REST API)
        "ferrum_gate_submit_intent" => call_submit_intent(client, args),
        "ferrum_gate_evaluate_intent" => call_evaluate_intent(client, args),
        "ferrum_gate_mint_capability" => call_mint_capability(client, args),
        "ferrum_gate_authorize_execution" => call_authorize_execution(client, args),
        "ferrum_gate_prepare_execution" => call_prepare_execution(client, args),
        "ferrum_gate_execute_prepared" => call_execute_prepared(client, args),
        "ferrum_gate_verify" => call_verify(client, args),
        "ferrum_gate_compensate" => call_compensate(client, args),

        // D1.9 Approval tools (wired to gateway REST API)
        "ferrum_gate_approve_intent" => call_approve_intent(client, args),
        "ferrum_gate_reject_intent" => call_reject_intent(client, args),

        // Permanently blocked tools (no backend endpoint)
        _ if BLOCKED_TOOLS.contains(&tool_name) => Err(McpToolError::blocked(tool_name)),

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
    let execution_id = args
        .get("execution_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("execution_id"))?;

    match client.query_lineage(execution_id) {
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
// D1.7 Lifecycle Tool Call Functions
// ---------------------------------------------------------------------------

fn call_submit_intent(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let principal_id = args
        .get("principal_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("principal_id"))?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("title"))?;
    let goal = args
        .get("goal")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("goal"))?;
    let action_type = args
        .get("action_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("action_type"))?;
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("target"))?;
    let scope = args
        .get("scope")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("scope"))?;
    let parameters = args
        .get("parameters")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let risk_tier = args
        .get("risk_tier")
        .and_then(|v| v.as_str())
        .map(|s| match s {
            "Low" => ferrum_proto::RiskTier::Low,
            "Medium" => ferrum_proto::RiskTier::Medium,
            "High" => ferrum_proto::RiskTier::High,
            "Critical" => ferrum_proto::RiskTier::Critical,
            _ => ferrum_proto::RiskTier::High,
        });

    // Build IntentCompileRequest using mapping helpers
    let action = crate::stage2_types::ToolCallAction::new(
        uuid::Uuid::new_v4().to_string(),
        action_type.to_string(),
        scope.to_string(),
        target.to_string(),
        parameters,
        principal_id.to_string(),
    );
    let draft = crate::mapping_helpers::tool_call_action_to_draft_intent_compile_request(&action);

    let request = ferrum_proto::IntentCompileRequest {
        // D78-5: Use UUID v5 stable derivation from actor_id (via draft) rather
        // than separately parsing a raw principal_id UUID string.
        principal_id: draft
            .principal_id
            .map_err(|e| McpToolError::invalid_args(&e.to_string()))?,
        session_id: None,
        channel_id: None,
        title: title.to_string(),
        goal: goal.to_string(),
        agent_plan_summary: draft.agent_plan_summary,
        trusted_context: draft.trusted_context.unwrap_or_default(),
        raw_inputs: draft.raw_inputs.unwrap_or_default(),
        requested_resource_scope: draft.requested_resource_scope.unwrap_or_default(),
        requested_risk_tier: risk_tier.or(draft.requested_risk_tier),
        approval_mode: draft.approval_mode,
        metadata: ferrum_proto::JsonMap::new(),
    };

    match client.compile_intent(&request) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_evaluate_intent(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let proposal_id = args
        .get("proposal_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("proposal_id"))?;
    let intent_id = args
        .get("intent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("intent_id"))?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("title"))?;
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("tool_name"))?;
    let server_name = args
        .get("server_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("server_name"))?;
    let arguments = args
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let expected_effect = args
        .get("expected_effect")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("expected_effect"))?;
    let estimated_risk = args
        .get("estimated_risk")
        .and_then(|v| v.as_str())
        .map(|s| match s {
            "Low" => ferrum_proto::RiskTier::Low,
            "Medium" => ferrum_proto::RiskTier::Medium,
            "High" => ferrum_proto::RiskTier::High,
            "Critical" => ferrum_proto::RiskTier::Critical,
            _ => ferrum_proto::RiskTier::High,
        })
        .unwrap_or(ferrum_proto::RiskTier::High);
    let rollback_class = args
        .get("rollback_class")
        .and_then(|v| v.as_str())
        .map(|s| match s {
            "R0NativeReversible" => ferrum_proto::RollbackClass::R0NativeReversible,
            "R1SnapshotRecoverable" => ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            "R2Compensatable" => ferrum_proto::RollbackClass::R2Compensatable,
            _ => ferrum_proto::RollbackClass::R3IrreversibleHighConsequence,
        })
        .unwrap_or(ferrum_proto::RollbackClass::R3IrreversibleHighConsequence);

    let proposal_id_proto = parse_uuid_into_id(proposal_id, ferrum_proto::ProposalId)?;
    let intent_id_proto = parse_uuid_into_id(intent_id, ferrum_proto::IntentId)?;

    let proposal = ferrum_proto::ActionProposal {
        proposal_id: proposal_id_proto,
        intent_id: intent_id_proto,
        step_index: 0,
        title: title.to_string(),
        tool_name: tool_name.to_string(),
        server_name: server_name.to_string(),
        raw_arguments: arguments,
        expected_effect: expected_effect.to_string(),
        estimated_risk,
        requested_rollback_class: rollback_class,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    match client.evaluate_proposal(&proposal_id_proto, &proposal) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_mint_capability(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let intent_id = args
        .get("intent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("intent_id"))?;
    let proposal_id = args
        .get("proposal_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("proposal_id"))?;
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("tool_name"))?;
    let server_name = args
        .get("server_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("server_name"))?;
    let resource_path = args.get("resource_path").and_then(|v| v.as_str());
    let resource_mode = args
        .get("resource_mode")
        .and_then(|v| v.as_str())
        .map(|s| match s {
            "Read" => ferrum_proto::ResourceMode::Read,
            "Write" => ferrum_proto::ResourceMode::Write,
            _ => ferrum_proto::ResourceMode::Execute,
        })
        .unwrap_or(ferrum_proto::ResourceMode::Execute);
    let ttl_secs = args.get("ttl_secs").and_then(|v| v.as_u64()).unwrap_or(120);

    let mut resource_bindings = vec![];
    if let Some(path) = resource_path {
        resource_bindings.push(ferrum_proto::ResourceBinding::File {
            path: path.to_string(),
            mode: resource_mode,
            required_hash: None,
        });
    }

    let request = ferrum_proto::CapabilityMintRequest {
        intent_id: parse_uuid_into_id(intent_id, ferrum_proto::IntentId)?,
        proposal_id: parse_uuid_into_id(proposal_id, ferrum_proto::ProposalId)?,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
            tool_version: None,
        },
        resource_bindings,
        argument_constraints: vec![],
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 30,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: ttl_secs.min(300), // Cap at 300s per invariants
        metadata: ferrum_proto::JsonMap::new(),
    };

    match client.mint_capability(&request) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_authorize_execution(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let proposal_id = args
        .get("proposal_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("proposal_id"))?;
    let capability_id = args
        .get("capability_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("capability_id"))?;
    let dry_run = args
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: parse_uuid_into_id(proposal_id, ferrum_proto::ProposalId)?,
        capability_id: parse_uuid_into_id(capability_id, ferrum_proto::CapabilityId)?,
        dry_run,
    };

    match client.authorize_execution(&request) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_prepare_execution(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let execution_id = args
        .get("execution_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("execution_id"))?;

    let execution_id_proto = parse_uuid_into_id(execution_id, ferrum_proto::ExecutionId)?;

    match client.prepare_execution(&execution_id_proto) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_execute_prepared(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let execution_id = args
        .get("execution_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("execution_id"))?;
    let payload = args
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let execution_id_proto = parse_uuid_into_id(execution_id, ferrum_proto::ExecutionId)?;

    let request = ferrum_proto::ExecuteExecutionRequest { payload };

    match client.execute_execution(&execution_id_proto, &request) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_verify(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let execution_id = args
        .get("execution_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("execution_id"))?;

    let execution_id_proto = parse_uuid_into_id(execution_id, ferrum_proto::ExecutionId)?;

    match client.verify_execution(&execution_id_proto) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_compensate(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let execution_id = args
        .get("execution_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("execution_id"))?;

    let execution_id_proto = parse_uuid_into_id(execution_id, ferrum_proto::ExecutionId)?;

    match client.compensate_execution(&execution_id_proto) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

// ---------------------------------------------------------------------------
// D1.9 Approval Tool Call Functions
// ---------------------------------------------------------------------------

fn call_approve_intent(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let approval_id = args
        .get("approval_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("approval_id"))?;

    let actor_id = args
        .get("actor_id")
        .and_then(|v| v.as_str())
        .unwrap_or("mcp-agent");
    let actor_label = args
        .get("actor_label")
        .and_then(|v| v.as_str())
        .unwrap_or("MCP Agent");
    let reason = args
        .get("reason")
        .and_then(|v| v.as_str())
        .map(String::from);

    let approval_id_proto = parse_uuid_into_id(approval_id, ferrum_proto::ApprovalId)?;

    let request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Agent,
            actor_id: actor_id.to_string(),
            display_name: Some(actor_label.to_string()),
        },
        approve: true,
        reason,
        mfa_factor: None,
    };

    match client.resolve_approval(&approval_id_proto, &request) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
                ),
            }],
            is_error: false,
        }),
        Err(e) => Err(McpToolError::from_gateway_error(&e)),
    }
}

fn call_reject_intent(
    client: &FerrumGatewayClient,
    args: &serde_json::Value,
) -> Result<ToolsCallResult, McpToolError> {
    let approval_id = args
        .get("approval_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpToolError::missing_arg("approval_id"))?;

    let actor_id = args
        .get("actor_id")
        .and_then(|v| v.as_str())
        .unwrap_or("mcp-agent");
    let actor_label = args
        .get("actor_label")
        .and_then(|v| v.as_str())
        .unwrap_or("MCP Agent");
    let reason = args
        .get("reason")
        .and_then(|v| v.as_str())
        .map(String::from);

    let approval_id_proto = parse_uuid_into_id(approval_id, ferrum_proto::ApprovalId)?;

    let request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Agent,
            actor_id: actor_id.to_string(),
            display_name: Some(actor_label.to_string()),
        },
        approve: false,
        reason,
        mfa_factor: None,
    };

    match client.resolve_approval(&approval_id_proto, &request) {
        Ok(result) => Ok(ToolsCallResult {
            content: vec![crate::ToolContent {
                r#type: "text".to_string(),
                text: Some(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_e| format!("{:?}", result)),
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
    use crate::ClientConfig;

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

    // -------------------------------------------------------------------------
    // call_submit_intent Tests (D1.3.3 compile-only)
    // -------------------------------------------------------------------------

    #[test]
    fn test_call_submit_intent_missing_args() {
        // Verify call_submit_intent fails with missing required arguments.
        let args = serde_json::json!({});
        // Dummy client — request should fail before reaching the client.
        let config = ClientConfig::new().base_url("http://127.0.0.1:1");
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = call_submit_intent(&client, &args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, -32602); // INVALID_PARAMS
        assert!(err.message.contains("principal_id"));
    }

    #[test]
    fn test_call_submit_intent_successful_compile() {
        // D1.3.3: Verify successful compile request path through call_submit_intent.
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "envelope": {
                    "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                    "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "session_id": null,
                    "channel_id": null,
                    "title": "fs_write: /tmp/test.txt",
                    "goal": "MCP tool call: fs_write on /tmp/test.txt",
                    "normalized_goal": "MCP tool call: fs_write on /tmp/test.txt",
                    "allowed_outcomes": [],
                    "forbidden_outcomes": [],
                    "resource_scope": [],
                    "risk_tier": "High",
                    "approval_mode": "Required",
                    "default_rollback_class": "R0NativeReversible",
                    "time_budget": { "max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1 },
                    "trust_context": {
                        "input_labels": [],
                        "sensitivity_labels": [],
                        "taint_score": 0,
                        "contains_external_metadata": false,
                        "contains_tool_output": false,
                        "contains_untrusted_text": false
                    },
                    "derived_from_event_ids": [],
                    "tags": [],
                    "metadata": {},
                    "status": "Active",
                    "created_at": "2025-01-01T00:00:00Z",
                    "expires_at": "2025-12-31T23:59:59Z"
                },
                "warnings": []
            }"#)
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let args = serde_json::json!({
            "principal_id": "550e8400-e29b-41d4-a716-446655440001",
            "title": "fs_write: /tmp/test.txt",
            "goal": "MCP tool call: fs_write on /tmp/test.txt",
            "action_type": "fs_write",
            "target": "/tmp/test.txt",
            "scope": "fs:write:/tmp/test.txt",
            "parameters": {}
        });

        let result = call_submit_intent(&client, &args);
        assert!(
            result.is_ok(),
            "call_submit_intent should succeed: {:?}",
            result.err()
        );

        let tools_result = result.unwrap();
        assert!(!tools_result.is_error);
        assert_eq!(tools_result.content.len(), 1);
        assert_eq!(tools_result.content[0].r#type, "text");
        assert!(tools_result.content[0].text.is_some());
        let text = tools_result.content[0].text.as_ref().unwrap();
        assert!(text.contains("fs_write: /tmp/test.txt"));
        assert!(text.contains("550e8400-e29b-41d4-a716-446655440000"));

        mock.assert();
    }

    #[test]
    fn test_call_submit_intent_gateway_error_mapping() {
        // D1.3.3: Verify error mapping when compile returns a gateway server error.
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "compile failed"}"#)
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let args = serde_json::json!({
            "principal_id": "550e8400-e29b-41d4-a716-446655440001",
            "title": "test",
            "goal": "test goal",
            "action_type": "fs_write",
            "target": "/tmp/test.txt",
            "scope": "fs:write:/tmp/test.txt",
            "parameters": {}
        });

        let result = call_submit_intent(&client, &args);
        assert!(result.is_err(), "call_submit_intent should fail on 500");

        let err = result.unwrap_err();
        assert_eq!(err.code, -32004); // SERVER_ERROR
        assert!(err.message.contains("compile failed"));

        mock.assert();
    }

    #[test]
    fn test_call_query_lineage_missing_execution_id() {
        let args = serde_json::json!({});
        let config = ClientConfig::new().base_url("http://127.0.0.1:1");
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = call_query_lineage(&client, &args);
        assert!(
            result.is_err(),
            "call_query_lineage should fail without execution_id"
        );

        let err = result.unwrap_err();
        assert_eq!(err.code, -32602); // INVALID_PARAMS
        assert!(err.message.contains("execution_id"));
    }

    #[test]
    fn test_call_query_lineage_uses_correct_path() {
        let mut server = mockito::Server::new();
        let execution_id = "550e8400-e29b-41d4-a716-446655440099";
        let mock = server
            .mock(
                "GET",
                format!("/v1/provenance/lineage/{}", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "events": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let args = serde_json::json!({
            "execution_id": execution_id
        });
        let result = call_query_lineage(&client, &args);
        assert!(
            result.is_ok(),
            "call_query_lineage should succeed: {:?}",
            result.err()
        );

        let tools_result = result.unwrap();
        assert!(!tools_result.is_error);
        let text = tools_result.content[0].text.as_ref().unwrap();
        assert!(text.contains(execution_id));

        mock.assert();
    }
}
