//! PostgreSQL P3 runtime infrastructure — all repos implemented.
//!
//! **PostgreSQL runtime support is partial (P3).**
//! All P3 repos have real implementations: `PostgresIntentRepo`,
//! `PostgresProposalRepo`, `PostgresExecutionRepo`, `PostgresCapabilityRepo`,
//! `PostgresRollbackRepo`, `PostgresApprovalRepo`, `PostgresProvenanceRepo`,
//! `PostgresLedgerRepo`, and `PostgresPolicyBundleRepo`.
//!
//! # P3 Status
//!
//! - [x] PostgresIntentRepo with real sqlx queries
//! - [x] PostgresProposalRepo with real sqlx queries
//! - [x] PostgresExecutionRepo with real sqlx queries
//! - [x] PostgresCapabilityRepo with real sqlx queries
//! - [x] PostgresRollbackRepo with real sqlx queries
//! - [x] PostgresApprovalRepo with real sqlx queries
//! - [x] PostgresProvenanceRepo with real sqlx queries
//! - [x] PostgresLedgerRepo with real sqlx queries
//! - [x] PostgresPolicyBundleRepo with real sqlx queries
//! - [ ] Migration infrastructure (P4)
//! - [ ] Production readiness (P5)
//!
//! See ADR-50 for the phased implementation plan.

mod approvals;
mod capabilities;
mod executions;
mod helpers;
mod intents;
mod ledger;
mod policy_bundles;
mod proposals;
mod provenance;
mod rollback;

pub use approvals::PostgresApprovalRepo;
pub use capabilities::PostgresCapabilityRepo;
pub use executions::PostgresExecutionRepo;
pub use intents::PostgresIntentRepo;
pub use ledger::PostgresLedgerRepo;
pub use policy_bundles::PostgresPolicyBundleRepo;
pub use proposals::PostgresProposalRepo;
pub use provenance::PostgresProvenanceRepo;
pub use rollback::PostgresRollbackRepo;

use crate::Result;
use crate::repos::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, PolicyBundleRepo,
    ProposalRepo, ProvenanceRepo, RollbackRepo, StoreFacade,
};
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;

