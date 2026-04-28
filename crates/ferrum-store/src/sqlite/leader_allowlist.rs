//! PF4: Local leader allowlist for sync authorization.
//!
//! This module provides SQLite-backed storage for the PF4 leader authorization
//! allowlist. The contract is:
//!
//!   - Canonical key: `leader_address`
//!   - Local allowlist (no external capability broker)
//!   - Deny-by-default: missing entry => unauthorized (`Ok(false)`, not error)
//!   - Fail-closed on DB/read errors => `Err`

#[cfg(test)]
use chrono::Utc;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

/// Concrete leader allowlist backed by SQLite.
#[derive(Clone)]
pub struct LeaderAllowlist {
    pool: SqlitePool,
}

impl LeaderAllowlist {
    /// Create a new `LeaderAllowlist` backed by the given pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Check whether the given leader address is authorized for sync.
    ///
    /// Returns `Ok(true)` if authorized, `Ok(false)` if not authorized (deny-by-default),
    /// and `Err` on database errors (fail-closed).
    pub async fn is_authorized(&self, leader_address: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT authorized FROM leader_allowlist WHERE leader_address = ?")
            .bind(leader_address)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => {
                let authorized: i32 = r.get("authorized");
                Ok(authorized != 0)
            }
            None => Ok(false),
        }
    }

    /// Authorize a leader address (upsert with authorized=1).
    #[cfg(test)]
    pub async fn authorize(&self, leader_address: &str) -> Result<(), sqlx::Error> {
        let added_at = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT OR REPLACE INTO leader_allowlist (leader_address, authorized, added_at) VALUES (?, 1, ?)",
        )
        .bind(leader_address)
        .bind(&added_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Remove a leader from the allowlist.
    #[cfg(test)]
    pub async fn deauthorize(&self, leader_address: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM leader_allowlist WHERE leader_address = ?")
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

    #[tokio::test]
    async fn returns_false_for_unknown_leader() {
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool);
        let result = allowlist.is_authorized("unknown-leader").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn returns_true_for_authorized_leader() {
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool);

        allowlist.authorize("leader:9000").await.expect("authorize");
        let result = allowlist.is_authorized("leader:9000").await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn returns_false_after_deauthorize() {
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool);

        allowlist.authorize("leader:9000").await.expect("authorize");
        allowlist
            .deauthorize("leader:9000")
            .await
            .expect("deauthorize");

        let result = allowlist.is_authorized("leader:9000").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
