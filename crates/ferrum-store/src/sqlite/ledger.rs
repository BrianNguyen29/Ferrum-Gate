use async_trait::async_trait;
use ferrum_proto::EventId;
use sqlx::{Row, SqlitePool};
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{LedgerEntry, LedgerRepo, Result};

#[derive(Clone)]
pub struct SqliteLedgerRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteLedgerRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            write_queue: None,
        }
    }

    pub fn with_write_queue(mut self, queue: WriteQueue) -> Self {
        self.write_queue = Some(queue);
        self
    }
}

#[async_trait]
impl LedgerRepo for SqliteLedgerRepo {
    async fn append(&self, entry: &LedgerEntry) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::AppendLedger {
                entry: entry.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
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
        let rows =
            sqlx::query("SELECT raw_json FROM ledger_entries ORDER BY entry_id DESC LIMIT ?1")
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

    async fn get_latest(&self) -> Result<Option<LedgerEntry>> {
        let rows =
            sqlx::query("SELECT raw_json FROM ledger_entries ORDER BY entry_id DESC LIMIT 1")
                .fetch_all(&self.pool)
                .await?;

        rows.into_iter()
            .next()
            .map(|row| -> Result<LedgerEntry> {
                let raw_json: String = row.try_get("raw_json")?;
                Ok(serde_json::from_str(&raw_json)?)
            })
            .transpose()
    }

    /// Verify the ledger chain integrity.
    ///
    /// Reads all ledger entries ordered by entry_id ASC and validates:
    /// - Empty ledger is valid.
    /// - First entry must have `previous_ledger_hash = None`.
    /// - Each subsequent entry's `previous_ledger_hash` must equal the prior entry's `content_hash`.
    /// - Entries after genesis must have both `content_hash` and `previous_ledger_hash`.
    async fn verify_chain(&self) -> Result<()> {
        let rows = sqlx::query(
            "SELECT entry_id, content_hash, previous_ledger_hash FROM ledger_entries ORDER BY entry_id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut prior_content_hash: Option<String> = None;

        for row in rows {
            let entry_id: i64 = row.get("entry_id");
            let content_hash: Option<String> = row.get("content_hash");
            let previous_ledger_hash: Option<String> = row.get("previous_ledger_hash");

            if prior_content_hash.is_none() {
                // First entry (genesis)
                if previous_ledger_hash.is_some() {
                    return Err(crate::StoreError::InvalidState(format!(
                        "ledger entry {} has previous_ledger_hash but is the genesis entry",
                        entry_id
                    )));
                }
            } else {
                // Subsequent entry - must have valid previous_ledger_hash
                let prior_hash = prior_content_hash.as_deref().ok_or_else(|| {
                    crate::StoreError::InvalidState(format!(
                        "ledger entry {} cannot verify chain: prior entry has no content_hash",
                        entry_id
                    ))
                })?;

                let prev = previous_ledger_hash.as_deref().ok_or_else(|| {
                    crate::StoreError::InvalidState(format!(
                        "ledger entry {} is missing previous_ledger_hash",
                        entry_id
                    ))
                })?;

                if prev != prior_hash {
                    return Err(crate::StoreError::InvalidState(format!(
                        "ledger entry {} has broken chain: previous_ledger_hash '{}' != prior content_hash '{}'",
                        entry_id, prev, prior_hash
                    )));
                }
            }

            // Update prior_content_hash for next iteration
            if let Some(ref ch) = content_hash {
                prior_content_hash = Some(ch.clone());
            } else if prior_content_hash.is_some() {
                // If current entry has no content_hash but prior did, subsequent entries cannot verify
                prior_content_hash = None;
            }
        }

        Ok(())
    }
}
