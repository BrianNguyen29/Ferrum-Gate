//! PostgreSQL P3 runtime infrastructure — IntentRepo implemented.
//!
//! **PostgreSQL runtime support is partial (P3).**
//! Only `PostgresIntentRepo` has a real implementation.
//! All other repos remain skeletons that return an error.
//!
//! # P3 Status
//!
//! - [x] PostgresIntentRepo with real sqlx queries
//! - [ ] Remaining 8 repos (P3+ deferred)
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
/// **Partial runtime support.** `PostgresIntentRepo` is functional;
/// all other repos return `StoreError::Other("PostgreSQL P2 skeleton only; ...")`.
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

/// Error message used by all skeleton repo operations.
const SKELETON_ERROR: &str = "PostgreSQL P2 skeleton only; runtime support not implemented";

/// Helper to create the skeleton error for repo methods.
fn skeleton_error() -> crate::StoreError {
    crate::StoreError::Other(SKELETON_ERROR.to_string())
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
}
