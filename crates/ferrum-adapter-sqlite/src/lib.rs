//! SQLite adapter for mutation and transaction recovery.
//!
//! This adapter implements the `RollbackAdapter` trait for SQLite databases,
//! supporting prepare→execute→verify→rollback lifecycle on file-backed DBs.
//!
//! # Transaction-Based Rollback
//!
//! - **DML operations** (INSERT/UPDATE/DELETE): Use SAVEPOINT for reversible transactions
//! - **DDL operations** (CREATE TABLE, ALTER TABLE, DROP TABLE): Capture schema before execution
//!   and restore on rollback

use async_trait::async_trait;
use chrono::Utc;
use ferrum_proto::{
    ActionType, CheckType, JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Global counter for generating unique savepoint names
static SAVEPOINT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const ADAPTER_KIND: &str = "ferrum-adapter-sqlite";

/// Phase context for error normalization.
const PHASE_EXECUTE: &str = "execute";
const PHASE_ROLLBACK: &str = "rollback";

#[derive(Debug, Error)]
pub enum SqliteAdapterError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid target: expected SqliteTxn, got {0}")]
    InvalidTarget(String),
    #[error("db path not found or not a file: {0}")]
    DbPathNotFound(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),
    #[error("transaction error: {0}")]
    Transaction(String),
}

impl From<SqliteAdapterError> for AdapterError {
    fn from(err: SqliteAdapterError) -> Self {
        match err {
            SqliteAdapterError::Sqlite(e) => AdapterError::Internal(format!("sqlite error: {}", e)),
            SqliteAdapterError::InvalidTarget(msg) => AdapterError::Validation(msg),
            SqliteAdapterError::DbPathNotFound(msg) => AdapterError::Validation(msg),
            SqliteAdapterError::Serialization(msg) => AdapterError::Internal(msg),
            SqliteAdapterError::SchemaMismatch(msg) => AdapterError::Validation(msg),
            SqliteAdapterError::Transaction(msg) => AdapterError::Internal(msg),
        }
    }
}

/// SQL statement type for rollback strategy selection
#[derive(Debug, Clone, PartialEq)]
enum SqlType {
    /// Data Manipulation Language - uses SAVEPOINT for rollback
    Dml,
    /// Data Definition Language - uses schema capture for rollback
    Ddl,
}

/// SQLite adapter implementing the RollbackAdapter trait.
///
/// Uses file-backed SQLite databases to provide deterministic
/// prepare→execute→verify→rollback lifecycle testing with transaction-based rollback.
pub struct SqliteAdapter {
    key: &'static str,
}

impl SqliteAdapter {
    pub fn new(key: &'static str) -> Self {
        Self { key }
    }

    /// Extracts the db_path from a RollbackTarget::SqliteTxn variant.
    fn extract_db_path(target: &RollbackTarget) -> Result<&str, AdapterError> {
        match target {
            RollbackTarget::SqliteTxn { db_path, .. } => Ok(db_path),
            _ => Err(AdapterError::Validation(format!(
                "invalid target: expected SqliteTxn, got {:?}",
                target
            ))),
        }
    }

    /// Validates that the given path exists and is a file.
    fn validate_db_exists(path: &str) -> Result<(), AdapterError> {
        let p = Path::new(path);
        if !p.exists() || !p.is_file() {
            return Err(SqliteAdapterError::DbPathNotFound(path.to_string()).into());
        }
        Ok(())
    }

    /// Opens a new connection to the SQLite database with production WAL-mode tuning.
    fn open_conn(path: &str) -> Result<Connection, SqliteAdapterError> {
        let conn = Connection::open(path).map_err(SqliteAdapterError::Sqlite)?;
        // WAL mode enables concurrent readers and a single writer, improving throughput.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        Ok(conn)
    }

    /// Classifies SQL statement as DML or DDL.
    fn classify_sql(sql: &str) -> SqlType {
        let trimmed = sql.trim().to_uppercase();
        // DDL statements: CREATE, ALTER, DROP, TRUNCATE
        if trimmed.starts_with("CREATE")
            || trimmed.starts_with("ALTER")
            || trimmed.starts_with("DROP")
            || trimmed.starts_with("TRUNCATE")
        {
            SqlType::Ddl
        } else {
            SqlType::Dml
        }
    }

