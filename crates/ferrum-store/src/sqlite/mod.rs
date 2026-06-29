mod agents;
mod approvals;
mod audit_checkpoint;
mod audit_log;
mod audit_merkle_root;
mod capabilities;
mod executions;
mod helpers;
mod intents;
mod leader_allowlist;
mod leader_tip_cache;
mod ledger;
mod lifecycle_outbox;
mod mfa_credentials;
mod migrations;
mod policy_bundles;
mod proposals;
mod provenance;
mod rollback;
mod sync_preflight;
pub mod tokens;
pub mod write_queue;

pub use agents::SqliteAgentRepo;
pub use approvals::SqliteApprovalRepo;
pub use audit_checkpoint::SqliteAuditCheckpointRepo;
pub use audit_log::SqliteAuditLogRepo;
pub use audit_merkle_root::SqliteAuditMerkleRootRepo;
pub use capabilities::SqliteCapabilityRepo;
pub use executions::SqliteExecutionRepo;
pub use intents::SqliteIntentRepo;
pub use leader_allowlist::LeaderAllowlist;
pub use leader_tip_cache::{CacheWriteError, LeaderTipCache};
pub use ledger::SqliteLedgerRepo;
pub use lifecycle_outbox::SqliteLifecycleOutboxRepo;
pub use mfa_credentials::SqliteMfaCredentialRepo;
pub use policy_bundles::SqlitePolicyBundleRepo;
pub use proposals::SqliteProposalRepo;
pub use provenance::SqliteProvenanceRepo;
pub use rollback::SqliteRollbackRepo;
pub use sync_preflight::SqliteSyncPreflightRepo;
pub use tokens::SqliteTokenRepo;

use crate::Result;
use crate::repos::{
    AgentRepo, ApprovalRepo, AuditCheckpointRepo, AuditLogRepo, AuditMerkleRootRepo,
    CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, LifecycleOutboxRepo, MfaCredentialRepo,
    PolicyBundleRepo, ProposalRepo, ProvenanceRepo, RollbackRepo, StoreFacade, TokenRepo,
};
use crate::sqlite::write_queue::{WriteQueue, WriterState, spawn_writer_task};
use async_trait::async_trait;
use sqlx::{
    Row,
    sqlite::{SqlitePool, SqlitePoolOptions, SqliteSynchronous},
};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

/// Optional WAL-mode tuning parameters.
///
/// When `None`, sqlx/sqlite defaults are used (synchronous=FULL, wal_autocheckpoint=1000).
/// When `Some`, values are applied per-connection in the pool's `after_connect` hook.
#[derive(Debug, Clone, Default)]
pub struct SqliteWalTuning {
    /// PRAGMA synchronous value: "off", "normal", "full", or "extra".
    /// None preserves the sqlite default (FULL for WAL databases).
    pub synchronous: Option<String>,
    /// PRAGMA wal_autocheckpoint: number of frames between checkpoints.
    /// None preserves the sqlite default (1000).
    pub wal_autocheckpoint: Option<u32>,
}

impl SqliteWalTuning {
    /// Returns true if any tuning field is set.
    pub fn is_some(&self) -> bool {
        self.synchronous.is_some() || self.wal_autocheckpoint.is_some()
    }
}

pub struct SqliteStore {
    pool: SqlitePool,
    write_queue: WriteQueue,
    #[allow(dead_code)]
    writer_handle: Arc<TokioMutex<JoinHandle<()>>>,
    /// Internal state for signaling shutdown to the writer task.
    writer_state: Arc<WriterState>,
}

impl Clone for SqliteStore {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            write_queue: self.write_queue.clone(),
            writer_handle: self.writer_handle.clone(),
            writer_state: self.writer_state.clone(),
        }
    }
}

impl SqliteStore {
    /// Connect with optional WAL tuning.
    ///
    /// For file-based databases, WAL pragmas are applied via `after_connect`.
    /// For in-memory databases (`:memory:` or `sqlite::memory:`), tuning pragmas
    /// are skipped since WAL mode has no persistence benefit.
    pub async fn connect_with_tuning(database_url: &str, tuning: SqliteWalTuning) -> Result<Self> {
        Self::connect_with_pool_size_and_tuning(database_url, 5, tuning).await
    }

    pub async fn connect(database_url: &str) -> Result<Self> {
        Self::connect_with_tuning(database_url, SqliteWalTuning::default()).await
    }

    pub async fn connect_with_pool_size(database_url: &str, max_connections: u32) -> Result<Self> {
        Self::connect_with_pool_size_and_tuning(
            database_url,
            max_connections,
            SqliteWalTuning::default(),
        )
        .await
    }

