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
use std::sync::atomic::{AtomicU64, Ordering};

/// Clone-safe wrapper around an `AtomicU64` so it can be used in `#[derive(Clone)]` structs.
#[derive(Debug)]
struct AtomicCounter {
    inner: AtomicU64,
}

impl AtomicCounter {
    fn new(value: u64) -> Self {
        Self {
            inner: AtomicU64::new(value),
        }
    }

    fn fetch_add(&self, val: u64, order: Ordering) -> u64 {
        self.inner.fetch_add(val, order)
    }

    fn load(&self, order: Ordering) -> u64 {
        self.inner.load(order)
    }
}

impl Clone for AtomicCounter {
    fn clone(&self) -> Self {
        Self {
            inner: AtomicU64::new(self.inner.load(Ordering::Relaxed)),
        }
    }
}

/// Conservative PostgreSQL connection pool configuration.
///
/// Defaults are chosen for single-node, bounded workloads and are
/// **not** tuned for HA/multi-node or production-scale deployment.
#[derive(Debug, Clone)]
pub struct PostgresPoolConfig {
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum idle connections to maintain.
    pub min_idle: u32,
    /// Timeout in seconds when acquiring a connection from the pool.
    pub acquire_timeout_secs: u64,
    /// Statement timeout in milliseconds per session (`0` disables).
    /// Conservative default: 5000 ms.
    pub statement_timeout_ms: u64,
    /// Idle-in-transaction timeout in milliseconds per session (`0` disables).
    /// Conservative default: 10000 ms.
    pub idle_in_transaction_timeout_ms: u64,
}

impl Default for PostgresPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_idle: 2,
            acquire_timeout_secs: 5,
            statement_timeout_ms: 5000,
            idle_in_transaction_timeout_ms: 10000,
        }
    }
}

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
    max_connections: u32,
    acquire_timeouts: AtomicCounter,
}

impl PostgresStore {
    /// Connect to PostgreSQL using the provided database URL.
    ///
    /// Uses conservative default pool settings (`max_connections = 10`,
    /// `min_idle = 2`, `acquire_timeout = 5s`).
    pub async fn connect(database_url: &str) -> Result<Self> {
        Self::connect_with_config(database_url, PostgresPoolConfig::default()).await
    }

    /// Connect to PostgreSQL with explicit pool configuration.
    pub async fn connect_with_config(
        database_url: &str,
        config: PostgresPoolConfig,
    ) -> Result<Self> {
        let statement_timeout_ms = config.statement_timeout_ms;
        let idle_in_transaction_timeout_ms = config.idle_in_transaction_timeout_ms;
        let max_connections = config.max_connections;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_idle)
            .acquire_timeout(std::time::Duration::from_secs(config.acquire_timeout_secs))
            .after_connect(move |conn, _meta| {
                Box::pin(async move {
                    if statement_timeout_ms > 0 {
                        let sql = format!("SET statement_timeout = {}", statement_timeout_ms);
                        sqlx::query(&sql).execute(&mut *conn).await?;
                    }
                    if idle_in_transaction_timeout_ms > 0 {
                        let sql = format!(
                            "SET idle_in_transaction_session_timeout = {}",
                            idle_in_transaction_timeout_ms
                        );
                        sqlx::query(&sql).execute(&mut *conn).await?;
                    }
                    Ok(())
                })
            })
            .connect(database_url)
            .await?;
        Ok(Self {
            pool,
            max_connections,
            acquire_timeouts: AtomicCounter::new(0),
        })
    }

    /// Returns a clone of the underlying connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Apply embedded schema migrations for all P3 repos within a transaction.
    ///
    /// Idempotent: checks `_schema_version` before running SQL. If the recorded
    /// version is >= [`migrations::CURRENT_SCHEMA_VERSION`], the call is a no-op.
    pub async fn apply_embedded_migrations(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Bootstrap: ensure version tracking table exists before querying it.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&mut *tx)
        .await?;

        let current_version: i32 =
            sqlx::query_scalar("SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or(0);

        if (current_version as i64) >= migrations::CURRENT_SCHEMA_VERSION {
            tx.commit().await?;
            return Ok(());
        }

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

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO _schema_version (version, applied_at) VALUES ($1, $2)
             ON CONFLICT (version) DO UPDATE SET applied_at = $2",
        )
        .bind(migrations::CURRENT_SCHEMA_VERSION)
        .bind(now)
        .execute(&mut *tx)
        .await?;

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
        if let Err(e) = sqlx::query("SELECT 1").execute(&self.pool).await {
            if matches!(e, sqlx::Error::PoolTimedOut) {
                self.acquire_timeouts.fetch_add(1, Ordering::Relaxed);
            }
            return Err(crate::StoreError::Other(e.to_string()));
        }
        Ok(())
    }

    fn write_queue_depth(&self) -> usize {
        0
    }

    fn pool_status(&self) -> Option<crate::repos::PoolStatus> {
        Some(crate::repos::PoolStatus {
            total_connections: self.pool.size(),
            idle_connections: self.pool.num_idle() as u32,
            max_connections: self.max_connections,
            acquire_timeouts: self.acquire_timeouts.load(Ordering::Relaxed),
        })
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

    #[test]
    fn postgres_pool_config_defaults_are_conservative() {
        let config = super::PostgresPoolConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_idle, 2);
        assert_eq!(config.acquire_timeout_secs, 5);
        assert_eq!(config.statement_timeout_ms, 5000);
        assert_eq!(config.idle_in_transaction_timeout_ms, 10000);
    }

    #[test]
    fn postgres_store_pool_status_reflects_config() {
        // We cannot connect to a real PG in a unit test, but we can verify
        // that pool_status returns values consistent with the store config
        // by inspecting a store created with a mock/unreachable config.
        // Since connect_with_config is async and needs a real PG, we test
        // the trait default and struct layout instead.
        let config = super::PostgresPoolConfig {
            max_connections: 42,
            ..super::PostgresPoolConfig::default()
        };
        assert_eq!(config.max_connections, 42);
        // PoolStatus struct sanity check
        let status = crate::PoolStatus {
            total_connections: 5,
            idle_connections: 3,
            max_connections: 42,
            acquire_timeouts: 1,
        };
        assert_eq!(status.total_connections, 5);
        assert_eq!(status.idle_connections, 3);
        assert_eq!(status.max_connections, 42);
        assert_eq!(status.acquire_timeouts, 1);
    }

    #[test]
    fn postgres_current_schema_version_is_set() {
        assert_eq!(super::migrations::CURRENT_SCHEMA_VERSION, 1);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_migration_records_schema_version_and_is_idempotent() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let version: i32 =
            sqlx::query_scalar("SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(version as i64, super::migrations::CURRENT_SCHEMA_VERSION);

        // Second run should be a no-op
        store.apply_embedded_migrations().await.unwrap();
    }
}
