use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, ApprovalId, ApprovalRequest, ApprovalState, CapabilityId, CapabilityLease,
    CapabilityStatus, EventId, ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, PolicyBundle, ProposalId, ProvenanceEdge, ProvenanceEvent,
    ProvenanceQueryRequest, RollbackContract, RollbackContractId, RollbackState, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    /// List intents with optional filters and cursor-based pagination.
    /// Returns (items, next_cursor) where next_cursor is None when no more pages.
    async fn list_intents(
        &self,
        intent_id: Option<IntentId>,
        statuses: &[IntentStatus],
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<IntentEnvelope>, Option<String>)>;
    /// List intents with the latest execution state for each intent.
    /// Returns (items, next_cursor) where items are (intent, exec_state) tuples.
    /// exec_state is None when no execution exists for the intent.
    /// Bounded by limit (fetches limit+1 to determine if there are more pages).
    async fn list_intents_with_exec_state(
        &self,
        intent_id: Option<IntentId>,
        statuses: &[IntentStatus],
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<(IntentEnvelope, Option<String>)>, Option<String>)>;
}

#[async_trait]
pub trait ProposalRepo: Send + Sync {
    async fn insert(&self, proposal: &ActionProposal) -> Result<()>;
    async fn get(&self, proposal_id: ProposalId) -> Result<Option<ActionProposal>>;
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
    /// Atomically update capability status only if current status is Active.
    /// Returns true if the row was updated (was Active), false otherwise.
    async fn update_status_if_active(
        &self,
        capability_id: CapabilityId,
        status: CapabilityStatus,
    ) -> Result<bool>;
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
    async fn list_pending_paginated(&self, limit: u32, offset: u32)
    -> Result<Vec<ApprovalRequest>>;
    async fn list_pending_by_proposal_paginated(
        &self,
        proposal_id: ProposalId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ApprovalRequest>>;
    /// Cursor-based pagination for pending approvals.
    /// Uses keyset pagination with (created_at DESC, approval_id DESC).
    /// Returns items older than or equal to the cursor position.
    async fn list_pending_cursor(
        &self,
        created_after: Timestamp,
        approval_id_after: ApprovalId,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>>;
    /// Cursor-based pagination for pending approvals filtered by proposal.
    /// Uses keyset pagination with (created_at DESC, approval_id DESC).
    async fn list_pending_by_proposal_cursor(
        &self,
        proposal_id: ProposalId,
        created_after: Timestamp,
        approval_id_after: ApprovalId,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>>;
}

#[async_trait]
pub trait ProvenanceRepo: Send + Sync {
    async fn append_event(&self, event: &ProvenanceEvent) -> Result<()>;
    async fn get_event(&self, event_id: EventId) -> Result<Option<ProvenanceEvent>>;
    async fn append_edges(&self, to_event_id: EventId, edges: &[ProvenanceEdge]) -> Result<()>;
    async fn query(&self, request: &ProvenanceQueryRequest) -> Result<Vec<ProvenanceEvent>>;
    async fn get_edges_to(&self, to_event_id: EventId) -> Result<Vec<ProvenanceEdge>>;
    /// Get all edges that originate from any of the given event IDs (child edges / descendants).
    async fn get_edges_from(&self, from_event_ids: &[EventId]) -> Result<Vec<ProvenanceEdge>>;
}

#[async_trait]
pub trait LedgerRepo: Send + Sync {
    async fn append(&self, entry: &LedgerEntry) -> Result<()>;
    async fn get_by_event(&self, event_id: EventId) -> Result<Option<LedgerEntry>>;
    async fn list_recent(&self, limit: u32) -> Result<Vec<LedgerEntry>>;
    /// Get the latest ledger entry by entry_id, if any.
    async fn get_latest(&self) -> Result<Option<LedgerEntry>>;
    /// Verify the ledger chain integrity.
    async fn verify_chain(&self) -> Result<()>;
}

/// Repository for policy bundles.
///
/// Provides CRUD operations for policy bundles with content-hash based
/// idempotency support for create operations.
#[async_trait]
pub trait PolicyBundleRepo: Send + Sync {
    /// Insert a new policy bundle.
    /// Returns an error if a bundle with the same bundle_id already exists.
    async fn insert(&self, bundle: &PolicyBundle) -> Result<()>;

    /// Get a policy bundle by its bundle_id.
    async fn get(&self, bundle_id: &str) -> Result<Option<PolicyBundle>>;

    /// Get a policy bundle by its content hash.
    /// Used for idempotency checks on create operations.
    async fn get_by_content_hash(&self, content_hash: &str) -> Result<Option<PolicyBundle>>;

    /// Update an existing policy bundle.
    /// Returns an error if the bundle does not exist.
    async fn update(&self, bundle: &PolicyBundle) -> Result<()>;

    /// Delete a policy bundle by its bundle_id.
    /// Returns an error if the bundle does not exist.
    async fn delete(&self, bundle_id: &str) -> Result<()>;

    /// List all policy bundles.
    async fn list(&self) -> Result<Vec<PolicyBundle>>;

    /// List all active policy bundles.
    async fn list_active(&self) -> Result<Vec<PolicyBundle>>;

    /// Set the active flag for a policy bundle.
    async fn set_active(&self, bundle_id: &str, active: bool) -> Result<()>;
}

/// Facade trait that bundles all repository accessors.
///
/// Decouples GatewayRuntime from any concrete store implementation.
/// Each accessor returns an Arc<dyn XxxRepo>.
#[async_trait]
pub trait StoreFacade: Send + Sync {
    fn capabilities(&self) -> Arc<dyn CapabilityRepo>;
    fn executions(&self) -> Arc<dyn ExecutionRepo>;
    fn rollback_contracts(&self) -> Arc<dyn RollbackRepo>;
    fn approvals(&self) -> Arc<dyn ApprovalRepo>;
    fn provenance(&self) -> Arc<dyn ProvenanceRepo>;
    fn ledger(&self) -> Arc<dyn LedgerRepo>;
    fn intents(&self) -> Arc<dyn IntentRepo>;
    fn proposals(&self) -> Arc<dyn ProposalRepo>;
    fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo>;

    /// Returns the current number of pending write operations in the queue.
    /// This represents operations that have been sent but not yet completed processing.
    fn write_queue_depth(&self) -> usize;

    /// Performs a cheap health check on the store.
    /// Returns Ok(()) if the store is reachable and functional.
    /// Returns Err if the store is unavailable or not functional.
    async fn health_check(&self) -> crate::Result<()>;
}