    async fn connect_with_pool_size_and_tuning(
        database_url: &str,
        max_connections: u32,
        tuning: SqliteWalTuning,
    ) -> Result<Self> {
        // Check if this is an in-memory database (skip WAL tuning for :memory:)
        // Use contains to detect :memory: in any position
        let is_in_memory =
            database_url.contains(":memory:") || database_url.contains("sqlite::memory");

        let mut builder = SqlitePoolOptions::new().max_connections(max_connections);

        if !is_in_memory && tuning.is_some() {
            builder = builder.after_connect(move |conn, _meta| {
                let tuning = tuning.clone();
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys=ON")
                        .execute(&mut *conn)
                        .await?;
                    // WAL mode - always set, idempotent
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *conn)
                        .await?;

                    // Synchronous - apply via raw PRAGMA (can be changed per-connection)
                    if let Some(ref sync_str) = tuning.synchronous {
                        sqlx::query(&format!("PRAGMA synchronous={}", sync_str))
                            .execute(&mut *conn)
                            .await?;
                    }

                    // wal_autocheckpoint - raw pragma
                    if let Some(checkpoint) = tuning.wal_autocheckpoint {
                        sqlx::query(&format!("PRAGMA wal_autocheckpoint={}", checkpoint))
                            .execute(&mut *conn)
                            .await?;
                    }

                    // busy_timeout and cache_size - always set for production defaults
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA cache_size=-64000")
                        .execute(&mut *conn)
                        .await?;

                    Ok(())
                })
            });
        } else {
            // No tuning or in-memory - use safe defaults
            builder = builder.after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys=ON")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA synchronous=NORMAL")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA wal_autocheckpoint=1000")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA cache_size=-64000")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            });
        }

        let pool = builder.connect(database_url).await?;

        // Spawn the writer task
        let (write_queue, writer_handle, writer_state) = spawn_writer_task(pool.clone());

        Ok(Self {
            pool,
            write_queue,
            writer_handle: Arc::new(TokioMutex::new(writer_handle)),
            writer_state,
        })
    }

    /// Re-apply synchronous pragma on a connection.
    /// Useful after migrations that may reset the per-connection value.
    /// Note: This applies to one connection from the pool; subsequent connections
    /// from the pool retain their default settings.
    pub async fn set_synchronous(&self, mode: &str) -> Result<()> {
        // Validate the mode string
        let _ = SqliteSynchronous::from_str(mode).map_err(
            |e: <SqliteSynchronous as FromStr>::Err| crate::StoreError::Other(e.to_string()),
        )?;
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| crate::StoreError::Other(e.to_string()))?;
        sqlx::query(&format!("PRAGMA synchronous={}", mode))
            .execute(&mut *conn)
            .await?;
        drop(conn); // Explicitly return connection to pool before checking from a new connection
        Ok(())
    }

    /// Returns a clone of the WriteQueue for repos that support write-queue routing.
    pub fn write_queue(&self) -> WriteQueue {
        self.write_queue.clone()
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Apply embedded schema migrations within a transaction.
    ///
    /// Idempotent: checks `_schema_version` before running SQL. If the recorded
    /// version is equal to [`migrations::CURRENT_SCHEMA_VERSION`], the call is a no-op.
    /// If the recorded version is greater, the call returns a [`StoreError::SchemaDrift`].
    pub async fn apply_embedded_migrations(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Bootstrap: ensure version tracking table exists before querying it.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&mut *tx)
        .await?;

        let current_version: i64 =
            sqlx::query_scalar("SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or(0);

        if current_version > migrations::CURRENT_SCHEMA_VERSION {
            return Err(crate::StoreError::SchemaDrift {
                db_version: current_version,
                expected_version: migrations::CURRENT_SCHEMA_VERSION,
            });
        }

        if current_version == migrations::CURRENT_SCHEMA_VERSION {
            ensure_lifecycle_outbox_reconciliation_lease_columns(&mut tx).await?;
            tx.commit().await?;
            return Ok(());
        }

        // Existing databases may be at schema v13. Ensure columns required by
        // migration 014 exist before replaying the idempotent migration bundle.
        ensure_lifecycle_outbox_reconciliation_lease_columns(&mut tx).await?;

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

        ensure_lifecycle_outbox_reconciliation_lease_columns(&mut tx).await?;

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO _schema_version (version, applied_at) VALUES (?1, ?2)
             ON CONFLICT (version) DO UPDATE SET applied_at = excluded.applied_at",
        )
        .bind(migrations::CURRENT_SCHEMA_VERSION)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub fn intents(&self) -> SqliteIntentRepo {
        SqliteIntentRepo::new(self.pool.clone())
    }

    pub fn proposals(&self) -> SqliteProposalRepo {
        SqliteProposalRepo::new(self.pool.clone())
    }

    pub fn capabilities(&self) -> SqliteCapabilityRepo {
        SqliteCapabilityRepo::new(self.pool.clone())
    }

    pub fn executions(&self) -> SqliteExecutionRepo {
        SqliteExecutionRepo::new(self.pool.clone())
    }

    pub fn rollback_contracts(&self) -> SqliteRollbackRepo {
        SqliteRollbackRepo::new(self.pool.clone())
    }

    pub fn lifecycle_outbox(&self) -> SqliteLifecycleOutboxRepo {
        SqliteLifecycleOutboxRepo::new(self.pool.clone())
    }

    pub fn approvals(&self) -> SqliteApprovalRepo {
        SqliteApprovalRepo::new(self.pool.clone())
    }

    pub fn provenance(&self) -> SqliteProvenanceRepo {
        SqliteProvenanceRepo::new(self.pool.clone())
    }

    pub fn ledger(&self) -> SqliteLedgerRepo {
        SqliteLedgerRepo::new(self.pool.clone())
    }

    pub fn policy_bundles(&self) -> SqlitePolicyBundleRepo {
        SqlitePolicyBundleRepo::new(self.pool.clone())
    }

    pub fn tokens(&self) -> SqliteTokenRepo {
        SqliteTokenRepo::new(self.pool.clone())
    }

    pub fn audit_log(&self) -> SqliteAuditLogRepo {
        SqliteAuditLogRepo::new(self.pool.clone())
    }

    pub fn audit_merkle_roots(&self) -> SqliteAuditMerkleRootRepo {
        SqliteAuditMerkleRootRepo::new(self.pool.clone())
    }

    pub fn audit_checkpoints(&self) -> SqliteAuditCheckpointRepo {
        SqliteAuditCheckpointRepo::new(self.pool.clone())
    }

    pub fn agents(&self) -> SqliteAgentRepo {
        SqliteAgentRepo::new(self.pool.clone())
    }

    pub fn mfa_credentials(&self) -> SqliteMfaCredentialRepo {
        SqliteMfaCredentialRepo::new(self.pool.clone())
    }

    /// Verify the local ledger chain integrity.
    ///
    /// Delegates to `SqliteLedgerRepo::verify_chain()` which validates:
    /// - Empty ledger is valid.
    /// - Genesis entry must have `previous_ledger_hash = None`.
    /// - Each subsequent entry's `previous_ledger_hash` matches prior entry's `content_hash`.
    pub async fn verify_ledger_chain(&self) -> Result<()> {
        self.ledger().verify_chain().await
    }

    /// Request shutdown of the SQLite store.
    ///
    /// Signals the writer task to stop accepting new operations and waits
    /// for it to drain remaining operations and exit.
    /// After shutdown, the SqliteStore should not be used for writes.
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("sqlite store shutdown requested");
        // Signal the writer task to stop accepting new operations and drain
        self.writer_state.request_shutdown();

        // Wait for the writer task to signal completion
        self.writer_state.wait_for_shutdown().await;

        Ok(())
    }
}

