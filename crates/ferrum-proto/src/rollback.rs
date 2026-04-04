use crate::{
    ExecutionId, HttpMethod, JsonMap, ProposalId, RollbackClass, RollbackContractId, Timestamp,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackContract {
    pub contract_id: RollbackContractId,
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub execution_id: ExecutionId,
    pub action_type: ActionType,
    pub rollback_class: RollbackClass,
    pub adapter_key: String,
    pub target: RollbackTarget,
    pub prepare_checks: Vec<CheckSpec>,
    pub verify_checks: Vec<CheckSpec>,
    pub compensation_plan: Vec<CompensationStep>,
    pub auto_commit: bool,
    pub state: RollbackState,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum RollbackState {
    PendingPrepare,
    Prepared,
    ExecutedAwaitingVerify,
    Verified,
    Committed,
    CompensationPending,
    Compensated,
    RolledBack,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ActionType {
    FileWrite,
    FileDelete,
    GitCommit,
    GitBranchCreate,
    GitPush,
    GitFetch,
    GitPull,
    SqlMutation,
    HttpMutation,
    EmailDraftCreate,
    EmailSend,
    McpToolMutation,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum RollbackTarget {
    FilePath {
        path: String,
        before_hash: Option<String>,
        after_hash: Option<String>,
    },
    GitRef {
        repo_path: String,
        before_ref: Option<String>,
        after_ref: Option<String>,
    },
    SqliteTxn {
        db_path: String,
        tx_id: String,
    },
    HttpRequest {
        method: HttpMethod,
        url: String,
        request_digest: String,
    },
    EmailDraft {
        draft_id: Option<String>,
        recipients: Vec<String>,
    },
    Generic {
        namespace: String,
        identifier: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckSpec {
    pub check_type: CheckType,
    pub config: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum CheckType {
    FileExists,
    FileHashMatches,
    GitRefMatches,
    SqlRowCountRange,
    HttpStatusExpected,
    EmailDraftExists,
    JsonPredicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompensationStep {
    pub order: u32,
    pub adapter_key: String,
    pub operation: String,
    pub args: JsonMap,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackPrepareRequest {
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub execution_id: ExecutionId,
    pub action_type: ActionType,
    pub rollback_class: RollbackClass,
    pub adapter_key: String,
    pub target: RollbackTarget,
    pub prepare_checks: Vec<CheckSpec>,
    pub verify_checks: Vec<CheckSpec>,
    pub compensation_plan: Vec<CompensationStep>,
    pub auto_commit: bool,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackPrepareResponse {
    pub contract: RollbackContract,
    pub accepted: bool,
    pub warnings: Vec<String>,
}
