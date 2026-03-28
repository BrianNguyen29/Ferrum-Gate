//! Leader tip cache for PF8 (Sync-2 read-only preflight).
//!
//! This module provides SQLite-backed storage for leader tips retrieved via
//! transport probe (Sync-3). The cache is consumed by `SqliteSyncPreflightRepo::read_leader_tip()`
//! to satisfy PF8 (leader tip available check) during preflight.
//!
//! ## Schema
//!
//! | Column         | Type   | Notes                              |
//! |----------------|--------|------------------------------------|
//! | leader_address | TEXT   | PRIMARY KEY (identity/lookup key)  |
//! | sequence       | INTEGER| NOT NULL (tip sequence number)     |
//! | hash           | TEXT   | NOT NULL (tip entry hash, Sha256Hex) |
//! | fetched_at     | TEXT   | NOT NULL (ISO8601 UTC timestamp)   |
//!
//! ## Design Decisions
//!
//! - `leader_address` is used as the primary key/identity because it is the
//!   stable identifier available at the transport boundary (per `ProbeFacadeRequest::leader_address`).
//!   We do NOT introduce a separate `leader_identity` concept in this slice.
//! - `fetched_at` allows callers to reason about tip staleness if needed
//!   (though staleness checking is not implemented in this slice).
//!
//! ## What Is NOT Here
//!
//! - Tip staleness enforcement (future slice)
//! - Multiple leader support (future slice; currently one leader at a time)
//! - Automatic cache eviction or TTL enforcement

use chrono::Utc;
use ferrum_sync::decision::TipId;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

// ---------------------------------------------------------------------------
// LeaderTipCache — concrete cache backed by SQLite
// ---------------------------------------------------------------------------

/// Concrete leader-tip cache backed by SQLite.
///
/// All methods are async and can be called directly from async contexts.
/// This struct is contained by `SqliteSyncPreflightRepo`; prefer accessing
/// it through the repo's async methods in production code.
#[derive(Clone)]
pub struct LeaderTipCache {
    pool: SqlitePool,
}

impl LeaderTipCache {
    /// Create a new `LeaderTipCache` backed by the given pool.
    ///
    /// The caller is responsible for ensuring migrations have been applied.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Read a cached leader tip by leader address.
    ///
    /// Returns `Ok(Some(TipId))` if a tip is cached for the given leader,
    /// `Ok(None)` if no tip is known for that leader,
    /// `Err` on database errors.
    ///
    /// ## Fail-Closed Note
    ///
    /// If the database query fails (e.g., pool unavailable), this returns `Err`.
    /// Callers must treat this as a preflight failure (PF8), not as "no tip known".
    pub async fn read(&self, leader_address: &str) -> Result<Option<TipId>, sqlx::Error> {
        let row = sqlx::query("SELECT sequence, hash FROM leader_tips WHERE leader_address = ?")
            .bind(leader_address)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => {
                let sequence: i64 = r.get("sequence");
                let hash: String = r.get("hash");
                let sequence = u64::try_from(sequence).map_err(|_| {
                    sqlx::Error::Protocol(
                        format!(
                            "leader_tips.sequence value {} is negative or overflows u64",
                            sequence
                        )
                        .into(),
                    )
                })?;
                Ok(Some(TipId { sequence, hash }))
            }
            None => Ok(None),
        }
    }

    /// Write a leader tip to the cache, upserting on conflict.
    ///
    /// If a tip already exists for `leader_address`, it is replaced.
    /// Callers may call this after a successful transport probe to cache
    /// the retrieved tip for later preflight use.
    ///
    /// ## Fail-Closed Note
    ///
    /// If the database write fails, this returns `Err`.
    /// Callers should retry or surface the error; a failed write does NOT
    /// mean the tip was not retrieved from the leader.
    pub async fn write(&self, leader_address: &str, tip: &TipId) -> Result<(), sqlx::Error> {
        let fetched_at = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT OR REPLACE INTO leader_tips (leader_address, sequence, hash, fetched_at) VALUES (?, ?, ?, ?)",
        )
        .bind(leader_address)
        .bind(tip.sequence as i64)
        .bind(&tip.hash)
        .bind(&fetched_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Remove a cached leader tip (e.g., on leader change or invalidate).
    pub async fn delete(&self, leader_address: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM leader_tips WHERE leader_address = ?")
            .bind(leader_address)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::SqliteStore;

    async fn make_pool() -> SqlitePool {
        let store = SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect");
        store.apply_embedded_migrations().await.expect("migrations");
        store.pool().clone()
    }

    fn make_tip(seq: u64, hash: &str) -> TipId {
        TipId {
            sequence: seq,
            hash: hash.to_string(),
        }
    }

    #[tokio::test]
    async fn leader_tip_cache_read_returns_none_when_not_cached() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);
        let result = cache.read("leader:9000").await.expect("query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn leader_tip_cache_write_then_read_roundtrip() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);
        let tip = make_tip(42, "abc123");

        cache.write("leader:9000", &tip).await.expect("write");

        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");

        assert_eq!(retrieved.sequence, 42);
        assert_eq!(retrieved.hash, "abc123");
    }

    #[tokio::test]
    async fn leader_tip_cache_write_replaces_existing() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        // Write initial tip
        let tip1 = make_tip(10, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        // Replace with new tip
        let tip2 = make_tip(20, "hash2");
        cache.write("leader:9000", &tip2).await.expect("write2");

        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");

        assert_eq!(retrieved.sequence, 20);
        assert_eq!(retrieved.hash, "hash2");
    }

    #[tokio::test]
    async fn leader_tip_cache_delete_removes_cached_tip() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);
        let tip = make_tip(99, "to_be_deleted");

        cache.write("leader:9000", &tip).await.expect("write");

        cache.delete("leader:9000").await.expect("delete");

        let result = cache.read("leader:9000").await.expect("read");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn leader_tip_cache_read_unknown_leader_returns_none() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        // Never written anything
        let result = cache.read("unknown-leader").await.expect("query");
        assert!(result.is_none());
    }
}
