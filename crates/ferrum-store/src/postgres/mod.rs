//! PostgreSQL P2 infrastructure skeleton.
//!
//! **This module is a compile-time infrastructure placeholder only.**
//! Runtime PostgreSQL support is NOT implemented. All operations return an error
//! indicating this is a skeleton.
//!
//! # P2 Status
//!
//! - [x] Module skeleton with placeholder repo implementations
//! - [ ] Real repository implementations (P3)
//! - [ ] Migration infrastructure (P4)
//! - [ ] Production readiness (P5)
//!
//! See ADR-50 for the phased implementation plan.

mod approvals;
mod capabilities;
mod executions;
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
use std::sync::Arc;

/// PostgreSQL P2 skeleton store.
///
/// **This is NOT functional runtime code.**
/// All operations return `StoreError::Other("PostgreSQL P2 skeleton only;
/// runtime support not implemented")`.
///
/// Use `SqliteStore` for actual runtime storage.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    _private: (),
}

impl PostgresStore {
    /// Creates a new PostgresStore skeleton.
    ///
    /// **NOTE**: This does NOT connect to PostgreSQL. It returns a skeleton
    /// that errors on all operations.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Returns a placeholder for when real PostgreSQL connection is implemented.
    ///
    /// # Errors
    ///
    /// Always returns `StoreError::Other("PostgreSQL P2 skeleton only;
    /// runtime support not implemented")`.
    pub async fn connect(_database_url: &str) -> Result<Self> {
        Err(crate::StoreError::Other(
            "PostgreSQL P2 skeleton only; runtime support not implemented".to_string(),
        ))
    }

    // Placeholder accessors — return skeleton repos that error on all operations

    pub fn intents(&self) -> PostgresIntentRepo {
        PostgresIntentRepo::new()
    }

    pub fn proposals(&self) -> PostgresProposalRepo {
        PostgresProposalRepo::new()
    }

    pub fn capabilities(&self) -> PostgresCapabilityRepo {
        PostgresCapabilityRepo::new()
    }

    pub fn executions(&self) -> PostgresExecutionRepo {
        PostgresExecutionRepo::new()
    }

    pub fn rollback_contracts(&self) -> PostgresRollbackRepo {
        PostgresRollbackRepo::new()
    }

    pub fn approvals(&self) -> PostgresApprovalRepo {
        PostgresApprovalRepo::new()
    }

    pub fn provenance(&self) -> PostgresProvenanceRepo {
        PostgresProvenanceRepo::new()
    }

    pub fn ledger(&self) -> PostgresLedgerRepo {
        PostgresLedgerRepo::new()
    }

    pub fn policy_bundles(&self) -> PostgresPolicyBundleRepo {
        PostgresPolicyBundleRepo::new()
    }
}

impl Default for PostgresStore {
    fn default() -> Self {
        Self::new()
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
        Err(crate::StoreError::Other(
            "PostgreSQL P2 skeleton only; runtime support not implemented".to_string(),
        ))
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
    async fn postgres_store_health_check_returns_error() {
        let store = PostgresStore::new();
        let err = store.health_check().await.unwrap_err();
        assert!(
            matches!(err, crate::StoreError::Other(ref s) if s.contains("P2 skeleton")),
            "expected P2 skeleton error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn postgres_store_connect_returns_error() {
        let err = PostgresStore::connect("postgres://localhost:5432/test")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::StoreError::Other(ref s) if s.contains("P2 skeleton")),
            "expected P2 skeleton error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn postgres_store_facade_returns_all_repos() {
        let store = PostgresStore::new();
        let facade: Arc<dyn StoreFacade> = Arc::new(store);

        // Verify each accessor returns a valid repo (no panic)
        let _ = facade.capabilities();
        let _ = facade.executions();
        let _ = facade.rollback_contracts();
        let _ = facade.approvals();
        let _ = facade.provenance();
        let _ = facade.ledger();
        let _ = facade.intents();
        let _ = facade.proposals();
        let _ = facade.policy_bundles();

        // All repos error on use
        let err = facade.health_check().await.unwrap_err();
        assert!(
            matches!(err, crate::StoreError::Other(ref s) if s.contains("P2 skeleton")),
            "expected P2 skeleton error, got: {}",
            err
        );
    }
}