/// PostgreSQL P3 store.
///
/// **Partial runtime support.** All P3 repos are functional:
/// `PostgresIntentRepo`, `PostgresProposalRepo`, `PostgresExecutionRepo`,
/// `PostgresCapabilityRepo`, `PostgresRollbackRepo`, `PostgresApprovalRepo`,
/// `PostgresProvenanceRepo`, `PostgresLedgerRepo`, and `PostgresPolicyBundleRepo`.
///
/// Use `SqliteStore` for full runtime storage.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Connect to PostgreSQL using the provided database URL.
    ///
    /// Uses a connection pool with `max_connections = 5`.
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Returns a clone of the underlying connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Apply minimal schema migrations required for the intent repo.
    pub async fn apply_intent_migration(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS intents (
                intent_id TEXT PRIMARY KEY,
                principal_id TEXT NOT NULL,
                normalized_goal TEXT NOT NULL,
                status TEXT NOT NULL,
                risk_tier TEXT NOT NULL,
                approval_mode TEXT NOT NULL,
                default_rollback_class TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS executions (
                execution_id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                proposal_id TEXT NOT NULL,
                capability_id TEXT NOT NULL,
                rollback_contract_id TEXT,
                decision TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                result_digest TEXT,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_executions_intent_id ON executions(intent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_executions_capability_id ON executions(capability_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_executions_state ON executions(state)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS proposals (
                proposal_id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                server_name TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                estimated_risk TEXT NOT NULL,
                requested_rollback_class TEXT NOT NULL,
                created_at TEXT NOT NULL,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_proposals_intent_id ON proposals(intent_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS capabilities (
                capability_id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                proposal_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                status TEXT NOT NULL,
                issued_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                revoked_at TEXT,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_capabilities_intent_id ON capabilities(intent_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS rollback_contracts (
                contract_id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                proposal_id TEXT NOT NULL,
                execution_id TEXT NOT NULL,
                adapter_key TEXT NOT NULL,
                action_type TEXT NOT NULL,
                rollback_class TEXT NOT NULL,
                state TEXT NOT NULL,
                auto_commit BOOLEAN NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_rollback_contracts_execution_id ON rollback_contracts(execution_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS approvals (
                approval_id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                proposal_id TEXT NOT NULL,
                execution_id TEXT,
                action_digest TEXT NOT NULL,
                state TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                created_at TEXT NOT NULL,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_approvals_proposal_id ON approvals(proposal_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_approvals_state_created_at ON approvals(state, created_at DESC, approval_id DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS provenance_events (
                event_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                intent_id TEXT,
                proposal_id TEXT,
                execution_id TEXT,
                capability_id TEXT,
                rollback_contract_id TEXT,
                policy_bundle_id TEXT,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_provenance_events_occurred_at ON provenance_events(occurred_at ASC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS provenance_edges (
                to_event_id TEXT NOT NULL,
                from_event_id TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                summary TEXT
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_provenance_edges_to_event_id ON provenance_edges(to_event_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_provenance_edges_from_event_id ON provenance_edges(from_event_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ledger_entries (
                entry_id BIGSERIAL PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE,
                intent_id TEXT,
                execution_id TEXT,
                occurred_at TEXT NOT NULL,
                content_hash TEXT,
                previous_ledger_hash TEXT,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_ledger_entries_occurred_at ON ledger_entries(occurred_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_ledger_entries_intent_id ON ledger_entries(intent_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_ledger_entries_execution_id ON ledger_entries(execution_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS policy_bundles (
                bundle_id TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                active BOOLEAN NOT NULL DEFAULT false,
                content_hash TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                raw_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_policy_bundles_content_hash ON policy_bundles(content_hash)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_policy_bundles_active ON policy_bundles(active)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub fn intents(&self) -> PostgresIntentRepo {
        PostgresIntentRepo::new(self.pool.clone())
    }

    pub fn proposals(&self) -> PostgresProposalRepo {
        PostgresProposalRepo::new(self.pool.clone())
    }

    pub fn capabilities(&self) -> PostgresCapabilityRepo {
        PostgresCapabilityRepo::new(self.pool.clone())
    }

    pub fn executions(&self) -> PostgresExecutionRepo {
        PostgresExecutionRepo::new(self.pool.clone())
    }

    pub fn rollback_contracts(&self) -> PostgresRollbackRepo {
        PostgresRollbackRepo::new(self.pool.clone())
    }

    pub fn approvals(&self) -> PostgresApprovalRepo {
        PostgresApprovalRepo::new(self.pool.clone())
    }

    pub fn provenance(&self) -> PostgresProvenanceRepo {
        PostgresProvenanceRepo::new(self.pool.clone())
    }

    pub fn ledger(&self) -> PostgresLedgerRepo {
        PostgresLedgerRepo::new(self.pool.clone())
    }

    pub fn policy_bundles(&self) -> PostgresPolicyBundleRepo {
        PostgresPolicyBundleRepo::new(self.pool.clone())
    }
}

#[async_trait]
impl StoreFacade for PostgresStore {
    fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
        Arc::new(self.capabilities())
    }

    fn executions(&self) -> Arc<dyn ExecutionRepo> {
        Arc::new(self.executions())
    }

    fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
        Arc::new(self.rollback_contracts())
    }

    fn approvals(&self) -> Arc<dyn ApprovalRepo> {
        Arc::new(self.approvals())
    }

    fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
        Arc::new(self.provenance())
    }

    fn ledger(&self) -> Arc<dyn LedgerRepo> {
        Arc::new(self.ledger())
    }

    fn intents(&self) -> Arc<dyn IntentRepo> {
        Arc::new(self.intents())
    }

    fn proposals(&self) -> Arc<dyn ProposalRepo> {
        Arc::new(self.proposals())
    }

    fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
        Arc::new(self.policy_bundles())
    }

    async fn health_check(&self) -> crate::Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| crate::StoreError::Other(e.to_string()))?;
        Ok(())
    }

    fn write_queue_depth(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn postgres_store_connect_errors_when_unreachable() {
        let err = PostgresStore::connect("postgres://localhost:5432/test")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::StoreError::Database(_)),
            "expected database error for unreachable host, got: {}",
            err
        );
    }

    #[test]
    fn postgres_intent_repo_implements_intent_repo() {
        fn _check<T: IntentRepo>() {}
        _check::<PostgresIntentRepo>();
    }

    #[test]
    fn postgres_store_implements_store_facade() {
        fn _check<T: StoreFacade>() {}
        _check::<PostgresStore>();
    }

    #[test]
    fn postgres_proposal_repo_implements_proposal_repo() {
        fn _check<T: ProposalRepo>() {}
        _check::<PostgresProposalRepo>();
    }

    #[test]
    fn postgres_execution_repo_implements_execution_repo() {
        fn _check<T: ExecutionRepo>() {}
        _check::<PostgresExecutionRepo>();
    }

    #[test]
    fn postgres_capability_repo_implements_capability_repo() {
        fn _check<T: CapabilityRepo>() {}
        _check::<PostgresCapabilityRepo>();
    }

    #[test]
    fn postgres_rollback_repo_implements_rollback_repo() {
        fn _check<T: RollbackRepo>() {}
        _check::<PostgresRollbackRepo>();
    }

    #[test]
    fn postgres_approval_repo_implements_approval_repo() {
        fn _check<T: ApprovalRepo>() {}
        _check::<PostgresApprovalRepo>();
    }

    #[test]
    fn postgres_provenance_repo_implements_provenance_repo() {
        fn _check<T: ProvenanceRepo>() {}
        _check::<PostgresProvenanceRepo>();
    }

    #[test]
    fn postgres_ledger_repo_implements_ledger_repo() {
        fn _check<T: LedgerRepo>() {}
        _check::<PostgresLedgerRepo>();
    }

    #[test]
    fn postgres_policy_bundle_repo_implements_policy_bundle_repo() {
        fn _check<T: PolicyBundleRepo>() {}
        _check::<PostgresPolicyBundleRepo>();
    }
}
