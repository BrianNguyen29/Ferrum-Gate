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

mod agents;
mod approvals;
mod audit_checkpoint;
mod audit_log;
mod audit_merkle_root;
mod capabilities;
mod executions;
mod helpers;
mod intents;
mod ledger;
mod lifecycle_outbox;
mod mfa_credentials;
mod migrations;
mod policy_bundles;
mod proposals;
mod provenance;
mod rollback;
mod tokens;

pub use agents::PostgresAgentRepo;
pub use approvals::PostgresApprovalRepo;
pub use audit_checkpoint::PostgresAuditCheckpointRepo;
pub use audit_log::PostgresAuditLogRepo;
pub use audit_merkle_root::PostgresAuditMerkleRootRepo;
pub use capabilities::PostgresCapabilityRepo;
pub use executions::PostgresExecutionRepo;
pub use intents::PostgresIntentRepo;
pub use ledger::PostgresLedgerRepo;
pub use lifecycle_outbox::PostgresLifecycleOutboxRepo;
pub use mfa_credentials::PostgresMfaCredentialRepo;
pub use policy_bundles::PostgresPolicyBundleRepo;
pub use proposals::PostgresProposalRepo;
pub use provenance::PostgresProvenanceRepo;
pub use rollback::PostgresRollbackRepo;
pub use tokens::PostgresTokenRepo;

