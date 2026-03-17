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
pub struct HealthResponse {
    pub status: String,
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
}
