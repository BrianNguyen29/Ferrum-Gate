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
//! ## Write Safety
//!
//! The `write` method enforces monotonicity guards to prevent regression:
//! - Stale writes (lower or equal sequence than cached) are rejected with `CacheWriteError`.
//! - Conflicting writes (same sequence, different hash) are rejected with `CacheWriteError`.
//! - Only strictly newer tips (higher sequence) are accepted.
//! - Idempotent writes (same sequence, same hash) are accepted (no-op).
//!
//! ## What Is NOT Here
//!
//! - Tip staleness enforcement (future slice)
//! - Multiple leader support (future slice; currently one leader at a time)
//! - Automatic cache eviction or TTL enforcement

use chrono::Utc;
use ferrum_sync::decision::TipId;
use sqlx::Acquire;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

/// Errors that can occur when writing to the leader tip cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheWriteError {
    /// The cached tip has a higher or equal sequence than the incoming tip.
    /// Rejecting this write prevents regression of the cached tip.
    StaleTip {
        cached_sequence: u64,
        incoming_sequence: u64,
    },
    /// The cached tip has the same sequence but a different hash.
    /// This indicates a conflicting state and must not be silently replaced.
    HashConflict {
        sequence: u64,
        cached_hash: String,
        incoming_hash: String,
    },
    /// A database error occurred during the write operation.
    DatabaseError(String),
}

impl std::fmt::Display for CacheWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheWriteError::StaleTip {
                cached_sequence,
                incoming_sequence,
            } => write!(
                f,
                "stale tip: cached seq={}, incoming seq={}",
                cached_sequence, incoming_sequence
            ),
            CacheWriteError::HashConflict {
                sequence,
                cached_hash,
                incoming_hash,
            } => write!(
                f,
                "hash conflict at seq={}: cached={}, incoming={}",
                sequence, cached_hash, incoming_hash
            ),
            CacheWriteError::DatabaseError(msg) => write!(f, "database error: {}", msg),
        }
    }
}

