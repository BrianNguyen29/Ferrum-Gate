use crate::{
    CapabilityId, ExecutionId, JsonMap, ProposalId, RiskTier, RollbackClass, RollbackContractId,
    Timestamp,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionProposal {
    pub proposal_id: ProposalId,
    pub intent_id: crate::IntentId,
    pub step_index: u32,
    pub title: String,
    pub tool_name: String,
    pub server_name: String,
    pub raw_arguments: serde_json::Value,
    pub expected_effect: String,
    pub estimated_risk: RiskTier,
    pub requested_rollback_class: RollbackClass,
    pub decision: Option<crate::Decision>,
    pub taint_inputs: Vec<String>,
    pub metadata: JsonMap,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionRecord {
    pub execution_id: ExecutionId,
    pub proposal_id: ProposalId,
    pub intent_id: crate::IntentId,
    pub capability_id: CapabilityId,
    pub rollback_contract_id: Option<RollbackContractId>,
    pub decision: crate::Decision,
    pub state: ExecutionState,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub result_digest: Option<String>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ExecutionState {
    Proposed,
    Authorized,
    Prepared,
    Running,
    AwaitingApproval,
    AwaitingVerification,
    Committed,
    Compensated,
    RolledBack,
    Denied,
    Quarantined,
    Failed,
}
