//! Concrete SQLite-backed implementation of `SyncPreflightRepo`.
//!
//! This module wires `ferrum-sync`'s `SyncPreflightRepo` trait to the real
//! `SqliteStore` backend.
//!
//! ## What Is Real (Backed by Actual Schema)
//!
//! - **`verify_local_chain()`** (PF1 + PF5): Currently a stub. Full verification
//!   requires `ferrum_ledger::LedgerEntry` with `sequence` and `entry_hash` fields.
//!   Returns `Ok(())` unconditionally.
//!
//! - **`read_local_state()`** (PF2/PF6/PF7 + follower tip): Reads the three boolean
//!   flags from `sync_state` and the follower tip from `ledger_entries`.
//!
//! - **`read_leader_tip()`** (PF8): Reads from `leader_tips` cache table.
//!
//! - **`is_leader_authorized()`** (PF4): Queries `leader_allowlist` table.

use ferrum_sync::decision::TipId;
use ferrum_sync::repo::{LocalPreflightState, SyncPreflightRepo, SyncRepoError};
use sqlx::Row;

use crate::repos::LedgerRepo;
use crate::sqlite::SqliteStore;
use crate::sqlite::leader_allowlist::LeaderAllowlist;
use crate::sqlite::leader_tip_cache::LeaderTipCache;

/// Convert a `LedgerEntry` to a `TipId`.
///
/// Uses `entry_id - 1` as the zero-based sequence number.
fn ledger_entry_to_tip(entry: &crate::repos::LedgerEntry) -> TipId {
    TipId {
        sequence: entry.entry_id.saturating_sub(1) as u64,
        hash: entry
            .content_hash
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

/// Concrete SQLite-backed `SyncPreflightRepo`.
#[derive(Clone)]
pub struct SqliteSyncPreflightRepo {
    store: SqliteStore,
    leader_tip_cache: LeaderTipCache,
    leader_allowlist: LeaderAllowlist,
}

impl SqliteSyncPreflightRepo {
    /// Create a new `SqliteSyncPreflightRepo` backed by the given `SqliteStore`.
    pub fn new(store: SqliteStore) -> Self {
        Self {
            leader_tip_cache: LeaderTipCache::new(store.pool().clone()),
            leader_allowlist: LeaderAllowlist::new(store.pool().clone()),
            store,
        }
    }

    /// Get the leader tip cache.
    pub fn leader_tip_cache(&self) -> &LeaderTipCache {
        &self.leader_tip_cache
    }

    /// Internal async implementation of `verify_local_chain`.
    ///
    /// Currently a stub: full chain verification requires `ferrum_ledger::LedgerEntry`
    /// with `sequence` and `entry_hash` fields.
    pub async fn verify_local_chain_async(&self) -> Result<(), SyncRepoError> {
        if let Err(e) = self.store.verify_ledger_chain().await {
            return Err(SyncRepoError::ChainIntegrityFailed {
                reason: format!("verify_ledger_chain failed: {e}"),
            });
        }
        if let Err(e) = self.store.ledger().get_latest().await {
            return Err(SyncRepoError::LedgerNotReadable {
                reason: format!("get_latest failed: {e}"),
            });
        }
        Ok(())
    }

    /// Internal async implementation of `read_local_state`.
    pub async fn read_local_state_async(&self) -> Result<LocalPreflightState, SyncRepoError> {
        // Read PF2/PF6/PF7 flags from sync_state
        let row = sqlx::query(
            "SELECT has_inflight_commits, has_uncommitted_entries, sync_in_progress FROM sync_state WHERE id = 1",
        )
        .fetch_optional(self.store.pool())
        .await
        .map_err(|e| SyncRepoError::InternalError {
            reason: format!("sync_state query failed: {}", e),
        })?;

        let (has_inflight_commits, has_uncommitted_entries, sync_in_progress) = match row {
            Some(r) => {
                let has_inflight: i32 = r.get("has_inflight_commits");
                let has_uncommitted: i32 = r.get("has_uncommitted_entries");
                let sync_active: i32 = r.get("sync_in_progress");
                (has_inflight != 0, has_uncommitted != 0, sync_active != 0)
            }
            None => {
                return Err(SyncRepoError::InternalError {
                    reason: "sync_state row missing (id=1); cannot read PF2/PF6/PF7 flags"
                        .to_string(),
                });
            }
        };

        // Read follower tip from ledger_entries
        let follower_tip = self
            .store
            .ledger()
            .get_latest()
            .await
            .map_err(|e| SyncRepoError::InternalError {
                reason: format!("get_latest for follower tip failed: {}", e),
            })?
            .map(|entry| ledger_entry_to_tip(&entry));

        Ok(LocalPreflightState {
            follower_tip,
            has_inflight_commits,
            has_uncommitted_entries,
            sync_in_progress,
        })
    }

    /// Internal async implementation of `is_leader_authorized`.
    pub async fn is_leader_authorized_async(
        &self,
        leader_address: &str,
    ) -> Result<bool, SyncRepoError> {
        self.leader_allowlist
            .is_authorized(leader_address)
            .await
            .map_err(|e| SyncRepoError::InternalError {
                reason: format!("leader_allowlist query failed: {}", e),
            })
    }

    /// Set sync flags for testing.
    #[cfg(test)]
    pub async fn set_sync_flags_test_only(
        &self,
        has_inflight_commits: bool,
        has_uncommitted_entries: bool,
        sync_in_progress: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE sync_state SET has_inflight_commits = ?, has_uncommitted_entries = ?, sync_in_progress = ? WHERE id = 1",
        )
        .bind(if has_inflight_commits { 1 } else { 0 })
        .bind(if has_uncommitted_entries { 1 } else { 0 })
        .bind(if sync_in_progress { 1 } else { 0 })
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    /// Write a leader tip to the cache for testing.
    #[cfg(test)]
    pub async fn write_leader_tip_test_only(
        &self,
        leader_address: &str,
        tip: &TipId,
    ) -> Result<(), super::leader_tip_cache::CacheWriteError> {
        self.leader_tip_cache.write(leader_address, tip).await
    }

    /// Authorize a leader for testing.
    #[cfg(test)]
    pub async fn authorize_leader_test_only(
        &self,
        leader_address: &str,
    ) -> Result<(), sqlx::Error> {
        self.leader_allowlist.authorize(leader_address).await
    }
}

impl SyncPreflightRepo for SqliteSyncPreflightRepo {
    fn verify_local_chain(&self) -> Result<(), SyncRepoError> {
        // Sync blocking - use block_in_place to call async from sync
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.verify_local_chain_async())
        })
    }

    fn read_local_state(&self) -> Result<LocalPreflightState, SyncRepoError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.read_local_state_async())
        })
    }

    fn is_leader_authorized(&self, leader_identity: &str) -> Result<bool, SyncRepoError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.is_leader_authorized_async(leader_identity))
        })
    }

    fn read_leader_tip(&self, leader_address: &str) -> Result<Option<TipId>, SyncRepoError> {
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.leader_tip_cache.read(leader_address))
        })
        .map_err(|e| SyncRepoError::InternalError {
            reason: format!("leader_tip_cache read failed: {}", e),
        })
    }

    fn verify_local_chain_for_hash_path_valid(&self) -> bool {
        self.verify_local_chain().is_ok()
    }
}
