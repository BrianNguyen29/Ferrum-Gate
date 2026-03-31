// Test-focused SQLite adapter for rollback/compensate evidence.
// This adapter performs real SQLite row mutations against a file-backed DB.
//
// Multi-row support: payloads can use either:
//   - Legacy single-row: {table, row_id, content}
//   - Multi-row transaction: {rows: [{table, row_id, content}, ...]}
//
// All multi-row operations execute atomically within a single SQLite transaction.

use async_trait::async_trait;
use ferrum_proto::{JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use sqlx::{Connection, Row, Sqlite, SqliteConnection, Transaction};
use std::collections::HashMap;
use std::sync::Mutex;

pub const ADAPTER_KIND: &str = "ferrum-adapter-sqlite";
pub const ADAPTER_KEY: &str = "sqlite";

#[derive(Clone, Debug)]
struct RowSnapshot {
    table: String,
    row_id: String,
    original_content: Option<String>,
    current_content: String,
}

/// A single row operation within a multi-row transaction payload.
#[derive(Clone, Debug, serde::Deserialize)]
struct RowOp {
    table: String,
    row_id: String,
    content: String,
}

#[derive(Default)]
pub struct SqliteSnapshotStore {
    snapshots: Mutex<HashMap<String, Vec<RowSnapshot>>>,
}

impl SqliteSnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    fn save_snapshot(&self, execution_id: &str, snapshot: RowSnapshot) {
        let mut snapshots = self.snapshots.lock().unwrap();
        snapshots
            .entry(execution_id.to_string())
            .or_default()
            .push(snapshot);
    }

    fn snapshots_for_execution(&self, execution_id: &str) -> Vec<RowSnapshot> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots.get(execution_id).cloned().unwrap_or_default()
    }

    fn clear_snapshots(&self, execution_id: &str) {
        let mut snapshots = self.snapshots.lock().unwrap();
        snapshots.remove(execution_id);
    }
}

pub struct SqliteRollbackAdapter {
    key: &'static str,
    snapshot_store: SqliteSnapshotStore,
}

impl SqliteRollbackAdapter {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            snapshot_store: SqliteSnapshotStore::new(),
        }
    }
}

#[async_trait]
impl RollbackAdapter for SqliteRollbackAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        let db_path = extract_db_path(request)?;
        let mut metadata = JsonMap::new();
        metadata.insert("db_path".to_string(), serde_json::json!(db_path));

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
        let db_path = extract_db_path_from_contract(contract)?;

        // Check if this is a multi-row transaction payload
        if let Some(rows) = payload.get("rows").and_then(|r| r.as_array()) {
            execute_multi_row_transaction(&self.snapshot_store, contract, &db_path, rows).await
        } else {
            // Legacy single-row payload
            execute_single_row(&self.snapshot_store, contract, &db_path, payload).await
        }
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let db_path = extract_db_path_from_contract(contract)?;
        let execution_id = contract.execution_id.to_string();
        let snapshots = self.snapshot_store.snapshots_for_execution(&execution_id);

        let mut verified = true;
        let mut conn = connect_sqlite(&db_path).await?;
        let mut metadata = JsonMap::new();

        for snapshot in &snapshots {
            ensure_safe_identifier(&snapshot.table)?;
            let current = fetch_content(&mut conn, &snapshot.table, &snapshot.row_id).await?;
            let matches = current.as_deref() == Some(snapshot.current_content.as_str());
            if !matches {
                verified = false;
            }
            metadata.insert(
                format!("{}:{}", snapshot.table, snapshot.row_id),
                serde_json::json!({
                    "matches": matches,
                    "present": current.is_some(),
                }),
            );
        }

        Ok(VerifyReceipt {
            verified,
            adapter_metadata: metadata,
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        recover_snapshots(&self.snapshot_store, contract).await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        recover_snapshots(&self.snapshot_store, contract).await
    }
}