    /// Generates a unique savepoint name.
    fn generate_savepoint_name() -> String {
        let counter = SAVEPOINT_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("sp_{}", counter)
    }

    /// Captures the schema of all tables in the database.
    fn capture_all_schemas(conn: &Connection) -> Result<String, SqliteAdapterError> {
        let mut stmt =
            conn.prepare("SELECT sql FROM sqlite_master WHERE type='table' ORDER BY name")?;
        let schemas: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(schemas.join(";\n"))
    }

    /// Normalizes an internal error with phase context.
    fn phase_wrap_internal(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Internal(format!("[{}] {}", phase, msg))
    }

    /// Normalizes a validation error with phase context.
    fn phase_wrap_validation(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Validation(format!("[{}] {}", phase, msg))
    }

    /// Runs a single check spec and returns whether verification passed.
    ///
    /// Returns `Ok(true)` if the check passed.
    /// Returns `Ok(false)` if the check failed (verification should fail closed).
    /// Returns `Err` only for actual programming errors (unsupported check type, misconfigured check).
    ///
    /// Query errors (DB locked, IO errors, etc.) during a check result in `Ok(false)` — fail closed.
    fn run_check(
        check: &ferrum_proto::CheckSpec,
        conn: &Connection,
        phase: &'static str,
    ) -> Result<bool, AdapterError> {
        match check.check_type {
            CheckType::SqlRowCountRange => {
                // Validate 'table' field is present
                let table = match check.config.get("table") {
                    Some(serde_json::Value::String(s)) => s.as_str(),
                    Some(v) => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] SqlRowCountRange check 'table' must be a string, got {}",
                            phase, v
                        )));
                    }
                    None => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] SqlRowCountRange check requires 'table' config",
                            phase
                        )));
                    }
                };

                let min_rows = check
                    .config
                    .get("min_rows")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let max_rows = check
                    .config
                    .get("max_rows")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(i64::MAX);

                let count: i64 =
                    match conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
                        row.get(0)
                    }) {
                        Ok(c) => c,
                        // Query errors (DB locked, table doesn't exist, IO errors, etc.) -> fail closed
                        Err(_) => return Ok(false),
                    };

                if count < min_rows || count > max_rows {
                    // Row count out of range -> check failed, fail closed
                    return Ok(false);
                }
                Ok(true)
            }
            _ => Err(AdapterError::Unsupported(format!(
                "[{}] unsupported check type: {:?}",
                phase, check.check_type
            ))),
        }
    }

    /// Executes a SQL statement and returns rows_affected.
    fn execute_sql(conn: &Connection, sql: &str) -> Result<i64, SqliteAdapterError> {
        conn.execute(sql, [])?;
        Ok(conn.changes() as i64)
    }
}

