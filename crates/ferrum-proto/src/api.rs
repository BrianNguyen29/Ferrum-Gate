use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateProposalResponse {
    pub decision: crate::Decision,
    pub reason: String,
    pub matched_rule_ids: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateOutcomeResponse {
    /// Whether the outcome matches the intent's expectations.
    pub aligned: bool,
    /// Human-readable explanation of alignment decision.
    pub reason: String,
    /// Matched rule IDs from outcome evaluation.
    pub matched_rule_ids: Vec<String>,
    /// Warnings (e.g., advisory mismatch).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthorizeExecutionRequest {
    pub proposal_id: crate::ProposalId,
    pub capability_id: crate::CapabilityId,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthorizeExecutionResponse {
    pub execution: crate::ExecutionRecord,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrepareExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub prepared: bool,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompensateExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub compensated: bool,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}

/// Request body for the execute endpoint.
/// Payload format is adapter-specific; for FileWrite fs adapter, supply
/// `{"content": "..."}` or a raw string.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteExecutionRequest {
    /// Optional JSON payload passed to the adapter's execute method.
    /// For FileWrite, this should contain the new file content.
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub executed: bool,
    pub result_digest: Option<String>,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}

/// Response envelope for the verify endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub verified: bool,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
}

/// Status of a single component in the deep health check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ComponentStatus {
    /// Name of the component (e.g., "store").
    pub component: String,
    /// Human-readable status description.
    pub status: String,
    /// True if the component is healthy.
    pub healthy: bool,
    /// Optional error message when unhealthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Deep health check response for /v1/readyz/deep.
///
/// Returns structured status for each component. Returns HTTP 200 when all
/// components are healthy, HTTP 503 when any component is unhealthy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeepHealthResponse {
    /// Overall status: "ok" or "degraded".
    pub status: String,
    /// True if all components are healthy.
    pub healthy: bool,
    /// Detailed status of each component.
    pub components: Vec<ComponentStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    pub correlation_id: String,
    pub retriable: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ApiErrorCode {
    ValidationError,
    NotFound,
    PolicyDenied,
    ApprovalRequired,
    CapabilityExpired,
    CapabilityRevoked,
    IntegrityMismatch,
    RollbackUnsupported,
    AdapterFailure,
    Conflict,
    Internal,
    Unauthorized,
}

/// Response envelope for paginated approval lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalListEnvelope {
    pub items: Vec<crate::ApprovalRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Response for GET /v1/executions/{id} — includes execution record and
/// optionally the linked rollback contract for fs-first rollback inspection.
///
/// The rollback_contract field is populated when the execution has a
/// rollback_contract_id set and the contract is retrievable from the store.
/// This enables operators to inspect contract state, target path, before_hash,
/// after_hash, compensation_plan, and verify_checks for the fs-first FileWrite slice.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionDetailResponse {
    pub execution: crate::ExecutionRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_contract: Option<crate::RollbackContract>,
}

// Policy Bundle API types

/// Request to create a new policy bundle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatePolicyBundleRequest {
    /// The YAML content of the policy bundle.
    pub yaml_content: String,
}

/// Response after creating a policy bundle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleResponse {
    pub bundle: crate::PolicyBundle,
    /// SHA-256 hash of the bundle content.
    pub content_hash: String,
}

/// Response for listing policy bundles.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleListResponse {
    pub bundles: Vec<crate::PolicyBundle>,
    pub total: usize,
}

/// Request to update an existing policy bundle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdatePolicyBundleRequest {
    /// The YAML content of the policy bundle.
    pub yaml_content: String,
}

/// Request to set the active flag of a policy bundle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetPolicyBundleActiveRequest {
    pub active: bool,
}

/// Request to simulate a policy bundle against a sample proposal.
/// Side-effect free: no proposal, bundle, or provenance is persisted.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleSimulateRequest {
    /// The YAML content of the policy bundle to evaluate.
    pub bundle_yaml: String,
    /// The sample proposal to evaluate against.
    pub proposal: crate::ActionProposal,
    /// Optional intent envelope. If omitted, a minimal intent is scaffolded
    /// from the proposal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<crate::IntentEnvelope>,
}

/// Response for the policy bundle simulate endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleSimulateResponse {
    /// The decision produced by the bundle (or Allow if no rule matched).
    pub decision: crate::Decision,
    /// Human-readable explanation of the decision.
    pub reason: String,
    /// IDs of matched rules, if any.
    pub matched_rule_ids: Vec<String>,
    /// Advisory warnings (e.g. unknown matchers encountered).
    pub warnings: Vec<String>,
}
