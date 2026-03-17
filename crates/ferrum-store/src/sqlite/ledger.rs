use async_trait::async_trait;
use ferrum_proto::EventId;
use sqlx::{Row, SqlitePool};

use crate::{LedgerEntry, LedgerRepo, Result};

#[derive(Clone)]
pub struct SqliteLedgerRepo {
    pool: SqlitePool,
}

impl SqliteLedgerRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LedgerRepo for SqliteLedgerRepo {
    async fn append(&self, entry: &LedgerEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO ledger_entries (
                event_id, intent_id, execution_id, occurred_at,
                content_hash, previous_ledger_hash, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(entry.event_id.to_string())
        .bind(entry.intent_id.map(|id| id.to_string()))
        .bind(entry.execution_id.map(|id| id.to_string()))
        .bind(entry.occurred_at)
        .bind(&entry.content_hash)
        .bind(&entry.previous_ledger_hash)
        .bind(serde_json::to_string(entry)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_by_event(&self, event_id: EventId) -> Result<Option<LedgerEntry>> {
        let row = sqlx::query("SELECT raw_json FROM ledger_entries WHERE event_id = ?1")
            .bind(event_id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| -> Result<LedgerEntry> {
            let raw_json: String = row.try_get("raw_json")?;
            Ok(serde_json::from_str(&raw_json)?)
        })
        .transpose()
    }

    async fn list_recent(&self, limit: u32) -> Result<Vec<LedgerEntry>> {
        let rows = sqlx::query(
            "SELECT raw_json FROM ledger_entries ORDER BY entry_id DESC LIMIT ?1",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| -> Result<LedgerEntry> {
                let raw_json: String = row.try_get("raw_json")?;
                Ok(serde_json::from_str(&raw_json)?)
            })
            .collect()
    }
}
