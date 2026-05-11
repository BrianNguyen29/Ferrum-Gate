//! PostgreSQL P3 runtime infrastructure — all repos implemented.
//!
//! **PostgreSQL runtime support is partial (P4.2).**
//! All P3 repos have real implementations: `PostgresIntentRepo`,
//! `PostgresProposalRepo`, `PostgresExecutionRepo`, `PostgresCapabilityRepo`,
//! `PostgresRollbackRepo`, `PostgresApprovalRepo`, `PostgresProvenanceRepo`,
//! `PostgresLedgerRepo`, and `PostgresPolicyBundleRepo`.
//!
//! # P4.2 Status
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
//! - [x] Migration infrastructure (P4.2)
//! - [ ] Production readiness (P5)
//!
//! See ADR-50 for the phased implementation plan.

mod approvals;
mod capabilities;
mod executions;
mod helpers;
mod intents;
mod ledger;
mod migrations;
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

    /// Apply embedded schema migrations for all P3 repos within a transaction.
    pub async fn apply_embedded_migrations(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let mut statement = String::new();

        for line in migrations::INIT_MIGRATION.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }

            statement.push_str(line);
            statement.push('\n');

            if trimmed.ends_with(';') {
                let sql = statement.trim();
                if !sql.is_empty() {
                    sqlx::query(sql).execute(&mut *tx).await?;
                }
                statement.clear();
            }
        }

        let sql = statement.trim();
        if !sql.is_empty() {
            sqlx::query(sql).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Backward-compatible alias for `apply_embedded_migrations`.
    #[deprecated(since = "0.1.0", note = "use `apply_embedded_migrations` instead")]
    pub async fn apply_intent_migration(&self) -> Result<()> {
        self.apply_embedded_migrations().await
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