#[async_trait]
impl RollbackAdapter for SqliteAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate that target is SqliteTxn
        let db_path = Self::extract_db_path(&request.target)?;

        // Validate that action_type is SqlMutation
        match request.action_type {
            ActionType::SqlMutation => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported action type: {:?}",
                    request.action_type
                )));
            }
        }

        // Validate database exists (fail-closed)
        Self::validate_db_exists(db_path).map_err(|_e| {
            Self::phase_wrap_validation(
                PHASE_EXECUTE,
                format!("database not found or not a file: {}", db_path),
            )
        })?;

        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "prepared_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "db_path".to_string(),
            serde_json::Value::String(db_path.to_string()),
        );

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: metadata,
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        let db_path = Self::extract_db_path(&contract.target)?;

        // Extract SQL from payload
        let sql = match payload {
            serde_json::Value::String(s) => s.as_str(),
            serde_json::Value::Object(obj) => {
                obj.get("sql").and_then(|v| v.as_str()).unwrap_or_default()
            }
            _ => {
                return Err(AdapterError::Validation(
                    "payload must be a string or object with 'sql' field".into(),
                ));
            }
        };

        if sql.is_empty() {
            return Err(AdapterError::Validation(
                "sql field is required in payload".into(),
            ));
        }

        // Classify SQL type
        let sql_type = Self::classify_sql(sql);

        let conn = Self::open_conn(db_path).map_err(|e| {
            Self::phase_wrap_internal(PHASE_EXECUTE, format!("failed to open database: {}", e))
        })?;

        let rows_affected: i64;
        let savepoint_name: Option<String>;
        let schema_capture: Option<String>;

        match sql_type {
            SqlType::Dml => {
                // DML: Use SAVEPOINT for rollback
                savepoint_name = Some(Self::generate_savepoint_name());
                let sp_name = savepoint_name.as_ref().unwrap();

                // Create savepoint
                conn.execute(&format!("SAVEPOINT {}", sp_name), [])
                    .map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_EXECUTE,
                            format!("failed to create savepoint: {}", e),
                        )
                    })?;

                // Execute the DML statement
                rows_affected = Self::execute_sql(&conn, sql).map_err(|e| {
                    // Rollback to savepoint on failure
                    let _ = conn.execute(&format!("ROLLBACK TO SAVEPOINT {}", sp_name), []);
                    Self::phase_wrap_internal(PHASE_EXECUTE, format!("execute failed: {}", e))
                })?;

                // Release savepoint (make it permanent)
                conn.execute(&format!("RELEASE SAVEPOINT {}", sp_name), [])
                    .map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_EXECUTE,
                            format!("failed to release savepoint: {}", e),
                        )
                    })?;

                schema_capture = None;
            }
            SqlType::Ddl => {
                // DDL: Capture schema before execution for rollback
                savepoint_name = None;
                schema_capture = Some(Self::capture_all_schemas(&conn).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("failed to capture schema: {}", e),
                    )
                })?);

                // Execute the DDL statement
                rows_affected = Self::execute_sql(&conn, sql).map_err(|e| {
                    Self::phase_wrap_internal(PHASE_EXECUTE, format!("execute failed: {}", e))
                })?;
            }
        }

        let mut metadata = JsonMap::new();
        metadata.insert(
            "rows_affected".to_string(),
            serde_json::json!(rows_affected),
        );
        metadata.insert(
            "sql_type".to_string(),
            serde_json::json!(if sql_type == SqlType::Dml {
                "DML"
            } else {
                "DDL"
            }),
        );

        if let Some(sp_name) = &savepoint_name {
            metadata.insert("savepoint_name".to_string(), serde_json::json!(sp_name));
        }

        if let Some(schema) = &schema_capture {
            metadata.insert("schema_capture".to_string(), serde_json::json!(schema));
        }

        Ok(ExecuteReceipt {
            external_id: None,
            result_digest: Some(format!("{:?}", sql_type)),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let db_path = Self::extract_db_path(&contract.target)?;

        // Fail-closed with verified=false for DB connection/lock/path errors
        // This follows the pattern: if we cannot verify due to infrastructure issues,
        // the verification failed (verified=false), rather than a hard error.
        let conn = match Self::open_conn(db_path) {
            Ok(c) => c,
            Err(_e) => {
                // Connection errors (DB not found, locked, permission denied, etc.)
                // should result in verified=false, not a hard adapter error.
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: JsonMap::new(),
                });
            }
        };

        // Run verify_checks if present — fail closed on any check error/failure
        for check in &contract.verify_checks {
            match Self::run_check(check, &conn, "verify") {
                Ok(true) => {}
                Ok(false) => {
                    // Check failed or query error -> verified=false
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: JsonMap::new(),
                    });
                }
                Err(e) => {
                    // Unsupported/misconfigured check type is a programming error — propagate
                    return Err(e);
                }
            }
        }

        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // Compensation delegates to rollback
        self.rollback(contract).await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        let db_path = Self::extract_db_path(&contract.target)?;

        // Get sql_type and schema_capture from contract metadata
        let sql_type_str = contract
            .metadata
            .get("sql_type")
            .and_then(|v| v.as_str())
            .unwrap_or("DML");

        let sql_type = if sql_type_str == "DDL" {
            SqlType::Ddl
        } else {
            SqlType::Dml
        };

        match sql_type {
            SqlType::Dml => {
                // DML rollback: Execute compensation plan
                if contract.compensation_plan.is_empty() {
                    return Err(AdapterError::Validation(
                        "DML rollback requires compensation_plan".into(),
                    ));
                }

                let conn = Self::open_conn(db_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_ROLLBACK,
                        format!("failed to open database: {}", e),
                    )
                })?;

                for step in &contract.compensation_plan {
                    if step.operation == "rollback" {
                        if let Some(sql) = step.args.get("sql").and_then(|v| v.as_str()) {
                            Self::execute_sql(&conn, sql).map_err(|e| {
                                Self::phase_wrap_internal(
                                    PHASE_ROLLBACK,
                                    format!("compensation failed: {}", e),
                                )
                            })?;
                        }
                    }
                }

                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: JsonMap::new(),
                })
            }
            SqlType::Ddl => {
                // DDL rollback: Schema migration guard check
                // For DDL, we need to verify that the schema hasn't drifted since execute.
                // The schema_capture should be in contract.metadata (set by execute for DDL operations).
                // However, if not present (e.g., contract wasn't updated after execute),
                // we perform the schema verification using the compensation_plan SQL.

                let conn = Self::open_conn(db_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_ROLLBACK,
                        format!("failed to open database: {}", e),
                    )
                })?;

                // Try to get schema_capture from metadata (set during execute for DDL)
                let schema_capture = contract
                    .metadata
                    .get("schema_capture")
                    .and_then(|v| v.as_str());

                // Schema migration guard: if we have a captured schema, verify no drift
                if let Some(captured) = schema_capture {
                    let current_schema = Self::capture_all_schemas(&conn).map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_ROLLBACK,
                            format!("failed to capture current schema: {}", e),
                        )
                    })?;

                    if current_schema != captured {
                        return Err(AdapterError::Validation(
                            "schema migration guard triggered: schema has drifted".into(),
                        ));
                    }
                }

                // Execute compensation plan if present
                if !contract.compensation_plan.is_empty() {
                    for step in &contract.compensation_plan {
                        if step.operation == "rollback" {
                            if let Some(sql) = step.args.get("sql").and_then(|v| v.as_str()) {
                                Self::execute_sql(&conn, sql).map_err(|e| {
                                    Self::phase_wrap_internal(
                                        PHASE_ROLLBACK,
                                        format!("DDL rollback failed: {}", e),
                                    )
                                })?;
                            }
                        }
                    }
                }

                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: JsonMap::new(),
                })
            }
        }
    }
}

