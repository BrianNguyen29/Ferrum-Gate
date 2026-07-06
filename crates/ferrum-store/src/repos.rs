use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, AgentRecord, ApprovalId, ApprovalRequest, ApprovalState, AuditAction,
    AuditLogEntry, AuditMerkleRoot, AuditResourceType, CapabilityId, CapabilityLease,
    CapabilityStatus, EventId, ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, JsonMap, LifecycleOutboxId, LifecycleOutboxRecord,
    LifecycleOutboxStatus, MfaCredentialRecord, PolicyBundle, PolicyBundleVersion, ProposalId,
    ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest, RollbackContract, RollbackContractId,
    RollbackState, Timestamp,
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

/// Snapshot of a connection pool's current state.
///
/// Emitted by stores that maintain a connection pool (e.g. PostgreSQL).
/// Non-pool stores return `None` from `StoreFacade::pool_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStatus {
    /// Current number of connections in the pool (active + idle).
    pub total_connections: u32,
    /// Current number of idle connections available for use.
    pub idle_connections: u32,
    /// Maximum number of connections the pool is configured to hold.
    pub max_connections: u32,
    /// Cumulative count of pool acquire timeouts since process start.
    pub acquire_timeouts: u64,
}

#[derive(Debug, Clone)]
pub struct LifecycleOutboxLease {
    pub outbox_id: LifecycleOutboxId,
    pub owner: String,
    pub generation: i64,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct LifecycleOutboxClaim {
    pub record: LifecycleOutboxRecord,
    pub lease: LifecycleOutboxLease,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleOutboxLeaseStats {
    pub active: usize,
    pub expired: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationFailureDisposition {
    Retryable,
    NeedsOperatorReview,
    LeaseLost,
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
    /// Atomically revoke an active, unexpired capability and set `revoked_at`.
    /// Returns true if the row was updated, false otherwise.
    async fn revoke_if_active(
        &self,
        capability_id: CapabilityId,
        revoked_at: Timestamp,
    ) -> Result<bool>;
    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<CapabilityLease>>;
}

#[async_trait]
pub trait ExecutionRepo: Send + Sync {
    async fn insert(&self, execution: &ExecutionRecord) -> Result<()>;
    async fn get(&self, execution_id: ExecutionId) -> Result<Option<ExecutionRecord>>;
    async fn update(&self, execution: &ExecutionRecord) -> Result<()>;
    async fn update_state(&self, execution_id: ExecutionId, state: ExecutionState) -> Result<()>;
    async fn compare_and_set_state(
        &self,
        execution_id: ExecutionId,
        expected_states: &[ExecutionState],
        new_state: ExecutionState,
    ) -> Result<bool>;
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
pub trait LifecycleOutboxRepo: Send + Sync {
    async fn enqueue_lifecycle_transition(&self, record: &LifecycleOutboxRecord) -> Result<()>;
    async fn record_lifecycle_transition(
        &self,
        execution: &ExecutionRecord,
        rollback_contract: Option<&RollbackContract>,
        outbox: &LifecycleOutboxRecord,
    ) -> Result<()>;
    async fn record_authorization(
        &self,
        capability: &CapabilityLease,
        execution: &ExecutionRecord,
        outbox: &LifecycleOutboxRecord,
    ) -> Result<bool>;
    async fn mark_provenance_written(
        &self,
        outbox_id: LifecycleOutboxId,
        event_id: EventId,
    ) -> Result<()>;
    async fn mark_provenance_obligation_written(
        &self,
        outbox_id: LifecycleOutboxId,
        event_kind: ferrum_proto::ProvenanceEventKind,
        event_id: EventId,
    ) -> Result<bool>;
    async fn mark_reconciled(&self, outbox_id: LifecycleOutboxId, result: JsonMap) -> Result<()>;
    async fn mark_needs_operator_review(
        &self,
        outbox_id: LifecycleOutboxId,
        reason: String,
    ) -> Result<()>;
    async fn reset_for_retry(
        &self,
        outbox_id: LifecycleOutboxId,
        actor_id: String,
        reason: Option<String>,
    ) -> Result<Option<LifecycleOutboxRecord>>;
    async fn mark_operator_resolved(
        &self,
        outbox_id: LifecycleOutboxId,
        actor_id: String,
        reason: String,
    ) -> Result<Option<LifecycleOutboxRecord>>;
    async fn get(&self, outbox_id: LifecycleOutboxId) -> Result<Option<LifecycleOutboxRecord>>;
    async fn list_by_status(
        &self,
        status: LifecycleOutboxStatus,
        limit: u32,
    ) -> Result<Vec<LifecycleOutboxRecord>>;
    async fn claim_pending_reconciliation(
        &self,
        limit: u32,
        lease_owner: &str,
        lease_ttl: chrono::Duration,
    ) -> Result<Vec<LifecycleOutboxClaim>>;
    async fn renew_reconciliation_lease(
        &self,
        lease: &LifecycleOutboxLease,
        lease_ttl: chrono::Duration,
    ) -> Result<bool>;
    async fn mark_provenance_written_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        event_id: EventId,
    ) -> Result<bool>;
    async fn mark_provenance_obligation_written_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        event_kind: ferrum_proto::ProvenanceEventKind,
        event_id: EventId,
    ) -> Result<bool>;
    async fn mark_reconciled_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        result: JsonMap,
    ) -> Result<bool>;
    async fn mark_needs_operator_review_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        reason: String,
    ) -> Result<bool>;
    async fn record_reconciliation_failure(
        &self,
        lease: &LifecycleOutboxLease,
        error: String,
        max_attempts: u32,
    ) -> Result<ReconciliationFailureDisposition>;
    async fn reconciliation_lease_stats(&self) -> Result<LifecycleOutboxLeaseStats>;
    async fn list_pending_reconciliation(&self, limit: u32) -> Result<Vec<LifecycleOutboxRecord>>;
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
    /// Append an event and its parent edges atomically.
    ///
    /// This is the required path for internally generated governance events
    /// whose `parent_edges` are part of the causal contract. Implementations
    /// must ensure that an edge insert failure rolls back the event insert.
    async fn append_event_with_edges(
        &self,
        event: &ProvenanceEvent,
        edges: &[ProvenanceEdge],
    ) -> Result<()>;
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

/// Repository for append-only audit logs.
#[async_trait]
pub trait AuditLogRepo: Send + Sync {
    /// Append a new audit log entry.
    async fn append(&self, entry: &AuditLogEntry) -> Result<()>;

