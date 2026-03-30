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
    CapabilityUsed,
    ScopeMismatch,
    IntegrityMismatch,
    RollbackUnsupported,
    AdapterFailure,
    Conflict,
    Internal,
}

/// Response envelope for paginated approval lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalListEnvelope {
    pub items: Vec<crate::approval::ApprovalRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
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

// Compensate request/response types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompensateRequest {
    pub execution_id: crate::ExecutionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompensateResponse {
    pub execution_id: crate::ExecutionId,
    pub compensated: bool,
    pub compensated_at: Option<crate::Timestamp>,
}

// Rollback request/response types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackRequest {
    pub execution_id: crate::ExecutionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackResponse {
    pub execution_id: crate::ExecutionId,
    pub rolled_back: bool,
    pub rolled_back_at: Option<crate::Timestamp>,
}

// Cancel request/response types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancelExecutionRequest {
    pub execution_id: crate::ExecutionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancelExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub cancelled: bool,
    pub cancelled_at: Option<crate::Timestamp>,
}

// Ledger verification types
/// Response for on-demand ledger hash-chain verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LedgerVerificationResponse {
    /// True if the ledger chain is valid, false if tampered or broken.
    pub valid: bool,
    /// Number of ledger entries verified.
    pub entry_count: u64,
    /// Timestamp when verification was performed.
    pub verified_at: crate::Timestamp,
    /// Error details when verification fails (None when valid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LedgerVerificationError>,
}

/// Error details when ledger verification fails.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "detail")]
pub enum LedgerVerificationError {
    /// Chain linkage is broken: prev_hash does not match expected.
    BrokenChain { expected: String, actual: String },
    /// Entry hash mismatch: tampered content detected.
    TamperDetected {
        sequence: u64,
        recorded: String,
        recomputed: String,
    },
    /// Sequence number mismatch.
    SequenceMismatch { event_seq: u64, ledger_len: usize },
    /// Ledger is empty but verification required at least one entry.
    EmptyLedger,
}
