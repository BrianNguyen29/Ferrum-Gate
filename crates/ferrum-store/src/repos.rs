use async_trait::async_trait;
use ferrum_ledger::LedgerEntry;
use ferrum_proto::{
    ActionProposal, ApprovalId, ApprovalRequest, ApprovalState, CapabilityId, CapabilityLease,
    CapabilityStatus, EventId, ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, ProposalId, ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest,
    RollbackContract, RollbackContractId, RollbackState,
};

use crate::Result;

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

    /// Atomically mark a capability as Used if it is currently Active.
    /// Returns true if the capability was successfully marked as Used,
    /// false if it was already Used, Expired, Revoked, or Quarantined.
    /// This is fail-closed: once used, a capability cannot be used again.
    async fn mark_used_if_active(&self, capability_id: CapabilityId) -> Result<bool>;

    /// Revoke a capability, setting its status to Revoked and persisting revoked_at.
    async fn revoke(&self, capability_id: CapabilityId) -> Result<()>;

    /// List capabilities that are active and not yet expired.
    /// Used for reconciliation and auditing.
    async fn list_active(&self) -> Result<Vec<CapabilityLease>>;
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

    /// Cursor-based pagination for pending approvals.
    /// Returns (items, next_cursor) where next_cursor is None if this is the last page.
    /// Ordering: created_at DESC, approval_id DESC (stable descending).
    async fn list_pending_cursor(
        &self,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)>;

    /// Cursor-based pagination for pending approvals filtered by proposal_id.
    /// Returns (items, next_cursor) where next_cursor is None if this is the last page.
    /// Ordering: created_at DESC, approval_id DESC (stable descending).
    async fn list_pending_by_proposal_cursor(
        &self,
        proposal_id: ProposalId,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)>;

    /// Cursor-based pagination for pending approvals filtered by execution_id.
    /// Returns (items, next_cursor) where next_cursor is None if this is the last page.
    /// Ordering: created_at DESC, approval_id DESC (stable descending).
    async fn list_pending_by_execution_id_cursor(
        &self,
        execution_id: ExecutionId,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)>;

    /// Cursor-based pagination for pending approvals filtered by both proposal_id and execution_id.
    /// Returns (items, next_cursor) where next_cursor is None if this is the last page.
    /// Ordering: created_at DESC, approval_id DESC (stable descending).
    async fn list_pending_by_proposal_and_execution_id_cursor(
        &self,
        proposal_id: ProposalId,
        execution_id: ExecutionId,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)>;
}

#[async_trait]
pub trait ProvenanceRepo: Send + Sync {
    async fn append_event(&self, event: &ProvenanceEvent) -> Result<()>;
    async fn get_event(&self, event_id: EventId) -> Result<Option<ProvenanceEvent>>;
    async fn append_edges(&self, to_event_id: EventId, edges: &[ProvenanceEdge]) -> Result<()>;
    async fn query(&self, request: &ProvenanceQueryRequest) -> Result<Vec<ProvenanceEvent>>;
    /// Query events with cursor-based pagination and DB-level filtering.
    /// Returns (events, next_cursor) where next_cursor is None if this is the last page.
    /// Ordering: occurred_at ASC, event_id ASC (stable ascending).
    async fn query_paginated(
        &self,
        request: &ProvenanceQueryRequest,
    ) -> Result<(Vec<ProvenanceEvent>, Option<String>)>;
    /// Query edges where the given event is the target (incoming edges / ancestry)
    async fn get_edges_to(&self, event_id: EventId) -> Result<Vec<ProvenanceEdge>>;
    /// Query edges where the given event is the source (outgoing edges / descendants)
    async fn get_edges_from(&self, event_id: EventId) -> Result<Vec<ProvenanceEdge>>;
    /// Reconstruct the lineage chain for a single event by walking edges backwards.
    /// Returns events ordered by occurred_at (oldest first), including the starting event.
    async fn get_lineage_by_event(&self, event_id: EventId) -> Result<Vec<ProvenanceEvent>>;
    /// Reconstruct the lineage chain for an execution by walking edges backwards.
    /// Returns events ordered by occurred_at (oldest first).
    async fn get_lineage_by_execution(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Vec<ProvenanceEvent>>;
}

#[async_trait]
pub trait LedgerRepo: Send + Sync {
    async fn append(&self, entry: &LedgerEntry) -> Result<()>;
    async fn append_event(&self, event: &ProvenanceEvent) -> Result<LedgerEntry>;
    async fn get_by_event(&self, event_id: EventId) -> Result<Option<LedgerEntry>>;
    async fn list_recent(&self, limit: u32) -> Result<Vec<LedgerEntry>>;
    /// Returns the most recent ledger entry, if any.
    async fn get_latest(&self) -> Result<Option<LedgerEntry>>;
    /// Lists all ledger entries ordered by entry_id ASC (chronological order).
    /// Used for chain verification after loading from persistence.
    async fn list_all(&self) -> Result<Vec<LedgerEntry>>;
}