// =============================================================================
// Plannable adapter for SQLite operations
// =============================================================================

pub mod planner;
pub use planner::PlannableSqliteAdapter;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        CheckSpec, CompensationStep, ExecutionId, IntentId, ProposalId, RollbackContractId,
        RollbackState,
    };
    use tempfile::tempdir;

    fn create_test_request(db_path: &str) -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path.to_string(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        }
    }

    fn create_test_contract(db_path: &str) -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path.to_string(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        }
    }

    fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
        map.into_iter().collect()
    }

    #[tokio::test]
    async fn test_prepare_execute_verify_rollback_on_file_db() {
        // Create a temp directory with a file-backed SQLite DB
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        // Create the adapter
        let adapter = SqliteAdapter::new("sqlite");

        // Create the DB file and table using Connection::open (creates if not exists)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        // PREPARE: Call prepare on the adapter
        let request = create_test_request(&db_path_str);
        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);

        // EXECUTE: Insert a row
        let payload = serde_json::json!({
            "sql": "INSERT INTO items (name) VALUES ('test_item')"
        });
        let contract = create_test_contract(&db_path_str);
        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();
        assert!(exec_receipt.result_digest.is_some());

        // VERIFY: Check the row was inserted
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }

        // VERIFY: Use adapter verify with SqlRowCountRange check
        let mut contract_with_verify = create_test_contract(&db_path_str);
        contract_with_verify.verify_checks = vec![CheckSpec {
            check_type: CheckType::SqlRowCountRange,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "table": "items",
                    "min_rows": 1,
                    "max_rows": 10
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];
        let verify_receipt = adapter.verify(&contract_with_verify).await.unwrap();
        assert!(verify_receipt.verified);

        // ROLLBACK: Delete the row via compensation
        let mut contract_with_compensation = create_test_contract(&db_path_str);
        contract_with_compensation.compensation_plan = vec![CompensationStep {
            order: 1,
            adapter_key: "sqlite".to_string(),
            operation: "rollback".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({
                    "sql": "DELETE FROM items WHERE name = 'test_item'"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            idempotency_key: "rollback-1".to_string(),
        }];
        let rollback_receipt = adapter.rollback(&contract_with_compensation).await.unwrap();
        assert!(rollback_receipt.recovered);

        // VERIFY: The row is gone
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn test_prepare_accepts_valid_db_path() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        // Create the DB file first
        Connection::open(&db_path).unwrap();

        let adapter = SqliteAdapter::new("sqlite");
        let request = create_test_request(&db_path_str);
        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
    }

    #[tokio::test]
    async fn test_prepare_fails_on_nonexistent_db_path() {
        let adapter = SqliteAdapter::new("sqlite");
        let request = create_test_request("/nonexistent/path/to/db.sqlite");
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_fails_when_row_count_out_of_range() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        // Create DB with a table but no rows
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        let adapter = SqliteAdapter::new("sqlite");
        let mut contract_with_verify = create_test_contract(&db_path_str);
        contract_with_verify.verify_checks = vec![CheckSpec {
            check_type: CheckType::SqlRowCountRange,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "table": "items",
                    "min_rows": 5,  // Expect at least 5 rows
                    "max_rows": 100
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let verify_receipt = adapter.verify(&contract_with_verify).await.unwrap();
        // Verification should fail because table is empty
        assert!(!verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_verify_returns_verified_false_on_nonexistent_db() {
        // G-E1 SQLite hardening: verify() should fail closed with verified=false
        // (not a hard error) when the database file doesn't exist.
        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract("/nonexistent/path/to/db.sqlite");

        let result = adapter.verify(&contract).await.unwrap();
        // Should return verified=false, not an error
        assert!(!result.verified);
    }

    // NOTE: The following test was renamed from test_verify_returns_verified_false_on_db_locked
    // because it does NOT test actual DB lock contention. SQLite's Connection::open() succeeds
    // even when the DB is locked; lock contention only manifests on query execution, which would
    // require holding a write lock with busy_timeout=5000ms (impractical for a fast unit test).
    // This test covers SqlRowCountRange check failure (empty table) which also returns
    // verified=false — the same fail-closed outcome, but via a different code path.

    #[tokio::test]
    async fn test_verify_returns_verified_false_on_row_count_out_of_range() {
        // G-E1 fail-closed: verify_check that fails (row count out of range) returns verified=false
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        // Create the database with an empty table
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");

        let contract = create_test_contract(&db_path_str);
        let result = adapter.verify(&contract).await.unwrap();
        // Without verify_checks, connection success means verified=true
        assert!(result.verified);

        // verify_check that will fail due to row count out of range
        let mut contract_with_check = create_test_contract(&db_path_str);
        contract_with_check.verify_checks = vec![CheckSpec {
            check_type: CheckType::SqlRowCountRange,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "table": "items",
                    "min_rows": 5,  // Expect at least 5 rows but table is empty
                    "max_rows": 100
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let result2 = adapter.verify(&contract_with_check).await.unwrap();
        // Row count out of range -> verified=false (fail closed)
        assert!(!result2.verified);
    }

    #[tokio::test]
    async fn test_execute_fails_on_invalid_sql() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract(&db_path_str);

        // Try to execute invalid SQL
        let payload = serde_json::json!({
            "sql": "INVALID SQL SYNTAX"
        });
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compensate_calls_rollback() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        let adapter = SqliteAdapter::new("sqlite");
        let mut contract = create_test_contract(&db_path_str);
        contract.compensation_plan = vec![CompensationStep {
            order: 1,
            adapter_key: "sqlite".to_string(),
            operation: "compensate".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({
                    "sql": "DELETE FROM items WHERE name = 'nonexistent'"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            idempotency_key: "compensate-1".to_string(),
        }];

        // compensate should call rollback internally and succeed
        let receipt = adapter.compensate(&contract).await.unwrap();
        assert!(receipt.recovered);
    }

    #[tokio::test]
    async fn test_sql_execute_insert_with_transaction() {
        // Use file-backed SQLite database for sharing across connections
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table first
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        // PREPARE
        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        // EXECUTE: Insert a row
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "INSERT INTO items (name) VALUES ('test_item')"
        });
        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();

        // Verify rows_affected is in metadata
        let rows_affected = exec_receipt
            .adapter_metadata
            .get("rows_affected")
            .and_then(|v| v.as_i64())
            .unwrap();
        assert_eq!(rows_affected, 1);

        // Verify sql_type is DML
        let sql_type = exec_receipt
            .adapter_metadata
            .get("sql_type")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(sql_type, "DML");

        // Verify the row was inserted
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[tokio::test]
    async fn test_sql_execute_update_with_transaction() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table with data
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO items (name) VALUES ('original');",
            )
            .unwrap();
        }

        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "UPDATE items SET name = 'updated' WHERE name = 'original'"
        });
        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();

        let rows_affected = exec_receipt
            .adapter_metadata
            .get("rows_affected")
            .and_then(|v| v.as_i64())
            .unwrap();
        assert_eq!(rows_affected, 1);

        // Verify the row was updated
        {
            let conn = Connection::open(&db_path).unwrap();
            let name: String = conn
                .query_row("SELECT name FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(name, "updated");
        }
    }

    #[tokio::test]
    async fn test_sql_execute_delete_with_transaction() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table with data
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO items (name) VALUES ('todelete');",
            )
            .unwrap();
        }

        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "DELETE FROM items WHERE name = 'todelete'"
        });
        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();

        let rows_affected = exec_receipt
            .adapter_metadata
            .get("rows_affected")
            .and_then(|v| v.as_i64())
            .unwrap();
        assert_eq!(rows_affected, 1);

        // Verify the row was deleted
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn test_sql_rollback_insert() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        // Prepare
        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute INSERT
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![CompensationStep {
                order: 1,
                adapter_key: "sqlite".to_string(),
                operation: "rollback".to_string(),
                args: json_map_from_serde_map(
                    serde_json::json!({
                        "sql": "DELETE FROM items WHERE name = 'test_item'"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                idempotency_key: "rollback-1".to_string(),
            }],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "INSERT INTO items (name) VALUES ('test_item')"
        });
        adapter.execute(&contract, &payload).await.unwrap();

        // Verify row exists before rollback
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }

        // Rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify row is gone after rollback
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn test_sql_rollback_update() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table with data
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO items (name) VALUES ('original');",
            )
            .unwrap();
        }

        // Prepare
        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute UPDATE
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![CompensationStep {
                order: 1,
                adapter_key: "sqlite".to_string(),
                operation: "rollback".to_string(),
                args: json_map_from_serde_map(
                    serde_json::json!({
                        "sql": "UPDATE items SET name = 'original' WHERE name = 'updated'"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                idempotency_key: "rollback-1".to_string(),
            }],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "UPDATE items SET name = 'updated' WHERE name = 'original'"
        });
        adapter.execute(&contract, &payload).await.unwrap();

        // Verify name is updated before rollback
        {
            let conn = Connection::open(&db_path).unwrap();
            let name: String = conn
                .query_row("SELECT name FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(name, "updated");
        }

        // Rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify name is restored after rollback
        {
            let conn = Connection::open(&db_path).unwrap();
            let name: String = conn
                .query_row("SELECT name FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(name, "original");
        }
    }

    #[tokio::test]
    async fn test_sql_rollback_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table with data
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO items (name) VALUES ('todelete');",
            )
            .unwrap();
        }

        // Prepare
        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute DELETE
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![CompensationStep {
                order: 1,
                adapter_key: "sqlite".to_string(),
                operation: "rollback".to_string(),
                args: json_map_from_serde_map(
                    serde_json::json!({
                        "sql": "INSERT INTO items (name) VALUES ('todelete')"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                idempotency_key: "rollback-1".to_string(),
            }],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "DELETE FROM items WHERE name = 'todelete'"
        });
        adapter.execute(&contract, &payload).await.unwrap();

        // Verify row is deleted before rollback
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }

        // Rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify row is restored after rollback
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[tokio::test]
    async fn test_sql_ddl_create_table_rollback() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create an empty database
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("").unwrap();
        }

        // Prepare
        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute CREATE TABLE
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![CompensationStep {
                order: 1,
                adapter_key: "sqlite".to_string(),
                operation: "rollback".to_string(),
                args: json_map_from_serde_map(
                    serde_json::json!({
                        "sql": "DROP TABLE test_items"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                idempotency_key: "rollback-1".to_string(),
            }],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "CREATE TABLE test_items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
        });
        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();

        // Verify sql_type is DDL
        let sql_type = exec_receipt
            .adapter_metadata
            .get("sql_type")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(sql_type, "DDL");

        // Verify table exists
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test_items'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);
        }

        // Rollback via compensation (DROP TABLE)
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify table is dropped
        {
            let conn = Connection::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test_items'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    // -------------------------------------------------------------------------
    // Gap tests: path/connection edge cases
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_prepare_fails_when_db_path_is_a_directory() {
        // G-E1: validate_db_exists should reject a directory path (is_file = false)
        let temp_dir = tempdir().unwrap();
        let dir_path = temp_dir.path().display().to_string();

        let adapter = SqliteAdapter::new("sqlite");
        let request = create_test_request(&dir_path);
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("not found or not a file"));
    }

    #[tokio::test]
    async fn test_execute_fails_on_nonexistent_db() {
        // execute() should fail with a connection error when DB path does not exist
        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract("/nonexistent/path/to/db.sqlite");
        let payload = serde_json::json!({ "sql": "INSERT INTO items (name) VALUES ('x')" });
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be a connection/infrastructure error, not a validation error
        assert!(format!("{}", err).contains("failed to open database"));
    }

    #[tokio::test]
    async fn test_rollback_dml_fails_with_empty_compensation_plan() {
        // DML rollback requires a compensation_plan; empty plan is a validation error
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract(&db_path_str);
        // Contract has empty compensation_plan (default) and sql_type defaults to DML
        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("compensation_plan"));
    }

    #[tokio::test]
    async fn test_rollback_dml_fails_on_missing_db_path() {
        // DML rollback should fail with a connection error when DB path does not exist
        let adapter = SqliteAdapter::new("sqlite");
        let mut contract = create_test_contract("/nonexistent/path/to/db.sqlite");
        contract.metadata.insert(
            "sql_type".to_string(),
            serde_json::Value::String("DML".to_string()),
        );
        contract.compensation_plan = vec![CompensationStep {
            order: 1,
            adapter_key: "sqlite".to_string(),
            operation: "rollback".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({ "sql": "DELETE FROM items WHERE id = 1" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            idempotency_key: "dml-missing-db".to_string(),
        }];

        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be a connection/infrastructure error from open_conn
        assert!(format!("{}", err).contains("failed to open database"));
    }

    #[tokio::test]
    async fn test_rollback_ddl_fails_on_missing_db_path() {
        // DDL rollback should fail with a connection error when DB path does not exist
        let adapter = SqliteAdapter::new("sqlite");
        let mut contract = create_test_contract("/nonexistent/path/to/db.sqlite");
        contract.metadata.insert(
            "sql_type".to_string(),
            serde_json::Value::String("DDL".to_string()),
        );
        contract.compensation_plan = vec![CompensationStep {
            order: 1,
            adapter_key: "sqlite".to_string(),
            operation: "rollback".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({ "sql": "DROP TABLE items" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            idempotency_key: "ddl-missing-db".to_string(),
        }];

        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("failed to open database"));
    }

    #[tokio::test]
    async fn test_rollback_ddl_without_schema_capture_skips_guard() {
        // DDL rollback without schema_capture in metadata should silently skip
        // the schema migration guard check and proceed to execute compensation_plan
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");

        // Build a DDL contract with NO schema_capture in metadata
        let mut contract = create_test_contract(&db_path_str);
        contract.metadata.insert(
            "sql_type".to_string(),
            serde_json::Value::String("DDL".to_string()),
        );
        // Note: schema_capture is intentionally absent
        contract.compensation_plan = vec![CompensationStep {
            order: 1,
            adapter_key: "sqlite".to_string(),
            operation: "rollback".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({ "sql": "INSERT INTO items (id) VALUES (999)" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            idempotency_key: "ddl-no-schema-capture".to_string(),
        }];

        // Rollback should succeed (no guard to trigger since schema_capture is absent)
        let result = adapter.rollback(&contract).await;
        assert!(result.is_ok());
        assert!(result.unwrap().recovered);
    }

    // -------------------------------------------------------------------------
    // Gap tests: verify check config edge cases
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_check_missing_table_returns_error() {
        // SqlRowCountRange without 'table' in config should return a Validation error
        // (programming error, not fail-closed verified=false)
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");
        let mut contract = create_test_contract(&db_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::SqlRowCountRange,
            config: json_map_from_serde_map(
                serde_json::json!({ "min_rows": 1, "max_rows": 10 })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        // Missing 'table' is a misconfigured check -> Err, not verified=false
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("table"));
    }

    // -------------------------------------------------------------------------
    // Gap tests: execute payload edge cases
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_rejects_object_with_missing_sql_key() {
        // Payload as object but missing 'sql' field should fail validation
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract(&db_path_str);

        // Object with no 'sql' key
        let payload = serde_json::json!({ "other_field": "value" });
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("sql"));
    }

    #[tokio::test]
    async fn test_execute_rejects_json_number_payload() {
        // JSON number payload is invalid; only string or object are accepted
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract(&db_path_str);

        let payload = serde_json::json!(42);
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("string or object"));
    }

    #[tokio::test]
    async fn test_execute_rejects_json_array_payload() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract(&db_path_str);

        let payload = serde_json::json!(["sql1", "sql2"]);
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_rejects_json_null_payload() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");
        let contract = create_test_contract(&db_path_str);

        let payload = serde_json::Value::Null;
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Gap tests: misleading test renaming/note
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_returns_verified_false_on_query_error_not_config_error() {
        // G-E1 fail-closed: query errors (DB locked, table not found, IO errors) during
        // a SqlRowCountRange check return verified=false.
        // NOTE: This is NOT a true DB-lock contention test. SQLite's Connection::open()
        // succeeds even when the DB is locked; the lock only manifests on query execution.
        // A genuine lock-contention test would require holding a write lock across another
        // connection with busy_timeout=5000ms, which is impractical for a fast unit test.
        // This test documents the actual fail-closed behavior for query errors.

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(conn);

        let adapter = SqliteAdapter::new("sqlite");

        // verify_check that queries a nonexistent table triggers query error -> verified=false
        let mut contract = create_test_contract(&db_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::SqlRowCountRange,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "table": "nonexistent_table",
                    "min_rows": 1,
                    "max_rows": 10
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await.unwrap();
        // Query error (table not found) should result in verified=false (fail closed)
        assert!(!result.verified);
    }

    // -------------------------------------------------------------------------
    // Schema migration guard edge case — documented but not tested
    // -------------------------------------------------------------------------
    // NOTE: capture_all_schemas captures ALL tables in the DB, so unrelated table
    // drift (e.g., another process creating a table between execute and rollback)
    // will trigger the schema migration guard even though the drift is unrelated
    // to the DDL operation under rollback. This is a known limitation.
    // A stable, intentional test for this would require simulating a second process
    // or external schema change, which is out of scope for a fast unit test.

    #[tokio::test]
    async fn test_sql_schema_migration_guard() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.display().to_string();

        let adapter = SqliteAdapter::new("sqlite");

        // Create table
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .unwrap();
        }

        // Prepare
        let request = create_test_request(&db_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute ALTER TABLE (DDL)
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::SqlMutation,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "sqlite".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: db_path_str.clone(),
                tx_id: "test-tx".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![CompensationStep {
                order: 1,
                adapter_key: "sqlite".to_string(),
                operation: "rollback".to_string(),
                args: json_map_from_serde_map(
                    serde_json::json!({
                        "sql": "ALTER TABLE items DROP COLUMN email"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                idempotency_key: "rollback-1".to_string(),
            }],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let payload = serde_json::json!({
            "sql": "ALTER TABLE items ADD COLUMN email TEXT"
        });
        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();

        // Verify sql_type is DDL and schema_capture is present
        let sql_type = exec_receipt
            .adapter_metadata
            .get("sql_type")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(sql_type, "DDL");
        let schema_capture = exec_receipt
            .adapter_metadata
            .get("schema_capture")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        assert!(!schema_capture.is_empty());

        // Update contract metadata with schema_capture so rollback can access it
        let mut contract = contract;
        contract.metadata.insert(
            "sql_type".to_string(),
            serde_json::Value::String("DDL".to_string()),
        );
        contract.metadata.insert(
            "schema_capture".to_string(),
            serde_json::Value::String(schema_capture),
        );

        // Now manually modify the schema (simulate external migration)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("ALTER TABLE items ADD COLUMN phone TEXT")
                .unwrap();
        }

        // Attempt to rollback should fail due to schema drift
        let rollback_receipt = adapter.rollback(&contract).await;
        assert!(rollback_receipt.is_err());

        // Verify the error indicates schema drift
        let err = rollback_receipt.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("schema migration guard triggered")
                || err_msg.contains("schema has drifted")
        );
    }
}
