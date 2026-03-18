use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, ApprovalId, ApprovalRequest, ApprovalState, CapabilityId, CapabilityLease,
    CapabilityStatus, EventId, ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, ProposalId, ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest,
    RollbackContract, RollbackContractId, RollbackState, Timestamp,
};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub entry_id: i64,
    pub event_id: EventId,
    pub intent_id: Option<IntentId>,
    pub execution_id: Option<ExecutionId>,
    pub occurred_at: Timestamp,
    pub content_hash: Option<String>,
    pub previous_ledger_hash: Option<String>,
    pub raw_json: serde_json::Value,
}

#[async_trait]
pub trait IntentRepo: Send + Sync {
    async fn insert(&self, intent: &IntentEnvelope) -> Result<()>;
    async fn get(&self, intent_id: IntentId) -> Result<Option<IntentEnvelope>>;
    async fn update(&self, intent: &IntentEnvelope) -> Result<()>;
    async fn update_status(&self, intent_id: IntentId, status: IntentStatus) -> Result<()>;
    async fn list_by_status(&self, status: IntentStatus) -> Result<Vec<IntentEnvelope>>;
}

#[async_trait]
pub trait ProposalRepo: Send + Sync {
    async fn insert(&self, proposal: &ActionProposal) -> Result<()>;
    async fn get(&self, proposal_id: ProposalId) -> Result<Option<ActionProposal>>;
    async fn update(&self, proposal: &ActionProposal) -> Result<()>;
    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<ActionProposal>>;
}

#[async_trait]
pub trait CapabilityRepo: Send + Sync {
    async fn insert(&self, capability: &CapabilityLease) -> Result<()>;
    async fn get(&self, capability_id: CapabilityId) -> Result<Option<CapabilityLease>>;
    async fn update(&self, capability: &CapabilityLease) -> Result<()>;
    async fn update_status(
        &self,
        capability_id: CapabilityId,
        status: CapabilityStatus,
    ) -> Result<()>;
    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<CapabilityLease>>;
}

#[async_trait]
pub trait ExecutionRepo: Send + Sync {
    async fn insert(&self, execution: &ExecutionRecord) -> Result<()>;
    async fn get(&self, execution_id: ExecutionId) -> Result<Option<ExecutionRecord>>;
    async fn update(&self, execution: &ExecutionRecord) -> Result<()>;
    async fn update_state(&self, execution_id: ExecutionId, state: ExecutionState) -> Result<()>;
    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<ExecutionRecord>>;
    async fn list_by_capability(&self, capability_id: CapabilityId)
    -> Result<Vec<ExecutionRecord>>;
}

#[async_trait]
pub trait RollbackRepo: Send + Sync {
    async fn insert(&self, contract: &RollbackContract) -> Result<()>;
    async fn get(&self, contract_id: RollbackContractId) -> Result<Option<RollbackContract>>;
    async fn update(&self, contract: &RollbackContract) -> Result<()>;
    async fn update_state(
        &self,
        contract_id: RollbackContractId,
        state: RollbackState,
    ) -> Result<()>;
    async fn list_by_execution(&self, execution_id: ExecutionId) -> Result<Vec<RollbackContract>>;
}

#[async_trait]
pub trait ApprovalRepo: Send + Sync {
    async fn insert(&self, approval: &ApprovalRequest) -> Result<()>;
    async fn get(&self, approval_id: ApprovalId) -> Result<Option<ApprovalRequest>>;
    async fn update(&self, approval: &ApprovalRequest) -> Result<()>;
    async fn resolve(&self, approval_id: ApprovalId, state: ApprovalState) -> Result<()>;
    async fn list_pending(&self) -> Result<Vec<ApprovalRequest>>;
}

#[async_trait]
pub trait ProvenanceRepo: Send + Sync {
    async fn append_event(&self, event: &ProvenanceEvent) -> Result<()>;
    async fn get_event(&self, event_id: EventId) -> Result<Option<ProvenanceEvent>>;
    async fn append_edges(&self, to_event_id: EventId, edges: &[ProvenanceEdge]) -> Result<()>;
    async fn query(&self, request: &ProvenanceQueryRequest) -> Result<Vec<ProvenanceEvent>>;
    /// Query edges where the given event is the target (incoming edges)
    async fn get_edges_to(&self, event_id: EventId) -> Result<Vec<ProvenanceEdge>>;
}

#[async_trait]
pub trait LedgerRepo: Send + Sync {
    async fn append(&self, entry: &LedgerEntry) -> Result<()>;
    async fn get_by_event(&self, event_id: EventId) -> Result<Option<LedgerEntry>>;
    async fn list_recent(&self, limit: u32) -> Result<Vec<LedgerEntry>>;
}
