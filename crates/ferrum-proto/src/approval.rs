use crate::{ActorRef, ApprovalId, ExecutionId, ProposalId, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRequest {
    pub approval_id: ApprovalId,
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub execution_id: Option<ExecutionId>,
    pub requested_by: ActorRef,
    pub reason: String,
    pub action_digest: String,
    pub expires_at: Timestamp,
    pub state: ApprovalState,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ApprovalState {
    Pending,
    Granted,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalResolveRequest {
    pub actor: ActorRef,
    pub approve: bool,
    pub reason: Option<String>,
}