impl From<sqlx::Error> for CacheWriteError {
    fn from(e: sqlx::Error) -> Self {
        CacheWriteError::DatabaseError(e.to_string())
    }
}

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
                    sqlx::Error::Protocol(format!(
                        "leader_tips.sequence value {} is negative or overflows u64",
                        sequence
                    ))
                })?;
                Ok(Some(TipId { sequence, hash }))
            }
            None => Ok(None),
        }
    }

    /// Write a leader tip to the cache with monotonicity guards.
    ///
    /// Callers may call this after a successful transport probe to cache
    /// the retrieved tip for later preflight use.
    ///
    /// ## Atomicity
    ///
    /// This method uses a SQLite transaction to ensure the check-and-write is atomic.
    /// The read, comparison, and write happen within a single transaction.
    /// SQLite serializes write transactions at the database level, preventing
    /// concurrent modifications.
    ///
    /// ## Monotonicity Guards (Fail-Closed)
    ///
    /// - **Stale write rejection**: If the cached tip has sequence >= incoming sequence,
    ///   the write is rejected with `CacheWriteError::StaleTip`. This prevents
    ///   regressing the cached tip.
    /// - **Hash conflict rejection**: If the cached tip has the same sequence but
    ///   a different hash, the write is rejected with `CacheWriteError::HashConflict`.
    ///   This prevents silently replacing a valid tip with a conflicting one.
    /// - **Idempotent write**: If the cached tip has the same sequence AND same hash,
    ///   the write is accepted (no-op at the DB level).
    /// - **Strictly newer**: Only writes where incoming sequence > cached sequence are accepted.
    ///
    /// ## Error Returns
    ///
    /// Returns `Err(CacheWriteError)` for monotonicity violations.
    /// Returns `Err(sqlx::Error)` for actual database failures.
    pub async fn write(&self, leader_address: &str, tip: &TipId) -> Result<(), CacheWriteError> {
        // Acquire a connection and begin a transaction for atomic check-and-write.
        let mut conn = self.pool.acquire().await.map_err(|e| {
            CacheWriteError::DatabaseError(format!("failed to acquire connection: {}", e))
        })?;

        let mut tx = conn.begin().await.map_err(|e| {
            CacheWriteError::DatabaseError(format!("failed to begin transaction: {}", e))
        })?;

        // Read existing cached tip within the transaction
        let row = sqlx::query("SELECT sequence, hash FROM leader_tips WHERE leader_address = ?")
            .bind(leader_address)
            .fetch_optional(&mut *tx)
            .await
            .map_err(CacheWriteError::from)?;

        // Check monotonicity guards
        if let Some(r) = row {
            let cached_sequence: i64 = r.get("sequence");
            let cached_hash: String = r.get("hash");
            let cached_sequence = u64::try_from(cached_sequence).map_err(|_| {
                CacheWriteError::DatabaseError(
                    "leader_tips.sequence value is negative or overflows u64".to_string(),
                )
            })?;

            // Reject stale writes: incoming must be strictly newer (sequence must increase)
            if tip.sequence < cached_sequence {
                // Rollback and return error (fail-closed)
                tx.rollback().await.ok();
                return Err(CacheWriteError::StaleTip {
                    cached_sequence,
                    incoming_sequence: tip.sequence,
                });
            }
            // At same sequence, check hash equality for idempotency vs conflict
            if tip.sequence == cached_sequence {
                if tip.hash != cached_hash {
                    // Same sequence, different hash = hash conflict
                    tx.rollback().await.ok();
                    return Err(CacheWriteError::HashConflict {
                        sequence: tip.sequence,
                        cached_hash,
                        incoming_hash: tip.hash.clone(),
                    });
                }
                // Same sequence, same hash = idempotent write, accept but don't rewrite
                tx.rollback().await.ok();
                return Ok(());
            }
            // tip.sequence > cached_sequence: strictly newer, proceed with write
        }
        // No existing cache entry - first write is always allowed

        // Write the new tip within the transaction
        let fetched_at = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT OR REPLACE INTO leader_tips (leader_address, sequence, hash, fetched_at) VALUES (?, ?, ?, ?)",
        )
        .bind(leader_address)
        .bind(tip.sequence as i64)
        .bind(&tip.hash)
        .bind(&fetched_at)
        .execute(&mut *tx)
        .await
        .map_err(CacheWriteError::from)?;

        // Commit the transaction
        tx.commit().await.map_err(|e| {
            CacheWriteError::DatabaseError(format!("failed to commit transaction: {}", e))
        })?;

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

    #[tokio::test]
    async fn leader_tip_cache_rejects_stale_write_lower_sequence() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        // Write initial tip with seq=50
        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        // Try to write stale tip with seq=30 (lower) - should be rejected
        let tip2 = make_tip(30, "hash2");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CacheWriteError::StaleTip { .. }));

        // Cached tip should be unchanged
        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 50);
        assert_eq!(retrieved.hash, "hash1");
    }

    #[tokio::test]
    async fn leader_tip_cache_rejects_stale_write_equal_sequence() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        // Write initial tip with seq=50
        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        // Try to write tip with same seq=50 but different hash - should be rejected as hash conflict
        let tip2 = make_tip(50, "different_hash");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CacheWriteError::HashConflict { .. }));

        // Cached tip should be unchanged
        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 50);
        assert_eq!(retrieved.hash, "hash1");
    }

    #[tokio::test]
    async fn leader_tip_cache_accepts_idempotent_write() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        // Write initial tip
        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        // Write same tip again (same seq, same hash) - should be accepted (idempotent)
        let tip2 = make_tip(50, "hash1");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_ok());

        // Cached tip should be unchanged
        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 50);
        assert_eq!(retrieved.hash, "hash1");
    }

    #[tokio::test]
    async fn leader_tip_cache_accepts_strictly_newer_tip() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        // Write initial tip
        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        // Write newer tip with higher sequence - should be accepted
        let tip2 = make_tip(100, "hash2");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_ok());

        // Cached tip should be updated
        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 100);
        assert_eq!(retrieved.hash, "hash2");
    }
}
