use async_trait::async_trait;
use ferrum_proto::AuditMerkleRoot;
use sqlx::{Row, SqlitePool};

use crate::{AuditMerkleRootRepo, Result};

#[derive(Clone)]
pub struct SqliteAuditMerkleRootRepo {
    pool: SqlitePool,
}

impl SqliteAuditMerkleRootRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditMerkleRootRepo for SqliteAuditMerkleRootRepo {
    async fn compute_and_cache_root(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<AuditMerkleRoot> {
        let window_end = window_start + chrono::Duration::hours(1);

        // Check cache first to avoid redundant computation.
        if let Some(cached) = self.get_root(window_start).await? {
            return Ok(cached);
        }

        let rows = sqlx::query_scalar::<_, String>(
            "SELECT content_hash FROM audit_log WHERE content_hash IS NOT NULL AND created_at >= ?1 AND created_at < ?2 ORDER BY id ASC"
        )
        .bind(window_start.to_rfc3339())
        .bind(window_end.to_rfc3339())
        .fetch_all(&self.pool)
        .await?;

        let entry_count = rows.len() as i64;
        let root = if rows.is_empty() {
            String::new()
        } else {
            crate::merkle::compute_merkle_root(&rows)
        };

        let computed_at = chrono::Utc::now();
        sqlx::query(
            "INSERT OR IGNORE INTO audit_merkle_roots (window_start, root, entry_count, computed_at) VALUES (?1, ?2, ?3, ?4)"
        )
        .bind(window_start.to_rfc3339())
        .bind(&root)
        .bind(entry_count)
        .bind(computed_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        // Return the canonical row (handles rare race where another caller inserted).
        if let Some(cached) = self.get_root(window_start).await? {
            Ok(cached)
        } else {
            Ok(AuditMerkleRoot {
                window_start,
                root,
                entry_count,
                computed_at,
            })
        }
    }

    async fn get_root(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<AuditMerkleRoot>> {
        let row = sqlx::query(
            "SELECT window_start, root, entry_count, computed_at FROM audit_merkle_roots WHERE window_start = ?"
        )
        .bind(window_start.to_rfc3339())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let ws: String = r.get("window_start");
            let ca: String = r.get("computed_at");
            AuditMerkleRoot {
                window_start: chrono::DateTime::parse_from_rfc3339(&ws)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                root: r.get("root"),
                entry_count: r.get::<i64, _>("entry_count"),
                computed_at: chrono::DateTime::parse_from_rfc3339(&ca)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            }
        }))
    }