    /// List audit log entries with optional filters and cursor-based pagination.
    /// Returns (items, next_cursor) where next_cursor is None when no more pages.
    async fn list(
        &self,
        action: Option<AuditAction>,
        resource_type: Option<AuditResourceType>,
        resource_id: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
        since: Option<chrono::DateTime<chrono::Utc>>,
        until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(Vec<AuditLogEntry>, Option<String>)>;

    /// Verify the audit log hash chain integrity.
    ///
    /// Reads all entries ordered by id ASC and validates that each entry's
    /// `previous_hash` matches the prior hashed entry's `content_hash`.
    /// Legacy entries without `content_hash` are skipped (treated as pre-chain).
    /// Returns `Ok(())` if the chain is valid or empty; returns `Err` with a
    /// descriptive message if a break is detected.
    async fn verify_chain(&self) -> Result<()>;
}

/// Repository for Merkle roots over audit log time windows.
#[async_trait]
pub trait AuditMerkleRootRepo: Send + Sync {
    /// Compute and cache the Merkle root for a given UTC-aligned hourly window.
    ///
    /// If the root is already cached, returns the cached value (idempotent).
    /// Excludes audit log entries without a `content_hash`.
    async fn compute_and_cache_root(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<AuditMerkleRoot>;

    /// Get a cached root by window start, if it exists.
    async fn get_root(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<AuditMerkleRoot>>;

    /// List cached roots with cursor-based pagination.
    /// Returns (items, next_cursor).
    async fn list_roots(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<AuditMerkleRoot>, Option<String>)>;
}

/// Repository for signed audit checkpoints.
#[async_trait]
pub trait AuditCheckpointRepo: Send + Sync {
    /// Insert a signed checkpoint.
    ///
    /// Returns an error if a checkpoint for the same window_start already exists.
    async fn insert(&self, checkpoint: &ferrum_proto::AuditCheckpoint) -> Result<()>;

    /// Get a checkpoint by window start, if it exists.
    async fn get(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<ferrum_proto::AuditCheckpoint>>;

    /// List checkpoints with cursor-based pagination.
    /// Returns (items, next_cursor).
    async fn list(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<ferrum_proto::AuditCheckpoint>, Option<String>)>;
}

/// Repository for scoped tokens.
///
/// Provides CRUD operations for opaque scoped bearer tokens with
/// hashed token values and durable revocation.
#[async_trait]
pub trait TokenRepo: Send + Sync {
    /// Insert a new scoped token.
    async fn insert(&self, token: &ferrum_proto::ScopedToken) -> Result<()>;

    /// Get a token by its token_id (metadata only).
    async fn get(&self, token_id: &str) -> Result<Option<ferrum_proto::ScopedToken>>;

    /// Get a token by its deterministic lookup hash (blake3 of raw token value).
    /// Returns the full token record including the secure verification hash and salt.
    async fn get_by_lookup_hash(
        &self,
        lookup_hash: &str,
    ) -> Result<Option<ferrum_proto::ScopedToken>>;

    /// List tokens with optional filters.
    async fn list(
        &self,
        actor_id: Option<&str>,
        role: Option<&str>,
        active_only: bool,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<(Vec<ferrum_proto::ScopedToken>, Option<String>)>;

    /// Revoke a token by setting revoked_at.
    async fn revoke(&self, token_id: &str, reason: Option<&str>) -> Result<bool>;

    /// Update last_used_at timestamp.
    async fn touch(&self, token_id: &str) -> Result<()>;
}

/// Repository for agent registry entries.
#[async_trait]
pub trait AgentRepo: Send + Sync {
    /// Insert a new agent record.
    async fn insert(&self, agent: &AgentRecord) -> Result<()>;

    /// Get an agent by its `agent_id`.
    async fn get(&self, agent_id: &str) -> Result<Option<AgentRecord>>;

    /// Get an agent by its `key_fingerprint`.
    async fn get_by_fingerprint(&self, fingerprint: &str) -> Result<Option<AgentRecord>>;

    /// List agents with optional filters.
    async fn list(
        &self,
        active_only: bool,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<(Vec<AgentRecord>, Option<String>)>;

    /// Count agents, optionally excluding revoked.
    async fn count(&self, active_only: bool) -> Result<usize>;

    /// Revoke an agent by setting `revoked_at`.
    /// Returns true if the row was updated (was not already revoked).
    async fn revoke(&self, agent_id: &str) -> Result<bool>;
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

    /// List version history for a policy bundle.
    async fn list_versions(&self, bundle_id: &str) -> Result<Vec<PolicyBundleVersion>>;

    /// Get a specific version of a policy bundle.
    async fn get_version(
        &self,
        bundle_id: &str,
        version: i64,
    ) -> Result<Option<PolicyBundleVersion>>;

    /// Rollback a policy bundle to a previous version.
    /// Creates a new version row copied from the target version, increments
    /// the version number, sets it active, and returns the new version number.
    async fn rollback(
        &self,
        bundle_id: &str,
        target_version: i64,
        actor: Option<&str>,
    ) -> Result<i64>;
}

/// Repository for MFA credentials.
#[async_trait]
pub trait MfaCredentialRepo: Send + Sync {
    /// Insert a new MFA credential record.
    async fn insert(&self, record: &MfaCredentialRecord) -> Result<()>;

    /// Get a credential by its factor ID.
    async fn get(
        &self,
        mfa_factor_id: ferrum_proto::MfaFactorId,
    ) -> Result<Option<MfaCredentialRecord>>;

    /// Get the active (non-revoked, status=Active) credential for an agent, if any.
    async fn get_active_for_agent(&self, agent_id: &str) -> Result<Option<MfaCredentialRecord>>;

    /// List all credentials for an agent (including pending and revoked).
    async fn list_by_agent(&self, agent_id: &str) -> Result<Vec<MfaCredentialRecord>>;

    /// Activate a pending credential and set `verified_at`.
    async fn activate(&self, mfa_factor_id: ferrum_proto::MfaFactorId) -> Result<bool>;

    /// Update `last_used_at` and `last_used_counter` after successful verification.
    async fn record_use(
        &self,
        mfa_factor_id: ferrum_proto::MfaFactorId,
        counter: u64,
    ) -> Result<bool>;

    /// Record a failed verification attempt and optionally lock the factor.
    ///
    /// Increments `failed_attempts`, sets `last_failed_at` to now.
    /// If the new `failed_attempts` reaches `max_attempts`, sets `locked_until`
    /// to `now + duration_secs` and increments `lockout_count`.
    /// Returns `true` if the factor was locked by this call.
    async fn record_failed_attempt(
        &self,
        mfa_factor_id: ferrum_proto::MfaFactorId,
        max_attempts: u32,
        lockout_duration_secs: u64,
    ) -> Result<bool>;

    /// Reset lockout state after a successful verification.
    ///
    /// Sets `failed_attempts = 0`, `locked_until = NULL`, `last_failed_at = NULL`.
    /// Preserves `lockout_count`.
    async fn reset_lockout(&self, mfa_factor_id: ferrum_proto::MfaFactorId) -> Result<bool>;

    /// Revoke a credential by setting `revoked_at`.
    async fn revoke(&self, mfa_factor_id: ferrum_proto::MfaFactorId) -> Result<bool>;
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
    fn lifecycle_outbox(&self) -> Arc<dyn LifecycleOutboxRepo>;
    fn approvals(&self) -> Arc<dyn ApprovalRepo>;
    fn provenance(&self) -> Arc<dyn ProvenanceRepo>;
    fn ledger(&self) -> Arc<dyn LedgerRepo>;
    fn intents(&self) -> Arc<dyn IntentRepo>;
    fn proposals(&self) -> Arc<dyn ProposalRepo>;
    fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo>;
    fn tokens(&self) -> Arc<dyn TokenRepo>;
    fn audit_log(&self) -> Arc<dyn AuditLogRepo>;
    fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo>;
    fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo>;
    fn agents(&self) -> Arc<dyn AgentRepo>;
    fn mfa_credentials(&self) -> Arc<dyn MfaCredentialRepo>;

    /// Returns the current number of pending write operations in the queue.
    /// This represents operations that have been sent but not yet completed processing.
    fn write_queue_depth(&self) -> usize;

    /// Performs a cheap health check on the store.
    /// Returns Ok(()) if the store is reachable and functional.
    /// Returns Err if the store is unavailable or not functional.
    async fn health_check(&self) -> crate::Result<()>;

    /// Returns the current connection pool status, if applicable.
    ///
    /// Returns `Some(PoolStatus)` for stores that manage a connection pool
    /// (e.g. PostgreSQL), and `None` for stores without a pool (e.g. SQLite).
    fn pool_status(&self) -> Option<PoolStatus> {
        None
    }

    /// Gracefully shut down the store.
    ///
    /// Signals background tasks (e.g. SQLite writer) to stop and waits
    /// for them to drain. The default implementation is a no-op.
    async fn shutdown(&self) -> crate::Result<()> {
        Ok(())
    }
}