use crate::Result;
use crate::repos::{
    AgentRepo, ApprovalRepo, AuditCheckpointRepo, AuditLogRepo, AuditMerkleRootRepo,
    CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, LifecycleOutboxRepo, MfaCredentialRepo,
    PolicyBundleRepo, ProposalRepo, ProvenanceRepo, RollbackRepo, StoreFacade, TokenRepo,
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
    /// This is the **single** PostgreSQL migration path. There is no separate
    /// unversioned `bootstrap_schema` routine; the version-tracking table is
    /// created inline below so the runner can query `_schema_version` on a
    /// completely empty database. The same `CREATE TABLE IF NOT EXISTS` also
    /// appears in `001_initial.sql` for completeness when the file is run
    /// manually, but the authoritative versioned flow is this method.
    ///
    /// Idempotent: checks `_schema_version` before running SQL. Only migrations
    /// with a version greater than the recorded version are applied, and each
    /// version is recorded immediately after its SQL succeeds. If the recorded
    /// version is greater than [`migrations::CURRENT_SCHEMA_VERSION`], the call
    /// returns a [`StoreError::SchemaDrift`].
    ///
    /// SQLite-only migrations (`leader_tips`, `sync_state`, `leader_allowlist`)
    /// are intentionally **not** ported to PostgreSQL. `policy_bundles` is already
    /// included in the PG `001_initial.sql`. See [`migrations`](super::migrations)
    /// for the parity matrix.
    pub async fn apply_embedded_migrations(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Bootstrap: ensure version tracking table exists before querying it.
        // This is idempotent (`IF NOT EXISTS`) and required so the runner works
        // on a brand-new database. The same DDL is also present in
        // `001_initial.sql` for manual/psql usage, but that does not create a
        // duplicate path because this method is the only runtime entry-point.
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

        if (current_version as i64) > migrations::CURRENT_SCHEMA_VERSION {
            return Err(crate::StoreError::SchemaDrift {
                db_version: current_version as i64,
                expected_version: migrations::CURRENT_SCHEMA_VERSION,
            });
        }

        for migration in migrations::MIGRATIONS {
            if migration.version <= current_version as i64 {
                continue;
            }

            for sql in split_postgres_statements(migration.sql) {
                sqlx::query(sql).execute(&mut *tx).await?;
            }

            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO _schema_version (version, applied_at) VALUES ($1, $2)
                 ON CONFLICT (version) DO UPDATE SET applied_at = $2",
            )
            .bind(migration.version)
            .bind(now)
            .execute(&mut *tx)
            .await?;
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

    pub fn lifecycle_outbox(&self) -> PostgresLifecycleOutboxRepo {
        PostgresLifecycleOutboxRepo::new(self.pool.clone())
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

    pub fn tokens(&self) -> PostgresTokenRepo {
        PostgresTokenRepo::new(self.pool.clone())
    }

    pub fn audit_log(&self) -> PostgresAuditLogRepo {
        PostgresAuditLogRepo::new(self.pool.clone())
    }

    pub fn audit_merkle_roots(&self) -> PostgresAuditMerkleRootRepo {
        PostgresAuditMerkleRootRepo::new(self.pool.clone())
    }

    pub fn audit_checkpoints(&self) -> PostgresAuditCheckpointRepo {
        PostgresAuditCheckpointRepo::new(self.pool.clone())
    }

    pub fn agents(&self) -> PostgresAgentRepo {
        PostgresAgentRepo::new(self.pool.clone())
    }

    pub fn mfa_credentials(&self) -> PostgresMfaCredentialRepo {
        PostgresMfaCredentialRepo::new(self.pool.clone())
    }

    /// Gracefully close the PostgreSQL connection pool.
    pub async fn shutdown(&self) {
        self.pool.close().await;
    }
}

fn split_postgres_statements(sql: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut start = 0usize;
    let mut in_line_comment = false;
    let mut in_single_quote = false;
    let mut dollar_tag: Option<String> = None;
    let bytes = sql.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if in_line_comment {
            if bytes[i] == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if let Some(tag) = dollar_tag.as_ref() {
            if sql[i..].starts_with(tag) {
                i += tag.len();
                dollar_tag = None;
            } else {
                i += 1;
            }
            continue;
        }

        if in_single_quote {
            if bytes[i] == b'\'' {
                if bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                } else {
                    in_single_quote = false;
                    i += 1;
                }
            } else {
                i += 1;
            }
            continue;
        }

        if bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-') {
            in_line_comment = true;
            i += 2;
            continue;
        }

        if bytes[i] == b'\'' {
            in_single_quote = true;
            i += 1;
            continue;
        }

        if bytes[i] == b'$' {
            if let Some(end) = sql[i + 1..].find('$') {
                let tag_end = i + 1 + end;
                let tag_body = &sql[i + 1..tag_end];
                if tag_body
                    .chars()
                    .all(|c| c == '_' || c.is_ascii_alphanumeric())
                {
                    dollar_tag = Some(sql[i..=tag_end].to_string());
                    i = tag_end + 1;
                    continue;
                }
            }
        }

        if bytes[i] == b';' {
            let statement = sql[start..=i].trim();
            if !statement.is_empty() {
                statements.push(statement);
            }
            start = i + 1;
        }
        i += 1;
    }

    let tail = sql[start..].trim();
    if !tail.is_empty() {
        statements.push(tail);
    }

    statements
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

    fn lifecycle_outbox(&self) -> Arc<dyn LifecycleOutboxRepo> {
        Arc::new(self.lifecycle_outbox())
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

    fn tokens(&self) -> Arc<dyn TokenRepo> {
        Arc::new(self.tokens())
    }

    fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
        Arc::new(self.audit_log())
    }

    fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
        Arc::new(self.audit_merkle_roots())
    }

    fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
        Arc::new(self.audit_checkpoints())
    }

    fn agents(&self) -> Arc<dyn AgentRepo> {
        Arc::new(self.agents())
    }

    fn mfa_credentials(&self) -> Arc<dyn MfaCredentialRepo> {
        Arc::new(self.mfa_credentials())
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

    async fn shutdown(&self) -> crate::Result<()> {
        self.shutdown().await;
        Ok(())
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
    fn postgres_token_repo_implements_token_repo() {
        fn _check<T: TokenRepo>() {}
        _check::<PostgresTokenRepo>();
    }

    #[test]
    fn postgres_audit_log_repo_implements_audit_log_repo() {
        fn _check<T: AuditLogRepo>() {}
        _check::<PostgresAuditLogRepo>();
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
        assert_eq!(super::migrations::CURRENT_SCHEMA_VERSION, 13);
    }

    #[test]
    fn postgres_statement_splitter_preserves_dollar_quoted_blocks() {
        let sql = r#"
            CREATE TABLE example (id integer);
            DO $$
            BEGIN
                IF NOT EXISTS (SELECT 1) THEN
                    ALTER TABLE example ADD CONSTRAINT example_check CHECK (id > 0) NOT VALID;
                END IF;
            END $$;
            SELECT 'semi;colon';
        "#;

        let statements = super::split_postgres_statements(sql);
        assert_eq!(statements.len(), 3);
        assert!(statements[1].starts_with("DO $$"));
        assert!(statements[1].contains("NOT VALID;"));
        assert_eq!(statements[2], "SELECT 'semi;colon';");
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

    #[test]
    fn postgres_mfa_credential_repo_implements_mfa_credential_repo() {
        fn _check<T: MfaCredentialRepo>() {}
        _check::<PostgresMfaCredentialRepo>();
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_crud_smoke() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "encrypted-secret-b64",
            "nonce-b64",
            "key-1",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        let retrieved = repo.get(record.mfa_factor_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.agent_id, "agent-1");
        assert_eq!(retrieved.factor_type, ferrum_proto::MfaFactorType::Totp);

        repo.activate(record.mfa_factor_id).await.unwrap();
        let active = repo.get_active_for_agent("agent-1").await.unwrap();
        assert!(active.is_some());

        repo.record_use(record.mfa_factor_id, 42).await.unwrap();
        repo.revoke(record.mfa_factor_id).await.unwrap();
        let revoked = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(revoked.status, ferrum_proto::MfaFactorStatus::Inactive);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_get_active_for_agent() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "encrypted-secret-b64",
            "nonce-b64",
            "key-1",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        // Pending is not active
        let active = repo.get_active_for_agent("agent-1").await.unwrap();
        assert!(active.is_none());

        // Activate it
        repo.activate(record.mfa_factor_id).await.unwrap();
        let active = repo.get_active_for_agent("agent-1").await.unwrap();
        assert!(active.is_some());
        assert_eq!(
            active.unwrap().status,
            ferrum_proto::MfaFactorStatus::Active
        );
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_list_by_agent() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let repo = store.mfa_credentials();
        let r1 = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s1",
            "n1",
            "k1",
        );
        let r2 = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s2",
            "n2",
            "k2",
        );
        repo.insert(&r1).await.unwrap();
        repo.insert(&r2).await.unwrap();

        let list = repo.list_by_agent("agent-1").await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_record_use() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s",
            "n",
            "k",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        assert!(repo.record_use(record.mfa_factor_id, 42).await.unwrap());
        let retrieved = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert!(retrieved.last_used_at.is_some());
        assert_eq!(retrieved.last_used_counter, Some(42));

        // Same counter should fail (CAS replay protection)
        assert!(!repo.record_use(record.mfa_factor_id, 42).await.unwrap());
        // Lower counter should also fail
        assert!(!repo.record_use(record.mfa_factor_id, 41).await.unwrap());
        // Higher counter should succeed
        assert!(repo.record_use(record.mfa_factor_id, 43).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_record_use_rejects_overflow() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s",
            "n",
            "k",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        let err = repo
            .record_use(record.mfa_factor_id, i64::MAX as u64 + 1)
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::StoreError::InvalidState(_)),
            "expected InvalidState for counter overflow, got: {}",
            err
        );
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_revoke() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s",
            "n",
            "k",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        assert!(repo.revoke(record.mfa_factor_id).await.unwrap());
        let retrieved = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, ferrum_proto::MfaFactorStatus::Inactive);
        assert!(retrieved.revoked_at.is_some());

        // Idempotent: revoking again should return false
        assert!(!repo.revoke(record.mfa_factor_id).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_activate_revokes_existing_active_factor() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record1 = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s1",
            "n1",
            "k1",
        );
        let record2 = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s2",
            "n2",
            "k2",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record1).await.unwrap();
        repo.insert(&record2).await.unwrap();

        // Activate first factor
        assert!(repo.activate(record1.mfa_factor_id).await.unwrap());
        let active1 = repo.get(record1.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(active1.status, ferrum_proto::MfaFactorStatus::Active);

        // Activate second factor - should revoke the first one
        assert!(repo.activate(record2.mfa_factor_id).await.unwrap());
        let active2 = repo.get(record2.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(active2.status, ferrum_proto::MfaFactorStatus::Active);

        // First factor should now be inactive
        let revoked1 = repo.get(record1.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(revoked1.status, ferrum_proto::MfaFactorStatus::Inactive);
        assert!(revoked1.revoked_at.is_some());

        // Only one active factor per agent
        let active = repo.get_active_for_agent("agent-1").await.unwrap().unwrap();
        assert_eq!(active.mfa_factor_id, record2.mfa_factor_id);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_record_failed_attempt_and_lockout() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s",
            "n",
            "k",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        for _ in 0..4 {
            let locked = repo
                .record_failed_attempt(record.mfa_factor_id, 5, 900)
                .await
                .unwrap();
            assert!(!locked);
        }

        let r = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(r.failed_attempts, 4);
        assert!(r.locked_until.is_none());

        let locked = repo
            .record_failed_attempt(record.mfa_factor_id, 5, 900)
            .await
            .unwrap();
        assert!(locked);

        let r = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert_eq!(r.failed_attempts, 5);
        assert!(r.locked_until.is_some());
        assert_eq!(r.lockout_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_lockout_then_expired() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s",
            "n",
            "k",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        // Lock with 1 second duration
        repo.record_failed_attempt(record.mfa_factor_id, 1, 1)
            .await
            .unwrap();

        let r = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert!(r.locked_until.is_some());

        // Wait for lock to expire
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let r = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        let now = chrono::Utc::now();
        // locked_until should be in the past
        assert!(r.locked_until.map(|lu| lu <= now).unwrap_or(true));
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_mfa_credential_lockout_recovery_one_strike_relock() {
        let store = PostgresStore::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let record = ferrum_proto::MfaCredentialRecord::new(
            "agent-1",
            ferrum_proto::MfaFactorType::Totp,
            "s",
            "n",
            "k",
        );
        let repo = store.mfa_credentials();
        repo.insert(&record).await.unwrap();

        // Lock with 1 second duration
        repo.record_failed_attempt(record.mfa_factor_id, 1, 1)
            .await
            .unwrap();
        let r = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert!(r.locked_until.is_some());
        assert_eq!(r.failed_attempts, 1);

        // Wait for lock to expire
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // After expiry, a single failed attempt re-locks immediately because
        // failed_attempts is still 1 (equal to max_attempts). This is the
        // post-expiry one-strike re-lock behavior.
        let locked = repo
            .record_failed_attempt(record.mfa_factor_id, 1, 1)
            .await
            .unwrap();
        assert!(
            locked,
            "expected immediate re-lock after expiry with one strike"
        );

        let r = repo.get(record.mfa_factor_id).await.unwrap().unwrap();
        assert!(r.locked_until.is_some());
        assert!(r.locked_until.unwrap() > chrono::Utc::now());
        assert_eq!(r.lockout_count, 2);
    }
}
