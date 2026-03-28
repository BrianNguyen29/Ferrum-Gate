//! PF4: Local leader allowlist for sync authorization.
//!
//! This module provides SQLite-backed storage for the PF4 leader authorization
//! allowlist. The contract is:
//!
//!   - Canonical key: `leader_address`
//!   - Local allowlist (no external capability broker)
//!   - Deny-by-default: missing entry => unauthorized (`Ok(false)`, not error)
//!   - Fail-closed on DB/read errors => `Err`
//!
//! ## Schema
//!
//! | Column         | Type    | Notes                                      |
//! |----------------|---------|--------------------------------------------|
//! | leader_address | TEXT    | PRIMARY KEY (stable identifier)            |
//! | authorized     | INTEGER | NOT NULL DEFAULT 0 (0=false, 1=true)       |
//! | added_at       | TEXT    | NOT NULL (ISO8601 UTC timestamp)           |
//!
//! ## Design Decisions
//!
//! - `leader_address` is the primary key because it is the stable identifier
//!   available at the transport boundary (consistent with `leader_tips` table).
//! - `authorized` is a narrow integer boolean (0/1) rather than a TEXT flag,
//!   matching the pattern used in `sync_state`.
//! - No separate `leader_identity` concept; `leader_address` IS the identity
//!   key for PF4 purposes.
//! - The table starts empty; `authorize_leader_test_only()` seeds it for tests.
//!
//! ## What Is NOT Here
//!
//! - Write/apply path (future slice; transport adapter populates this table)
//! - Capability lease or expiry semantics (future slice)
//! - Integration with the generic `capabilities` table (explicitly separate)

use chrono::Utc;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

// ---------------------------------------------------------------------------
// LeaderAllowlist — concrete allowlist backed by SQLite
// ---------------------------------------------------------------------------

/// Concrete leader allowlist backed by SQLite.
///
/// All methods are async and can be called directly from async contexts.
/// This struct is contained by `SqliteSyncPreflightRepo`; prefer accessing
/// it through the repo's async methods in production code.
#[derive(Clone)]
pub struct LeaderAllowlist {
    pool: SqlitePool,
}

impl LeaderAllowlist {
    /// Create a new `LeaderAllowlist` backed by the given pool.
    ///
    /// The caller is responsible for ensuring migrations have been applied.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Check whether the given leader address is authorized for sync.
    ///
    /// Returns `Ok(true)` if the leader is in the allowlist with `authorized=1`,
    /// `Ok(false)` if the leader is not in the allowlist (deny-by-default),
    /// and `Err` on database errors (fail-closed).
    ///
    /// ## Deny-by-Default Contract
    ///
    /// A missing entry is NOT an error. It means the leader is not authorized,
    /// so `Ok(false)` is returned. This is intentional: an unknown leader should
    /// not cause a preflight failure due to a repo error, but should simply
    /// be denied authorization.
    ///
    /// ## Fail-Closed Contract
    ///
    /// If the database query fails (e.g., pool unavailable), this returns `Err`.
    /// Callers must treat this as a preflight failure (PF4), not as "denied".
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
            // Missing entry: deny-by-default => not authorized (Ok(false), not Err)
            None => Ok(false),
        }
    }

    /// Authorize a leader address (upsert).
    ///
    /// If the leader is already in the allowlist, its `authorized` flag is
    /// set to 1. If it is not yet in the allowlist, it is inserted with
    /// `authorized=1`.
    ///
    /// This is a narrow test-only helper. The production transport adapter
    /// that would normally call this is not yet implemented.
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

    /// Remove a leader from the allowlist (delete).
    ///
    /// This is a narrow test-only helper. The production transport adapter
    /// that would normally call this is not yet implemented.
    pub async fn deauthorize(&self, leader_address: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM leader_allowlist WHERE leader_address = ?")
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

    #[tokio::test]
    async fn is_authorized_returns_false_for_unknown_leader() {
        // Deny-by-default: unknown leader => Ok(false), not Err
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool);
        let result = allowlist.is_authorized("unknown-leader").await;
        assert!(
            result.is_ok(),
            "is_authorized must not return Err for unknown leader"
        );
        assert!(!result.unwrap(), "unknown leader must be denied (false)");
    }

    #[tokio::test]
    async fn is_authorized_returns_true_for_authorized_leader() {
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool);

        // Authorize a leader
        allowlist.authorize("leader:9000").await.expect("authorize");

        let result = allowlist.is_authorized("leader:9000").await;
        assert!(result.is_ok());
        assert!(result.unwrap(), "authorized leader must return true");
    }

    #[tokio::test]
    async fn is_authorized_returns_false_after_deauthorize() {
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool);

        // Authorize then deauthorize
        allowlist.authorize("leader:9000").await.expect("authorize");
        allowlist
            .deauthorize("leader:9000")
            .await
            .expect("deauthorize");

        let result = allowlist.is_authorized("leader:9000").await;
        assert!(result.is_ok());
        assert!(!result.unwrap(), "deauthorized leader must return false");
    }

    #[tokio::test]
    async fn is_authorized_returns_false_for_explicitly_unauthorized_entry() {
        // Even if the leader exists in the table with authorized=0
        let store = SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect");
        store.apply_embedded_migrations().await.expect("migrations");
        let pool = store.pool().clone();
        let allowlist = LeaderAllowlist::new(pool.clone());

        // Manually insert a row with authorized=0 via raw SQL
        sqlx::query(
            "INSERT OR REPLACE INTO leader_allowlist (leader_address, authorized, added_at) VALUES (?, 0, ?)",
        )
        .bind("leader:9000")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("manual insert");

        let result = allowlist.is_authorized("leader:9000").await;
        assert!(result.is_ok());
        assert!(
            !result.unwrap(),
            "explicitly unauthorized (authorized=0) leader must return false"
        );
    }

    #[tokio::test]
    async fn authorize_replaces_existing_authorized_flag() {
        let pool = make_pool().await;
        let allowlist = LeaderAllowlist::new(pool.clone());

        // Insert as unauthorized
        sqlx::query(
            "INSERT OR REPLACE INTO leader_allowlist (leader_address, authorized, added_at) VALUES (?, 0, ?)",
        )
        .bind("leader:9000")
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("manual insert");

        // Authorize via helper
        allowlist.authorize("leader:9000").await.expect("authorize");

        let result = allowlist.is_authorized("leader:9000").await;
        assert!(result.is_ok());
        assert!(
            result.unwrap(),
            "after authorize(), leader must be authorized"
        );
    }
}