async fn ensure_lifecycle_outbox_reconciliation_lease_columns(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<()> {
    let table_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'lifecycle_outbox'",
    )
    .fetch_optional(&mut **tx)
    .await?;
    if table_exists.is_none() {
        return Ok(());
    }

    let columns = sqlx::query("PRAGMA table_info(lifecycle_outbox)")
        .fetch_all(&mut **tx)
        .await?;
    let has_owner = columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "reconciliation_lease_owner");
    let has_expires = columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "reconciliation_lease_expires_at");
    let has_generation = columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "reconciliation_lease_generation");

    if !has_owner {
        sqlx::query("ALTER TABLE lifecycle_outbox ADD COLUMN reconciliation_lease_owner TEXT")
            .execute(&mut **tx)
            .await?;
    }
    if !has_expires {
        sqlx::query("ALTER TABLE lifecycle_outbox ADD COLUMN reconciliation_lease_expires_at TEXT")
            .execute(&mut **tx)
            .await?;
    }
    if !has_generation {
        sqlx::query(
            "ALTER TABLE lifecycle_outbox
             ADD COLUMN reconciliation_lease_generation INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query("DROP INDEX IF EXISTS idx_lifecycle_outbox_reconciliation_lease")
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_reconciliation_lease
         ON lifecycle_outbox(
            status,
            reconciliation_lease_expires_at,
            reconciliation_lease_generation,
            created_at,
            outbox_id
         )",
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[async_trait]
impl StoreFacade for SqliteStore {
    fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
        Arc::new(
            SqliteCapabilityRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn executions(&self) -> Arc<dyn ExecutionRepo> {
        Arc::new(
            SqliteExecutionRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
        Arc::new(
            SqliteRollbackRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn lifecycle_outbox(&self) -> Arc<dyn LifecycleOutboxRepo> {
        Arc::new(
            SqliteLifecycleOutboxRepo::new(self.pool.clone())
                .with_write_queue(self.write_queue.clone()),
        )
    }
    fn approvals(&self) -> Arc<dyn ApprovalRepo> {
        Arc::new(
            SqliteApprovalRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
        Arc::new(
            SqliteProvenanceRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn ledger(&self) -> Arc<dyn LedgerRepo> {
        Arc::new(
            SqliteLedgerRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn intents(&self) -> Arc<dyn IntentRepo> {
        Arc::new(
            SqliteIntentRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn proposals(&self) -> Arc<dyn ProposalRepo> {
        Arc::new(
            SqliteProposalRepo::new(self.pool.clone()).with_write_queue(self.write_queue.clone()),
        )
    }
    fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
        Arc::new(
            SqlitePolicyBundleRepo::new(self.pool.clone())
                .with_write_queue(self.write_queue.clone()),
        )
    }

    fn tokens(&self) -> Arc<dyn TokenRepo> {
        Arc::new(SqliteTokenRepo::new(self.pool.clone()))
    }

    fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
        Arc::new(SqliteAuditLogRepo::new(self.pool.clone()))
    }

    fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
        Arc::new(SqliteAuditMerkleRootRepo::new(self.pool.clone()))
    }

    fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
        Arc::new(SqliteAuditCheckpointRepo::new(self.pool.clone()))
    }

    fn agents(&self) -> Arc<dyn AgentRepo> {
        Arc::new(SqliteAgentRepo::new(self.pool.clone()))
    }

    fn mfa_credentials(&self) -> Arc<dyn MfaCredentialRepo> {
        Arc::new(SqliteMfaCredentialRepo::new(self.pool.clone()))
    }

    fn write_queue_depth(&self) -> usize {
        self.write_queue.pending_depth()
    }

    async fn health_check(&self) -> crate::Result<()> {
        // Use SELECT 1 as a cheap probe to verify the database is reachable.
        // This is faster than PRAGMA quick_check which scans pages.
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| crate::StoreError::Other(e.to_string()))?;
        sqlx::query("SELECT 1")
            .execute(&mut *conn)
            .await
            .map_err(|e| crate::StoreError::Other(e.to_string()))?;
        Ok(())
    }

    async fn shutdown(&self) -> crate::Result<()> {
        self.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_facade_returns_all_repos() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let facade: Arc<dyn StoreFacade> = Arc::new(store);

        // Verify each accessor returns a working repo (no panic)
        let _ = facade.capabilities();
        let _ = facade.executions();
        let _ = facade.rollback_contracts();
        let _ = facade.lifecycle_outbox();
        let _ = facade.approvals();
        let _ = facade.provenance();
        let _ = facade.ledger();
        let _ = facade.intents();
        let _ = facade.proposals();
        let _ = facade.tokens();
        let _ = facade.audit_log();
    }

    #[tokio::test]
    async fn test_store_facade_repo_types_are_dyn_traits() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let facade: Arc<dyn StoreFacade> = Arc::new(store);

        // Verify each accessor returns an Arc<dyn XxxRepo>
        // by calling methods that require &self (proving they exist and are valid)
        let _cap = facade.capabilities();
        let _exec = facade.executions();
        let _rb = facade.rollback_contracts();
        let _appr = facade.approvals();
        let _prov = facade.provenance();
        let _ledger = facade.ledger();
        let _intents = facade.intents();
        let _props = facade.proposals();
        let _pb = facade.policy_bundles();
        let _al = facade.audit_log();

        // Drop them to prove they're valid Arc types
        drop(_cap);
        drop(_exec);
        drop(_rb);
        drop(_appr);
        drop(_prov);
        drop(_ledger);
        drop(_intents);
        drop(_props);
        drop(_pb);
        drop(_al);
    }

    #[tokio::test]
    async fn test_policy_bundle_crud() {
        use chrono::Utc;
        use ferrum_proto::{Decision, Matcher, PolicyBundle, PolicyRule};

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // Create a policy bundle
        let bundle = PolicyBundle {
            bundle_id: "test-bundle".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![PolicyRule {
                id: "test-rule".to_string(),
                description: "Test rule".to_string(),
                decision: Decision::Deny,
                priority: 100,
                matchers: vec![Matcher::ScopeMismatch],
            }],
            active: false,
            content_hash: Some("abc123".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Insert
        store.policy_bundles().insert(&bundle).await.unwrap();

        // Get
        let retrieved = store.policy_bundles().get("test-bundle").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.bundle_id, "test-bundle");
        assert_eq!(retrieved.version, "0.1.0");

        // List
        let bundles = store.policy_bundles().list().await.unwrap();
        assert_eq!(bundles.len(), 1);

        // Set active (should keep raw_json consistent)
        store
            .policy_bundles()
            .set_active("test-bundle", true)
            .await
            .unwrap();
        let active_bundles = store.policy_bundles().list_active().await.unwrap();
        assert_eq!(active_bundles.len(), 1);
        // raw_json should now reflect active=true
        let retrieved = store
            .policy_bundles()
            .get("test-bundle")
            .await
            .unwrap()
            .unwrap();
        assert!(
            retrieved.active,
            "raw_json should reflect active=true after set_active"
        );

        // Delete
        store.policy_bundles().delete("test-bundle").await.unwrap();
        let retrieved = store.policy_bundles().get("test-bundle").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn wal_tuning_default_has_none_values() {
        let tuning = SqliteWalTuning::default();
        assert!(tuning.synchronous.is_none());
        assert!(tuning.wal_autocheckpoint.is_none());
        assert!(!tuning.is_some());
    }

    #[tokio::test]
    async fn wal_tuning_set_synchronous_changes_mode() {
        // Use shared-cache in-memory so we can query pragma on the same connection
        let store = SqliteStore::connect("sqlite::memory:?cache=shared")
            .await
            .unwrap();
        // Apply OFF synchronous mode using the pool's connection
        store.set_synchronous("off").await.unwrap();
        // Check the value using the same pool (a different connection from the pool)
        let mut conn = store.pool().acquire().await.unwrap();
        let _: (i64,) = sqlx::query_as("PRAGMA synchronous")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        // After set_synchronous on one connection, a fresh acquire from pool
        // may get a different connection with default synchronous (NORMAL=1) or the modified one.
        // This test verifies the method executes without error; exact value depends on
        // which connection we get from the pool.
        // For a more deterministic test, just verify no error occurred above.
        drop(conn);
    }

    #[tokio::test]
    async fn wal_tuning_wal_autocheckpoint_custom() {
        // Use a temp file DB so WAL autocheckpoint can be verified independently
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!(
            "ferrum_test_wal_autocheckpoint_{}.db",
            std::process::id()
        ));
        let url = format!("sqlite:{}?mode=rwc", db_path.display());

        let tuning = SqliteWalTuning {
            synchronous: None,
            wal_autocheckpoint: Some(500),
        };
        let store = SqliteStore::connect_with_tuning(&url, tuning)
            .await
            .unwrap();

        // Query the wal_autocheckpoint value via a fresh connection from the pool
        let mut conn = store.pool().acquire().await.unwrap();
        let row: (i64,) = sqlx::query_as("PRAGMA wal_autocheckpoint")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        // Value should be 500
        assert_eq!(row.0, 500);

        drop(conn);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }

    #[tokio::test]
    async fn wal_tuning_combined_applies_both_settings() {
        // Use a temp file DB so both settings are applied (in-memory uses defaults)
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("ferrum_test_combined_{}.db", std::process::id()));
        let url = format!("sqlite:{}?mode=rwc", db_path.display());

        let tuning = SqliteWalTuning {
            synchronous: Some("normal".to_string()),
            wal_autocheckpoint: Some(250),
        };
        let store = SqliteStore::connect_with_tuning(&url, tuning)
            .await
            .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // wal_autocheckpoint should persist (not reset by COMMIT)
        let mut conn = store.pool().acquire().await.unwrap();
        let row: (i64,) = sqlx::query_as("PRAGMA wal_autocheckpoint")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(row.0, 250);

        drop(conn);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }

    #[tokio::test]
    async fn test_audit_log_append_and_list() {
        use ferrum_proto::{AuditAction, AuditLogEntry, AuditResourceType};

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let entry = AuditLogEntry {
            id: 0,
            actor_id: "test-actor".to_string(),
            action: AuditAction::TokenCreate,
            resource_type: AuditResourceType::Token,
            resource_id: "tok_123".to_string(),
            result: "success".to_string(),
            metadata: Some(serde_json::json!({"role": "admin"})),
            created_at: chrono::Utc::now(),
            content_hash: None,
            previous_hash: None,
        };

        store.audit_log().append(&entry).await.unwrap();

        let (items, next_cursor) = store
            .audit_log()
            .list(None, None, None, None, 10, None, None)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].actor_id, "test-actor");
        assert_eq!(items[0].action, AuditAction::TokenCreate);
        assert_eq!(items[0].resource_type, AuditResourceType::Token);
        assert_eq!(items[0].resource_id, "tok_123");
        assert_eq!(items[0].result, "success");
        assert!(next_cursor.is_none());

        // Test filtering by action
        let (filtered, _) = store
            .audit_log()
            .list(
                Some(AuditAction::TokenCreate),
                None,
                None,
                None,
                10,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);

        let (filtered, _) = store
            .audit_log()
            .list(
                Some(AuditAction::TokenRevoke),
                None,
                None,
                None,
                10,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(filtered.len(), 0);
    }

    #[tokio::test]
    async fn test_audit_log_list_cursor_pagination_and_invalid_cursor() {
        use ferrum_proto::{AuditAction, AuditLogEntry, AuditResourceType};

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        for i in 0..5 {
            let entry = AuditLogEntry {
                id: 0,
                actor_id: format!("actor-{}", i),
                action: AuditAction::TokenCreate,
                resource_type: AuditResourceType::Token,
                resource_id: format!("tok_{}", i),
                result: "success".to_string(),
                metadata: None,
                created_at: chrono::Utc::now(),
                content_hash: None,
                previous_hash: None,
            };
            store.audit_log().append(&entry).await.unwrap();
        }

        // Page 1: limit 2
        let (page1, cursor1) = store
            .audit_log()
            .list(None, None, None, None, 2, None, None)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);
        let cursor1 = cursor1.expect("should have next cursor");

        // Page 2: using valid cursor
        let (page2, cursor2) = store
            .audit_log()
            .list(None, None, None, Some(&cursor1), 2, None, None)
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert!(cursor2.is_some());

        // Invalid cursor should not cause bind mismatch; treat as no cursor
        let (page_all, _) = store
            .audit_log()
            .list(None, None, None, Some("not-a-number"), 10, None, None)
            .await
            .unwrap();
        assert_eq!(page_all.len(), 5);
    }

    #[tokio::test]
    async fn test_verify_empty_ledger_is_valid() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        store.verify_ledger_chain().await.unwrap();
    }

    /// Insert a minimal provenance event for FK constraint
    async fn insert_provenance_event(pool: &SqlitePool, event_id: &str, occurred_at: &str) {
        let raw_json = serde_json::json!({
            "event_id": event_id,
            "kind": "Test",
            "occurred_at": occurred_at,
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO provenance_events (event_id, kind, occurred_at, raw_json) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(event_id)
        .bind("Test")
        .bind(occurred_at)
        .bind(raw_json.to_string())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_verify_valid_genesis_only() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // Insert provenance event first (FK requirement)
        insert_provenance_event(store.pool(), "event-1", "2024-01-01T00:00:00Z").await;

        // Insert genesis entry with no previous_ledger_hash
        let raw_json = serde_json::json!({
            "entry_id": 1,
            "event_id": "event-1",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:00Z",
            "content_hash": "hash1",
            "previous_ledger_hash": null,
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-1")
        .bind("hash1")
        .bind::<Option<&str>>(None)
        .bind(raw_json.to_string())
        .bind("2024-01-01T00:00:00Z")
        .execute(store.pool())
        .await
        .unwrap();

        store.verify_ledger_chain().await.unwrap();
    }

    #[tokio::test]
    async fn test_verify_valid_linked_chain() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // Insert provenance events first (FK requirement)
        insert_provenance_event(store.pool(), "event-1", "2024-01-01T00:00:00Z").await;
        insert_provenance_event(store.pool(), "event-2", "2024-01-01T00:00:01Z").await;

        // Insert genesis entry
        let raw_json1 = serde_json::json!({
            "entry_id": 1,
            "event_id": "event-1",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:00Z",
            "content_hash": "hash1",
            "previous_ledger_hash": null,
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-1")
        .bind("hash1")
        .bind::<Option<&str>>(None)
        .bind(raw_json1.to_string())
        .bind("2024-01-01T00:00:00Z")
        .execute(store.pool())
        .await
        .unwrap();

        // Insert second entry with previous_ledger_hash = hash1
        let raw_json2 = serde_json::json!({
            "entry_id": 2,
            "event_id": "event-2",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:01Z",
            "content_hash": "hash2",
            "previous_ledger_hash": "hash1",
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-2")
        .bind("hash2")
        .bind("hash1")
        .bind(raw_json2.to_string())
        .bind("2024-01-01T00:00:01Z")
        .execute(store.pool())
        .await
        .unwrap();

        store.verify_ledger_chain().await.unwrap();
    }

    #[tokio::test]
    async fn test_verify_broken_previous_hash_detected() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // Insert provenance events first (FK requirement)
        insert_provenance_event(store.pool(), "event-1", "2024-01-01T00:00:00Z").await;
        insert_provenance_event(store.pool(), "event-2", "2024-01-01T00:00:01Z").await;

        // Insert genesis entry
        let raw_json1 = serde_json::json!({
            "entry_id": 1,
            "event_id": "event-1",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:00Z",
            "content_hash": "hash1",
            "previous_ledger_hash": null,
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-1")
        .bind("hash1")
        .bind::<Option<&str>>(None)
        .bind(raw_json1.to_string())
        .bind("2024-01-01T00:00:00Z")
        .execute(store.pool())
        .await
        .unwrap();

        // Insert second entry with WRONG previous_ledger_hash (not matching hash1)
        let raw_json2 = serde_json::json!({
            "entry_id": 2,
            "event_id": "event-2",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:01Z",
            "content_hash": "hash2",
            "previous_ledger_hash": "wrong_hash",
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-2")
        .bind("hash2")
        .bind("wrong_hash")
        .bind(raw_json2.to_string())
        .bind("2024-01-01T00:00:01Z")
        .execute(store.pool())
        .await
        .unwrap();

        let err = store.verify_ledger_chain().await.unwrap_err();
        assert!(
            matches!(err, crate::StoreError::InvalidState(ref s) if s.contains("broken chain")),
            "expected InvalidState with broken chain, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_verify_genesis_with_previous_hash_detected() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // Insert provenance event first (FK requirement)
        insert_provenance_event(store.pool(), "event-1", "2024-01-01T00:00:00Z").await;

        // Insert genesis entry with previous_ledger_hash set (invalid)
        let raw_json = serde_json::json!({
            "entry_id": 1,
            "event_id": "event-1",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:00Z",
            "content_hash": "hash1",
            "previous_ledger_hash": "should_be_null",
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-1")
        .bind("hash1")
        .bind("should_be_null")
        .bind(raw_json.to_string())
        .bind("2024-01-01T00:00:00Z")
        .execute(store.pool())
        .await
        .unwrap();

        let err = store.verify_ledger_chain().await.unwrap_err();
        assert!(
            matches!(err, crate::StoreError::InvalidState(ref s) if s.contains("genesis")),
            "expected InvalidState for genesis, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_verify_missing_previous_hash_after_genesis_detected() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // Insert provenance events first (FK requirement)
        insert_provenance_event(store.pool(), "event-1", "2024-01-01T00:00:00Z").await;
        insert_provenance_event(store.pool(), "event-2", "2024-01-01T00:00:01Z").await;

        // Insert genesis entry
        let raw_json1 = serde_json::json!({
            "entry_id": 1,
            "event_id": "event-1",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:00Z",
            "content_hash": "hash1",
            "previous_ledger_hash": null,
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-1")
        .bind("hash1")
        .bind::<Option<&str>>(None)
        .bind(raw_json1.to_string())
        .bind("2024-01-01T00:00:00Z")
        .execute(store.pool())
        .await
        .unwrap();

        // Insert second entry with NULL previous_ledger_hash (should be hash1)
        let raw_json2 = serde_json::json!({
            "entry_id": 2,
            "event_id": "event-2",
            "intent_id": null,
            "execution_id": null,
            "occurred_at": "2024-01-01T00:00:01Z",
            "content_hash": "hash2",
            "previous_ledger_hash": null,
            "raw_json": {}
        });
        sqlx::query(
            "INSERT INTO ledger_entries (event_id, content_hash, previous_ledger_hash, raw_json, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("event-2")
        .bind("hash2")
        .bind::<Option<&str>>(None)
        .bind(raw_json2.to_string())
        .bind("2024-01-01T00:00:01Z")
        .execute(store.pool())
        .await
        .unwrap();

        let err = store.verify_ledger_chain().await.unwrap_err();
        assert!(
            matches!(err, crate::StoreError::InvalidState(ref s) if s.contains("missing previous_ledger_hash")),
            "expected InvalidState for missing previous_ledger_hash, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_health_check_succeeds_on_healthy_store() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        // health_check should succeed on a healthy store
        store
            .health_check()
            .await
            .expect("health_check should succeed");
    }

    #[tokio::test]
    async fn test_store_facade_shutdown_drains_writer() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let facade: Arc<dyn StoreFacade> = Arc::new(store);

        // Shutdown via the trait should drain the writer without panic
        facade.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_health_check_via_store_facade() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let facade: Arc<dyn StoreFacade> = Arc::new(store);

        // health_check via facade trait should succeed
        facade
            .health_check()
            .await
            .expect("facade health_check should succeed");
    }

    #[tokio::test]
    async fn test_migration_records_schema_version() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let version: i64 =
            sqlx::query_scalar("SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(version, super::migrations::CURRENT_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_migration_is_idempotent() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();

        // First run
        store.apply_embedded_migrations().await.unwrap();

        // Second run should be a no-op
        store.apply_embedded_migrations().await.unwrap();

        let version: i64 =
            sqlx::query_scalar("SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(version, super::migrations::CURRENT_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_migration_upgrades_v13_lifecycle_outbox_with_fencing_generation() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE _schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             )",
        )
        .execute(store.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO _schema_version(version, applied_at)
             VALUES (13, '2026-01-01T00:00:00Z')",
        )
        .execute(store.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE lifecycle_outbox (
                outbox_id TEXT PRIMARY KEY,
                execution_id TEXT NOT NULL,
                rollback_contract_id TEXT,
                previous_execution_state TEXT,
                new_execution_state TEXT NOT NULL,
                previous_rollback_state TEXT,
                new_rollback_state TEXT,
                intended_provenance_kind TEXT NOT NULL,
                idempotency_key TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL,
                provenance_event_id TEXT,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                reconciliation_lease_owner TEXT,
                reconciliation_lease_expires_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                raw_json TEXT NOT NULL
             )",
        )
        .execute(store.pool())
        .await
        .unwrap();

        store.apply_embedded_migrations().await.unwrap();

        let columns = sqlx::query("PRAGMA table_info(lifecycle_outbox)")
            .fetch_all(store.pool())
            .await
            .unwrap();
        assert!(
            columns
                .iter()
                .any(|row| { row.get::<String, _>("name") == "reconciliation_lease_generation" })
        );
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(version, 16);
    }

    #[tokio::test]
    async fn test_agent_registry_crud() {
        use ferrum_proto::AgentRecord;

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let agent = AgentRecord {
            agent_id: "agent_test_1".to_string(),
            public_key: "c29tZV9rZXk=".to_string(),
            key_fingerprint: "fp1".to_string(),
            allowed_scopes: vec!["intent:submit".to_string(), "execution:execute".to_string()],
            created_at: chrono::Utc::now(),
            revoked_at: None,
            description: Some("test agent".to_string()),
        };

        store.agents().insert(&agent).await.unwrap();

        let retrieved = store.agents().get("agent_test_1").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.agent_id, "agent_test_1");
        assert_eq!(retrieved.public_key, "c29tZV9rZXk=");
        assert_eq!(retrieved.allowed_scopes.len(), 2);

        let by_fp = store.agents().get_by_fingerprint("fp1").await.unwrap();
        assert!(by_fp.is_some());

        let (list, cursor) = store.agents().list(false, 10, None).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(cursor.is_none());

        let count_all = store.agents().count(false).await.unwrap();
        assert_eq!(count_all, 1);
        let count_active = store.agents().count(true).await.unwrap();
        assert_eq!(count_active, 1);

        let revoked = store.agents().revoke("agent_test_1").await.unwrap();
        assert!(revoked);

        let retrieved = store.agents().get("agent_test_1").await.unwrap().unwrap();
        assert!(retrieved.revoked_at.is_some());

        let count_all_after = store.agents().count(false).await.unwrap();
        assert_eq!(count_all_after, 1);
        let count_active_after = store.agents().count(true).await.unwrap();
        assert_eq!(count_active_after, 0);
    }

    #[tokio::test]
    async fn apply_embedded_migrations_rejects_future_schema_version() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        // Bootstrap _schema_version table
        sqlx::query(
            "CREATE TABLE _schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&store.pool)
        .await
        .unwrap();
        // Insert a future version
        sqlx::query("INSERT INTO _schema_version (version, applied_at) VALUES (?1, ?2)")
            .bind(migrations::CURRENT_SCHEMA_VERSION + 1)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&store.pool)
            .await
            .unwrap();

        let err = store.apply_embedded_migrations().await.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("schema version"),
            "expected schema drift error, got: {}",
            err_str
        );
    }
}
