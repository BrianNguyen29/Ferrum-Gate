use async_trait::async_trait;
use ferrum_ledger::LedgerEntry;
use ferrum_proto::{
    ActionProposal, ApprovalId, ApprovalRequest, ApprovalState, CapabilityId, CapabilityLease,
    CapabilityStatus, EventId, ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, PolicyBundle, PolicyBundleId, ProposalId, ProvenanceEdge,
    ProvenanceEvent, ProvenanceQueryRequest, ProvenanceStatsRequest, ProvenanceStatsResponse,
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
    /// Compute aggregated statistics over provenance events matching the given filters.
    /// This is more efficient than fetching all events and aggregating client-side
    /// because it performs aggregation at the database level.
    async fn query_stats(
        &self,
        request: &ProvenanceStatsRequest,
    ) -> Result<ProvenanceStatsResponse>;
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

    /// Cursor-based pagination for ledger entries.
    ///
    /// Returns `(entries, next_cursor)` where `next_cursor` is `None` if this is the last page.
    /// Ordering: entry_id DESC (newest-first), providing stable pagination for large ledgers.
    ///
    /// The cursor format is a raw entry_id integer as a string.
    /// Pass `after_cursor = None` for the first page.
    ///
    /// This is the primary method for handling larger-than-memory ledger datasets,
    /// replacing `list_recent` for paginated access.
    async fn list_cursor(
        &self,
        limit: u32,
        after_cursor: Option<u64>,
    ) -> Result<(Vec<LedgerEntry>, Option<u64>)>;
}

/// Repository trait for policy bundle persistence.
///
/// H1.1a: Provides bounded CRUD operations for policy bundle lifecycle management:
/// - Register a new bundle (idempotent by derived bundle_id)
/// - Fetch a bundle by its deterministic id
/// - List all bundles with cursor-based pagination
/// - Update bundle metadata (name, description, version) preserving created_at
/// - Delete a bundle by its deterministic bundle_id
///
/// No engine swap, no policy evaluation changes, single-node/local-only storage.
#[async_trait]
pub trait PolicyBundleRepo: Send + Sync {
    /// Register (upsert) a policy bundle. If a bundle with the same bundle_id
    /// already exists, update its metadata (name, description, version).
    /// The created_at timestamp is preserved from the existing record.
    async fn upsert(&self, bundle: &PolicyBundle) -> Result<()>;

    /// Fetch a bundle by its deterministic bundle_id.
    async fn get(&self, bundle_id: PolicyBundleId) -> Result<Option<PolicyBundle>>;

    /// List all bundles ordered by created_at DESC with cursor-based pagination.
    /// Returns (items, next_cursor) where next_cursor is None if this is the last page.
    async fn list_cursor(
        &self,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<PolicyBundle>, Option<String>)>;

    /// Update metadata for an existing bundle (name, description, version).
    /// The bundle_id and created_at are preserved; updated_at is set to now.
    /// Returns Ok(()) if the bundle was updated, Err if the bundle was not found.
    async fn update_metadata(
        &self,
        bundle_id: PolicyBundleId,
        name: &str,
        description: &str,
        version: &str,
    ) -> Result<()>;

    /// Delete a bundle by its deterministic bundle_id.
    /// Returns Ok(()) if the bundle was deleted, Err if the bundle was not found.
    async fn delete(&self, bundle_id: PolicyBundleId) -> Result<()>;
}
