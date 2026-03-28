use async_trait::async_trait;
use ferrum_ledger::LedgerEntry;
use ferrum_proto::{EventId, ProvenanceEvent};
use sqlx::{Row, SqlitePool};

use super::helpers::{enum_text, to_json};
use crate::{LedgerRepo, Result};

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
        // Append-time hash verification: read current tip and verify incoming entry
        // links to the correct previous hash before inserting.
        let mut tx = self.pool.begin().await?;

        // Get current tip hash (None if ledger is empty)
        let tip_hash = {
            let row = sqlx::query(
                "SELECT content_hash FROM ledger_entries ORDER BY entry_id DESC LIMIT 1",
            )
            .fetch_optional(&mut *tx)
            .await?;

            row.map(|r| -> String { r.try_get("content_hash").unwrap() })
        };

        // Verify entry's prev_hash matches the current tip using ferrum-ledger's
        // public verify_entry helper. This catches broken-chain entries before
        // they are persisted.
        ferrum_ledger::verify_entry(entry, tip_hash.as_deref()).map_err(|e| {
            crate::StoreError::Other(anyhow::anyhow!("append hash verification failed: {}", e))
        })?;

        // DB column content_hash  <-- domain field entry_hash
        // DB column previous_ledger_hash <-- domain field prev_hash
        sqlx::query(
            "INSERT INTO ledger_entries (
                event_id, intent_id, execution_id, occurred_at,
                content_hash, previous_ledger_hash, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(entry.event.event_id.to_string())
        .bind(entry.event.intent_id.map(|id| id.to_string()))
        .bind(entry.event.execution_id.map(|id| id.to_string()))
        .bind(entry.event.occurred_at)
        .bind(entry.entry_hash.as_str())
        .bind(entry.prev_hash.as_deref())
        .bind(serde_json::to_string(entry)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn append_event(&self, event: &ProvenanceEvent) -> Result<LedgerEntry> {
        // Atomic append: get tip, build entry, insert all in a single transaction.
        // The store layer owns sequencing and chain linkage.
        let mut tx = self.pool.begin().await?;

        // Get current tip to determine sequence and prev_hash.
        // ALSO verify the tip's content_hash column is valid before using it as prev_hash.
        // This catches the case where a tampered tip content_hash would otherwise cause
        // the next append to build an internally-consistent but wrong chain.
        let (sequence, prev_hash) = {
            let row = sqlx::query(
                "SELECT content_hash, raw_json FROM ledger_entries ORDER BY entry_id DESC LIMIT 1",
            )
            .fetch_optional(&mut *tx)
            .await?;

            match row {
                Some(r) => {
                    let content_hash: String = r.try_get("content_hash")?;
                    let raw_json: String = r.try_get("raw_json")?;

                    // Verify tip integrity: recompute hash and compare to persisted content_hash.
                    // This is the same cross-check done in verify_ledger_chain, but at append time
                    // to catch live tampering of the tip entry's hash column.
                    let tip_entry: LedgerEntry = serde_json::from_str(&raw_json)
                        .map_err(crate::StoreError::Serialization)?;
                    let recomputed_hash = ferrum_ledger::compute_entry_hash_raw(&tip_entry);
                    if content_hash != recomputed_hash {
                        return Err(crate::StoreError::Other(anyhow::anyhow!(
                            "append rejected: tip content_hash ({}) does not match recomputed hash ({}); possible ledger tampering",
                            content_hash,
                            recomputed_hash
                        )));
                    }

                    let entry_count: i64 = sqlx::query("SELECT COUNT(*) FROM ledger_entries")
                        .fetch_one(&mut *tx)
                        .await?
                        .try_get(0)?;
                    (entry_count as u64, Some(content_hash))
                }
                None => (0u64, None),
            }
        };

        // Use ferrum-ledger to compute the entry hash from event + prev_hash
        let entry = LedgerEntry::from_event(event.clone(), sequence, prev_hash.clone());

        // Persist the event to provenance_events first (FK dependency)
        let raw_json = to_json(event)?;
        sqlx::query(
            "INSERT INTO provenance_events (
                event_id, kind, occurred_at, intent_id, proposal_id, execution_id,
                capability_id, rollback_contract_id, policy_bundle_id, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(event.event_id.to_string())
        .bind(enum_text(&event.kind)?)
        .bind(event.occurred_at)
        .bind(event.intent_id.map(|id| id.to_string()))
        .bind(event.proposal_id.map(|id| id.to_string()))
        .bind(event.execution_id.map(|id| id.to_string()))
        .bind(event.capability_id.map(|id| id.to_string()))
        .bind(event.rollback_contract_id.map(|id| id.to_string()))
        .bind(event.policy_bundle_id.map(|id| id.to_string()))
        .bind(raw_json)
        .execute(&mut *tx)
        .await?;

        // Persist the ledger entry
        sqlx::query(
            "INSERT INTO ledger_entries (
                event_id, intent_id, execution_id, occurred_at,
                content_hash, previous_ledger_hash, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(event.event_id.to_string())
        .bind(event.intent_id.map(|id| id.to_string()))
        .bind(event.execution_id.map(|id| id.to_string()))
        .bind(event.occurred_at)
        .bind(entry.entry_hash.as_str())
        .bind(entry.prev_hash.as_deref())
        .bind(serde_json::to_string(&entry)?)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(entry)
    }

    async fn get_by_event(&self, event_id: EventId) -> Result<Option<LedgerEntry>> {
        let row = sqlx::query(
            "SELECT content_hash, previous_ledger_hash, raw_json
             FROM ledger_entries WHERE event_id = ?1",
        )
        .bind(event_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| -> Result<LedgerEntry> {
            let raw_json: String = row.try_get("raw_json")?;
            let entry: LedgerEntry = serde_json::from_str(&raw_json)?;
            Ok(entry)
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

    async fn list_all(&self) -> Result<Vec<LedgerEntry>> {
        let rows = sqlx::query("SELECT raw_json FROM ledger_entries ORDER BY entry_id ASC")
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
