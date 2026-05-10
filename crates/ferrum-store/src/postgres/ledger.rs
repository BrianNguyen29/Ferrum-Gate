//! PostgreSQL LedgerRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::EventId;
use sqlx::{PgPool, Row};

use crate::{LedgerEntry, LedgerRepo, Result};

#[derive(Clone)]
pub struct PostgresLedgerRepo {
    pool: PgPool,
}

impl PostgresLedgerRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LedgerRepo for PostgresLedgerRepo {
    async fn append(&self, entry: &LedgerEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO ledger_entries (
                event_id, intent_id, execution_id, occurred_at,
                content_hash, previous_ledger_hash, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(entry.event_id.to_string())
        .bind(entry.intent_id.map(|id| id.to_string()))
        .bind(entry.execution_id.map(|id| id.to_string()))
        .bind(entry.occurred_at.to_rfc3339())
        .bind(&entry.content_hash)
        .bind(&entry.previous_ledger_hash)
        .bind(serde_json::to_string(entry)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_by_event(&self, event_id: EventId) -> Result<Option<LedgerEntry>> {
        let row = sqlx::query("SELECT raw_json FROM ledger_entries WHERE event_id = $1")
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
            sqlx::query("SELECT raw_json FROM ledger_entries ORDER BY entry_id DESC LIMIT $1")
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
                if previous_ledger_hash.is_some() {
                    return Err(crate::StoreError::InvalidState(format!(
                        "ledger entry {} has previous_ledger_hash but is the genesis entry",
                        entry_id
                    )));
                }
            } else {
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

            if let Some(ref ch) = content_hash {
                prior_content_hash = Some(ch.clone());
            } else if prior_content_hash.is_some() {
                prior_content_hash = None;
            }
        }

        Ok(())
    }
}