/// Execute a multi-row transaction atomically.
/// All row operations are wrapped in a single SQLite transaction.
async fn execute_multi_row_transaction(
    snapshot_store: &SqliteSnapshotStore,
    contract: &RollbackContract,
    db_path: &str,
    rows: &[serde_json::Value],
) -> Result<ExecuteReceipt, AdapterError> {
    let mut conn = connect_sqlite(db_path).await?;

    // Begin transaction for atomicity
    let mut tx = conn
        .begin()
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to begin transaction: {}", err)))?;

    let execution_id = contract.execution_id.to_string();
    let mut row_count = 0;

    for row_value in rows {
        let row_op: RowOp = serde_json::from_value(row_value.clone())
            .map_err(|e| AdapterError::Validation(format!("Invalid row operation: {}", e)))?;

        ensure_safe_identifier(&row_op.table)?;

        // Ensure table exists
        ensure_table_on_tx(&mut tx, &row_op.table).await?;

        // Fetch original content and save snapshot
        let original_content = fetch_content_on_tx(&mut tx, &row_op.table, &row_op.row_id).await?;

        snapshot_store.save_snapshot(
            &execution_id,
            RowSnapshot {
                table: row_op.table.clone(),
                row_id: row_op.row_id.clone(),
                original_content,
                current_content: row_op.content.clone(),
            },
        );

        // Upsert the row
        upsert_content_on_tx(&mut tx, &row_op.table, &row_op.row_id, &row_op.content).await?;
        row_count += 1;
    }

    // Commit transaction
    tx.commit()
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to commit transaction: {}", err)))?;

    let mut metadata = JsonMap::new();
    metadata.insert("db_path".to_string(), serde_json::json!(db_path));
    metadata.insert("row_count".to_string(), serde_json::json!(row_count));

    Ok(ExecuteReceipt {
        external_id: Some(format!("sqlite:multi-row-txn:{}rows", row_count)),
        result_digest: Some(format!("sqlite-multi-row:{}", row_count)),
        adapter_metadata: metadata,
    })
}

/// Execute a legacy single-row operation (backward compatible).
async fn execute_single_row(
    snapshot_store: &SqliteSnapshotStore,
    contract: &RollbackContract,
    db_path: &str,
    payload: &serde_json::Value,
) -> Result<ExecuteReceipt, AdapterError> {
    let table = extract_required_string(payload, "table")?;
    let row_id = extract_required_string(payload, "row_id")?;
    let content = extract_required_string(payload, "content")?;

    ensure_safe_identifier(&table)?;

    let mut conn = connect_sqlite(db_path).await?;
    ensure_table(&mut conn, &table).await?;
    let original_content = fetch_content(&mut conn, &table, &row_id).await?;

    snapshot_store.save_snapshot(
        &contract.execution_id.to_string(),
        RowSnapshot {
            table: table.clone(),
            row_id: row_id.clone(),
            original_content,
            current_content: content.clone(),
        },
    );

    upsert_content(&mut conn, &table, &row_id, &content).await?;

    let mut metadata = JsonMap::new();
    metadata.insert("db_path".to_string(), serde_json::json!(db_path));
    metadata.insert("table".to_string(), serde_json::json!(table));
    metadata.insert("row_id".to_string(), serde_json::json!(row_id));

    Ok(ExecuteReceipt {
        external_id: Some(format!("sqlite:{}/{}", table, row_id)),
        result_digest: Some(format!("sqlite-row:{}", content.len())),
        adapter_metadata: metadata,
    })
}

async fn recover_snapshots(
    snapshot_store: &SqliteSnapshotStore,
    contract: &RollbackContract,
) -> Result<RecoveryReceipt, AdapterError> {
    let db_path = extract_db_path_from_contract(contract)?;
    let execution_id = contract.execution_id.to_string();
    let snapshots = snapshot_store.snapshots_for_execution(&execution_id);

    if snapshots.is_empty() {
        return Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: JsonMap::new(),
        });
    }

    let mut conn = connect_sqlite(&db_path).await?;

    // Begin transaction for atomic rollback
    let mut tx = conn
        .begin()
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to begin transaction: {}", err)))?;

    for snapshot in &snapshots {
        ensure_safe_identifier(&snapshot.table)?;
        ensure_table_on_tx(&mut tx, &snapshot.table).await?;
        match &snapshot.original_content {
            Some(original_content) => {
                upsert_content_on_tx(&mut tx, &snapshot.table, &snapshot.row_id, original_content)
                    .await?;
            }
            None => {
                delete_row_on_tx(&mut tx, &snapshot.table, &snapshot.row_id).await?;
            }
        }
    }

    // Commit transaction
    tx.commit()
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to commit transaction: {}", err)))?;

    snapshot_store.clear_snapshots(&execution_id);

    Ok(RecoveryReceipt {
        recovered: true,
        adapter_metadata: JsonMap::new(),
    })
}

fn extract_db_path(request: &RollbackPrepareRequest) -> Result<String, AdapterError> {
    match &request.target {
        RollbackTarget::SqliteTxn { db_path, .. } => Ok(db_path.clone()),
        _ => request
            .metadata
            .get("db_path")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| AdapterError::Validation("SQLite target requires db_path".to_string())),
    }
}

