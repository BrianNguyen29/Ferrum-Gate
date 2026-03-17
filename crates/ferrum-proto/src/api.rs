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

// Execute request/response types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteRequest {
    pub execution_id: crate::ExecutionId,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteResponse {
    pub execution_id: crate::ExecutionId,
    pub executed: bool,
    pub result_digest: Option<String>,
    pub external_id: Option<String>,
}

// Verify request/response types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyRequest {
    pub execution_id: crate::ExecutionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyResponse {
    pub execution_id: crate::ExecutionId,
    pub verified: bool,
    pub verified_at: Option<crate::Timestamp>,
}

// Commit request/response types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitRequest {
    pub execution_id: crate::ExecutionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitResponse {
    pub execution_id: crate::ExecutionId,
    pub committed: bool,
    pub committed_at: Option<crate::Timestamp>,
}
