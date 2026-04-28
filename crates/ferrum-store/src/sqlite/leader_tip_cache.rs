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
//! | hash           | TEXT   | NOT NULL (tip entry hash)          |
//! | fetched_at     | TEXT   | NOT NULL (ISO8601 UTC timestamp)   |
//!
//! ## Write Safety
//!
//! The `write` method enforces monotonicity guards to prevent regression:
//! - Stale writes (lower or equal sequence than cached) are rejected with `CacheWriteError`.
//! - Conflicting writes (same sequence, different hash) are rejected with `CacheWriteError`.
//! - Only strictly newer tips (higher sequence) are accepted.

use chrono::Utc;
use ferrum_sync::decision::TipId;
use sqlx::Acquire;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

/// Errors that can occur when writing to the leader tip cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheWriteError {
    StaleTip {
        cached_sequence: u64,
        incoming_sequence: u64,
    },
    HashConflict {
        sequence: u64,
        cached_hash: String,
        incoming_hash: String,
    },
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

/// Concrete leader-tip cache backed by SQLite.
#[derive(Clone)]
pub struct LeaderTipCache {
    pool: SqlitePool,
}

impl LeaderTipCache {
    /// Create a new `LeaderTipCache` backed by the given pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Read a cached leader tip by leader address.
    ///
    /// Returns `Ok(Some(TipId))` if a tip is cached, `Ok(None)` if not known,
    /// `Err` on database errors (fail-closed).
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
    /// - Stale writes (sequence <= cached) are rejected.
    /// - Hash conflicts (same sequence, different hash) are rejected.
    /// - Idempotent writes (same sequence, same hash) are accepted.
    pub async fn write(&self, leader_address: &str, tip: &TipId) -> Result<(), CacheWriteError> {
        let mut conn = self.pool.acquire().await.map_err(|e| {
            CacheWriteError::DatabaseError(format!("failed to acquire connection: {}", e))
        })?;

        let mut tx = conn.begin().await.map_err(|e| {
            CacheWriteError::DatabaseError(format!("failed to begin transaction: {}", e))
        })?;

        let row = sqlx::query("SELECT sequence, hash FROM leader_tips WHERE leader_address = ?")
            .bind(leader_address)
            .fetch_optional(&mut *tx)
            .await
            .map_err(CacheWriteError::from)?;

        if let Some(r) = row {
            let cached_sequence: i64 = r.get("sequence");
            let cached_hash: String = r.get("hash");
            let cached_sequence = u64::try_from(cached_sequence).map_err(|_| {
                CacheWriteError::DatabaseError(
                    "leader_tips.sequence value is negative or overflows u64".to_string(),
                )
            })?;

            if tip.sequence < cached_sequence {
                tx.rollback().await.ok();
                return Err(CacheWriteError::StaleTip {
                    cached_sequence,
                    incoming_sequence: tip.sequence,
                });
            }
            if tip.sequence == cached_sequence {
                if tip.hash != cached_hash {
                    tx.rollback().await.ok();
                    return Err(CacheWriteError::HashConflict {
                        sequence: tip.sequence,
                        cached_hash,
                        incoming_hash: tip.hash.clone(),
                    });
                }
                // Idempotent write
                tx.rollback().await.ok();
                return Ok(());
            }
        }

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

        tx.commit().await.map_err(|e| {
            CacheWriteError::DatabaseError(format!("failed to commit transaction: {}", e))
        })?;

        Ok(())
    }

    /// Remove a cached leader tip.
    pub async fn delete(&self, leader_address: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM leader_tips WHERE leader_address = ?")
            .bind(leader_address)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

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
    async fn read_returns_none_when_not_cached() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);
        let result = cache.read("leader:9000").await.expect("query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
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
    async fn write_replaces_existing() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        let tip1 = make_tip(10, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        let tip2 = make_tip(20, "hash2");
        cache.write("leader:9000", &tip2).await.expect("write2");

        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 20);
    }

    #[tokio::test]
    async fn delete_removes_cached_tip() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);
        let tip = make_tip(99, "to_be_deleted");

        cache.write("leader:9000", &tip).await.expect("write");
        cache.delete("leader:9000").await.expect("delete");

        let result = cache.read("leader:9000").await.expect("read");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn rejects_stale_write_lower_sequence() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        let tip2 = make_tip(30, "hash2");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CacheWriteError::StaleTip { .. }));

        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 50);
    }

    #[tokio::test]
    async fn rejects_hash_conflict() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        let tip2 = make_tip(50, "different_hash");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CacheWriteError::HashConflict { .. }
        ));
    }

    #[tokio::test]
    async fn accepts_idempotent_write() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        let tip2 = make_tip(50, "hash1");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn accepts_strictly_newer_tip() {
        let pool = make_pool().await;
        let cache = LeaderTipCache::new(pool);

        let tip1 = make_tip(50, "hash1");
        cache.write("leader:9000", &tip1).await.expect("write1");

        let tip2 = make_tip(100, "hash2");
        let result = cache.write("leader:9000", &tip2).await;
        assert!(result.is_ok());

        let retrieved = cache
            .read("leader:9000")
            .await
            .expect("read")
            .expect("should be Some");
        assert_eq!(retrieved.sequence, 100);
    }
}
