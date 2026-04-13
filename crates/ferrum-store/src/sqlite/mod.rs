mod approvals;
mod capabilities;
mod executions;
mod helpers;
mod intents;
mod leader_allowlist;
pub mod leader_tip_cache;
mod ledger;
mod migrations;
mod policy_bundles;
mod proposals;
mod provenance;
mod rollback;
mod sync_preflight;

#[cfg(test)]
mod tests;

pub use approvals::SqliteApprovalRepo;
pub use capabilities::SqliteCapabilityRepo;
pub use executions::SqliteExecutionRepo;
pub use intents::SqliteIntentRepo;
pub use ledger::SqliteLedgerRepo;
pub use policy_bundles::SqlitePolicyBundleRepo;
pub use proposals::SqliteProposalRepo;
pub use provenance::SqliteProvenanceRepo;
pub use rollback::SqliteRollbackRepo;
pub use sync_preflight::SqliteSyncPreflightRepo;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Duration;

use crate::{CapabilityRepo, ExecutionRepo, Result};

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = if database_url == ":memory:" {
            SqlitePoolOptions::new()
                .max_connections(5)
                .connect(database_url)
                .await?
        } else {
            let options = SqliteConnectOptions::from_str(database_url)?
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_millis(5000))
                .create_if_missing(true)
                .foreign_keys(true);
            SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(options)
                .await?
        };

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn apply_embedded_migrations(&self) -> Result<()> {
        // Enable foreign key enforcement for this connection.
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await?;

        let mut tx = self.pool.begin().await?;
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

    /// Create a `SqliteSyncPreflightRepo` backed by this store.
    ///
    /// The returned repo implements `ferrum_sync::SyncPreflightRepo`.
    /// See `sync_preflight` module docs for which methods are real vs
    /// currently unsupported.
    pub fn sync_preflight(&self) -> sync_preflight::SqliteSyncPreflightRepo {
        sync_preflight::SqliteSyncPreflightRepo::new(self.clone())
    }

    /// Reconcile legacy split-brain state: for any capability that is Active in SQLite
    /// but already has execution history, transition it to Used.
    /// This is fail-closed: if reconciliation fails, an error is returned.
    pub async fn reconcile_capabilities_with_executions(&self) -> Result<usize> {
        let active_capabilities = self.capabilities().list_active().await?;
        let mut reconciled_count = 0;

        for capability in active_capabilities {
            let executions = self
                .executions()
                .list_by_capability(capability.capability_id)
                .await?;
            if !executions.is_empty()
                && self
                    .capabilities()
                    .mark_used_if_active(capability.capability_id)
                    .await?
            {
                reconciled_count += 1;
            }
        }

        Ok(reconciled_count)
    }

    /// Verifies the persisted ledger chain integrity after loading from storage.
    ///
    /// This method performs TWO layers of verification to defend against different tamper vectors:
    ///
    /// 1. **DB Column Cross-Check**: Reads `content_hash` and `previous_ledger_hash` columns directly
    ///    from SQLite and validates them against the deserialized entry's `entry_hash` and `prev_hash`.
    ///    This catches tampering of the raw_json column while leaving hash columns untouched, or vice versa.
    ///
    /// 2. **Chain Linkage Verification**: Rebuilds an in-memory ledger using
    ///    [`ferrum_ledger::InMemoryLedger::load_entries`] and calls [`ferrum_ledger::InMemoryLedger::verify_chain`]
    ///    to validate sequence ordering and prev_hash linkage.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::StoreError`] wrapping the [`ferrum_ledger::LedgerError`] if:
    /// - A sequence number does not match its position in the chain
    /// - A `prev_hash` does not match the previous entry's hash (broken chain)
    /// - An entry's hash does not match the recomputed hash (tamper detected)
    /// - The persisted `content_hash` column does not match the recomputed entry hash (DB column tamper)
    /// - The persisted `previous_ledger_hash` column does not match the entry's `prev_hash` (DB column tamper)
    ///
    /// # Use
    ///
    /// Call this after `apply_embedded_migrations()` and before opening the gateway
    /// for new appends. If verification fails, refuse to start (fail-closed).
    pub async fn verify_ledger_chain(&self) -> Result<()> {
        use ferrum_ledger::{InMemoryLedger, LedgerEntry};
        use sqlx::Row;

        // Step 1: Read entries with raw_json AND the persisted hash columns directly from DB.
        // This cross-check defends against tampering that only modifies raw_json or only modifies
        // the hash columns, without modifying the other.
        let rows = sqlx::query(
            "SELECT entry_id, content_hash, previous_ledger_hash, raw_json
             FROM ledger_entries ORDER BY entry_id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(());
        }

        let mut entries = Vec::with_capacity(rows.len());
        for row in &rows {
            let content_hash: Option<String> = row.try_get("content_hash")?;
            let previous_ledger_hash: Option<String> = row.try_get("previous_ledger_hash")?;
            let raw_json: String = row.try_get("raw_json")?;

            let entry: LedgerEntry =
                serde_json::from_str(&raw_json).map_err(crate::StoreError::Serialization)?;

            // Cross-check: persisted content_hash must match the entry's recomputed entry_hash.
            // We recompute by hashing the event content + prev_hash (same as LedgerEntry::from_event).
            let recomputed_hash = ferrum_ledger::compute_entry_hash_raw(&entry);
            if content_hash.as_deref() != Some(&recomputed_hash) {
                return Err(crate::StoreError::Other(anyhow::anyhow!(
                    "ledger chain verification failed: content_hash column ({}) does not match recomputed entry hash ({}) for sequence {}",
                    content_hash.unwrap_or_default(),
                    recomputed_hash,
                    entry.sequence
                )));
            }

            // Cross-check: persisted previous_ledger_hash must match entry's prev_hash.
            let prev_hash_str = entry.prev_hash.as_deref();
            if previous_ledger_hash.as_deref() != prev_hash_str {
                return Err(crate::StoreError::Other(anyhow::anyhow!(
                    "ledger chain verification failed: previous_ledger_hash column ({}) does not match entry prev_hash ({}) for sequence {}",
                    previous_ledger_hash.unwrap_or_default(),
                    prev_hash_str.unwrap_or("None"),
                    entry.sequence
                )));
            }

            entries.push(entry);
        }

        // Step 2: Verify chain linkage using the in-memory ledger.
        let ledger = InMemoryLedger::load_entries(entries);
        ledger.verify_chain().map_err(|e| {
            crate::StoreError::Other(anyhow::anyhow!("ledger chain verification failed: {}", e))
        })?;

        Ok(())
    }
}
