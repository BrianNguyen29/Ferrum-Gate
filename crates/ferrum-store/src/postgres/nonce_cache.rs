use crate::{NonceCache, Result};
use async_trait::async_trait;
use sqlx::PgPool;
use std::time::Duration;

/// PostgreSQL-backed shared nonce cache.
///
/// Uses an atomic CAS upsert so that an expired slot can be reclaimed while an
/// unexpired slot rejects replay. Expired rows are removed periodically by
/// `vacuum`.
#[derive(Debug, Clone)]
pub struct PostgresNonceCache {
    pool: PgPool,
}

impl PostgresNonceCache {
    /// Create a new cache backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns a reference to the underlying connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl NonceCache for PostgresNonceCache {
    async fn check_and_insert(&self, nonce: &str, ttl: Duration) -> Result<bool> {
        let ttl_secs = ttl.as_secs() as i64;
        let rows: Option<i64> = sqlx::query_scalar(
            "INSERT INTO nonce_cache (nonce, expires_at)
             VALUES ($1, NOW() + MAKE_INTERVAL(secs => $2))
             ON CONFLICT (nonce) DO UPDATE
                 SET expires_at = EXCLUDED.expires_at
                 WHERE nonce_cache.expires_at <= NOW()
             RETURNING 1",
        )
        .bind(nonce)
        .bind(ttl_secs)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rows.is_some())
    }

    async fn vacuum(&self) -> Result<usize> {
        let result = sqlx::query("DELETE FROM nonce_cache WHERE expires_at <= NOW()")
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_nonce_cache_rejects_replay_and_reclaims_expired() {
        let pool = sqlx::PgPool::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        let cache = PostgresNonceCache::new(pool);

        // Ensure a clean slate for the test nonce.
        let _ = sqlx::query("DELETE FROM nonce_cache WHERE nonce = 'test-nonce'")
            .execute(cache.pool())
            .await
            .unwrap();

        let ttl = Duration::from_secs(1);
        assert!(cache.check_and_insert("test-nonce", ttl).await.unwrap());
        assert!(!cache.check_and_insert("test-nonce", ttl).await.unwrap());

        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(cache.check_and_insert("test-nonce", ttl).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL instance"]
    async fn postgres_nonce_cache_vacuum_removes_expired() {
        let pool = sqlx::PgPool::connect(
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test",
        )
        .await
        .unwrap();
        let cache = PostgresNonceCache::new(pool);

        let _ = sqlx::query("DELETE FROM nonce_cache WHERE nonce LIKE 'vacuum-%'")
            .execute(cache.pool())
            .await
            .unwrap();

        let ttl = Duration::from_millis(100);
        assert!(cache.check_and_insert("vacuum-1", ttl).await.unwrap());
        tokio::time::sleep(Duration::from_millis(150)).await;
        let removed = cache.vacuum().await.unwrap();
        assert!(removed >= 1, "expected at least one expired row removed");
    }
}