    async fn list_roots(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<AuditMerkleRoot>, Option<String>)> {
        let mut sql = String::from(
            "SELECT window_start, root, entry_count, computed_at FROM audit_merkle_roots WHERE 1=1",
        );
        if cursor.is_some() {
            sql.push_str(" AND window_start < ?");
        }
        sql.push_str(" ORDER BY window_start DESC LIMIT ?");

        let mut query = sqlx::query(&sql);
        if let Some(cursor) = cursor {
            query = query.bind(cursor);
        }
        query = query.bind((limit + 1) as i64);

        let rows = query.fetch_all(&self.pool).await?;
        let mut items = Vec::new();
        for row in &rows {
            let ws: String = row.get("window_start");
            let ca: String = row.get("computed_at");
            items.push(AuditMerkleRoot {
                window_start: chrono::DateTime::parse_from_rfc3339(&ws)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                root: row.get("root"),
                entry_count: row.get::<i64, _>("entry_count"),
                computed_at: chrono::DateTime::parse_from_rfc3339(&ca)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            });
        }

        let next_cursor = if items.len() > limit as usize {
            items.pop();
            items.last().map(|i| i.window_start.to_rfc3339())
        } else {
            None
        };

        Ok((items, next_cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repos::AuditLogRepo;
    use crate::sqlite::SqliteStore;
    use chrono::DurationRound;
    use ferrum_proto::{AuditAction, AuditLogEntry, AuditResourceType};

    async fn in_memory_store() -> (SqliteStore, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("merkle_test.db");
        let store = SqliteStore::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .unwrap();
        store.apply_embedded_migrations().await.unwrap();
        (store, tmp)
    }

    fn dummy_entry(actor_id: &str, action: AuditAction, resource_id: &str) -> AuditLogEntry {
        AuditLogEntry {
            id: 0,
            actor_id: actor_id.to_string(),
            action,
            resource_type: AuditResourceType::Token,
            resource_id: resource_id.to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: chrono::Utc::now(),
            content_hash: None,
            previous_hash: None,
        }
    }

    #[tokio::test]
    async fn test_compute_and_cache_root() {
        let (store, _tmp) = in_memory_store().await;
        let repo = crate::sqlite::SqliteAuditMerkleRootRepo::new(store.pool().clone());
        let audit = store.audit_log();

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();
        let _end = window + chrono::Duration::hours(1);

        // Insert two entries within the window
        let mut e1 = dummy_entry("alice", AuditAction::TokenCreate, "t1");
        e1.created_at = window + chrono::Duration::minutes(5);
        audit.append(&e1).await.unwrap();

        let mut e2 = dummy_entry("bob", AuditAction::TokenRevoke, "t2");
        e2.created_at = window + chrono::Duration::minutes(10);
        audit.append(&e2).await.unwrap();

        let root = repo.compute_and_cache_root(window).await.unwrap();
        assert_eq!(root.entry_count, 2);
        assert!(!root.root.is_empty());
        assert_eq!(root.window_start, window);

        // Idempotent: second call returns cached value
        let cached = repo.compute_and_cache_root(window).await.unwrap();
        assert_eq!(cached.root, root.root);
        assert_eq!(cached.entry_count, root.entry_count);
        assert_eq!(cached.computed_at, root.computed_at);
    }

    #[tokio::test]
    async fn test_compute_empty_window() {
        let (store, _tmp) = in_memory_store().await;
        let repo = crate::sqlite::SqliteAuditMerkleRootRepo::new(store.pool().clone());

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();

        let root = repo.compute_and_cache_root(window).await.unwrap();
        assert_eq!(root.entry_count, 0);
        assert!(root.root.is_empty());
    }

    #[tokio::test]
    async fn test_list_roots() {
        let (store, _tmp) = in_memory_store().await;
        let repo = crate::sqlite::SqliteAuditMerkleRootRepo::new(store.pool().clone());
        let audit = store.audit_log();

        let window1 = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();
        let window0 = window1 - chrono::Duration::hours(1);

        let mut e = dummy_entry("alice", AuditAction::TokenCreate, "t1");
        e.created_at = window0 + chrono::Duration::minutes(5);
        audit.append(&e).await.unwrap();

        repo.compute_and_cache_root(window0).await.unwrap();
        repo.compute_and_cache_root(window1).await.unwrap();

        let (items, next) = repo.list_roots(None, 10).await.unwrap();
        assert_eq!(items.len(), 2);
        assert!(next.is_none());
        // Descending order: window1 first
        assert_eq!(items[0].window_start, window1);
        assert_eq!(items[1].window_start, window0);
    }

    #[tokio::test]
    async fn test_list_roots_pagination() {
        let (store, _tmp) = in_memory_store().await;
        let repo = crate::sqlite::SqliteAuditMerkleRootRepo::new(store.pool().clone());
        let audit = store.audit_log();

        let base = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();

        for i in 0..3 {
            let mut e = dummy_entry("alice", AuditAction::TokenCreate, &format!("t{}", i));
            e.created_at = base - chrono::Duration::hours(i as i64) + chrono::Duration::minutes(5);
            audit.append(&e).await.unwrap();
            repo.compute_and_cache_root(base - chrono::Duration::hours(i as i64))
                .await
                .unwrap();
        }

        let (page1, cursor1) = repo.list_roots(None, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(cursor1.is_some());

        let (page2, cursor2) = repo.list_roots(cursor1.as_deref(), 2).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert!(cursor2.is_none());
    }
}