fn extract_db_path_from_contract(contract: &RollbackContract) -> Result<String, AdapterError> {
    match &contract.target {
        RollbackTarget::SqliteTxn { db_path, .. } => Ok(db_path.clone()),
        _ => contract
            .metadata
            .get("db_path")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| {
                AdapterError::Validation("SQLite contract requires db_path metadata".to_string())
            }),
    }
}

fn extract_required_string(payload: &serde_json::Value, key: &str) -> Result<String, AdapterError> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| AdapterError::Validation(format!("Missing '{}' in payload", key)))
}

fn ensure_safe_identifier(identifier: &str) -> Result<(), AdapterError> {
    let is_valid = !identifier.is_empty()
        && identifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if is_valid {
        Ok(())
    } else {
        Err(AdapterError::Validation(format!(
            "Unsafe sqlite identifier: {}",
            identifier
        )))
    }
}

fn sqlite_url(db_path: &str) -> String {
    if db_path.starts_with("sqlite:") {
        db_path.to_string()
    } else {
        format!("sqlite://{}", db_path)
    }
}

async fn connect_sqlite(db_path: &str) -> Result<SqliteConnection, AdapterError> {
    SqliteConnection::connect(&sqlite_url(db_path))
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to connect to sqlite: {}", err)))
}

// ============================================
// Transaction-aware helpers (for multi-row ops)
// ============================================

async fn ensure_table_on_tx<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    table: &str,
) -> Result<(), AdapterError> {
    let statement =
        format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, content TEXT NOT NULL)");
    sqlx::query(&statement)
        .execute(&mut **tx)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to ensure sqlite table: {}", err)))?;
    Ok(())
}

async fn fetch_content_on_tx<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    table: &str,
    row_id: &str,
) -> Result<Option<String>, AdapterError> {
    let statement = format!("SELECT content FROM {table} WHERE id = ?1");
    let row = sqlx::query(&statement)
        .bind(row_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to fetch sqlite row: {}", err)))?;
    Ok(row.map(|r| r.get::<String, _>(0)))
}

async fn upsert_content_on_tx<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    table: &str,
    row_id: &str,
    content: &str,
) -> Result<(), AdapterError> {
    let statement = format!(
        "INSERT INTO {table} (id, content) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET content = excluded.content"
    );
    sqlx::query(&statement)
        .bind(row_id)
        .bind(content)
        .execute(&mut **tx)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to upsert sqlite row: {}", err)))?;
    Ok(())
}

async fn delete_row_on_tx<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    table: &str,
    row_id: &str,
) -> Result<(), AdapterError> {
    let statement = format!("DELETE FROM {table} WHERE id = ?1");
    sqlx::query(&statement)
        .bind(row_id)
        .execute(&mut **tx)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to delete sqlite row: {}", err)))?;
    Ok(())
}

// ============================================
// Legacy helpers (for single-row ops)
// ============================================

async fn ensure_table(conn: &mut SqliteConnection, table: &str) -> Result<(), AdapterError> {
    let statement =
        format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, content TEXT NOT NULL)");
    sqlx::query(&statement)
        .execute(&mut *conn)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to ensure sqlite table: {}", err)))?;
    Ok(())
}

async fn fetch_content(
    conn: &mut SqliteConnection,
    table: &str,
    row_id: &str,
) -> Result<Option<String>, AdapterError> {
    let statement = format!("SELECT content FROM {table} WHERE id = ?1");
    let row = sqlx::query(&statement)
        .bind(row_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to fetch sqlite row: {}", err)))?;
    Ok(row.map(|r| r.get::<String, _>(0)))
}

async fn upsert_content(
    conn: &mut SqliteConnection,
    table: &str,
    row_id: &str,
    content: &str,
) -> Result<(), AdapterError> {
    let statement = format!(
        "INSERT INTO {table} (id, content) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET content = excluded.content"
    );
    sqlx::query(&statement)
        .bind(row_id)
        .bind(content)
        .execute(&mut *conn)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to upsert sqlite row: {}", err)))?;
    Ok(())
}

#[allow(dead_code)]
async fn delete_row(
    conn: &mut SqliteConnection,
    table: &str,
    row_id: &str,
) -> Result<(), AdapterError> {
    let statement = format!("DELETE FROM {table} WHERE id = ?1");
    sqlx::query(&statement)
        .bind(row_id)
        .execute(&mut *conn)
        .await
        .map_err(|err| AdapterError::Internal(format!("Failed to delete sqlite row: {}", err)))?;
    Ok(())
}
