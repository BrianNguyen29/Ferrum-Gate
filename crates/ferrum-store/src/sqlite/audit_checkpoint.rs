use async_trait::async_trait;
use ferrum_proto::AuditCheckpoint;
use sqlx::{Row, SqlitePool};

use crate::{AuditCheckpointRepo, Result};

#[derive(Clone)]
pub struct SqliteAuditCheckpointRepo {
    pool: SqlitePool,
}

impl SqliteAuditCheckpointRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditCheckpointRepo for SqliteAuditCheckpointRepo {
    async fn insert(&self, checkpoint: &AuditCheckpoint) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_checkpoints (window_start, merkle_root, entry_count, signer_id, signer_key_fingerprint, signed_at, signature, public_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        )
        .bind(checkpoint.window_start.to_rfc3339())
        .bind(&checkpoint.merkle_root)
        .bind(checkpoint.entry_count)
        .bind(&checkpoint.signer_id)
        .bind(&checkpoint.signer_key_fingerprint)
        .bind(checkpoint.signed_at.to_rfc3339())
        .bind(&checkpoint.signature)
        .bind(&checkpoint.public_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<AuditCheckpoint>> {
        let row = sqlx::query(
            "SELECT window_start, merkle_root, entry_count, signer_id, signer_key_fingerprint, signed_at, signature, public_key FROM audit_checkpoints WHERE window_start = ?"
        )
        .bind(window_start.to_rfc3339())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| parse_row(&r)))
    }

    async fn list(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<AuditCheckpoint>, Option<String>)> {
        let mut sql = String::from(
            "SELECT window_start, merkle_root, entry_count, signer_id, signer_key_fingerprint, signed_at, signature, public_key FROM audit_checkpoints WHERE 1=1",
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
            items.push(parse_row(row));
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

fn parse_row(r: &sqlx::sqlite::SqliteRow) -> AuditCheckpoint {
    let ws: String = r.get("window_start");
    let sa: String = r.get("signed_at");
    AuditCheckpoint {
        window_start: chrono::DateTime::parse_from_rfc3339(&ws)
            .unwrap()
            .with_timezone(&chrono::Utc),
        merkle_root: r.get("merkle_root"),
        entry_count: r.get::<i64, _>("entry_count"),
        signer_id: r.get("signer_id"),
        signer_key_fingerprint: r.get("signer_key_fingerprint"),
        signed_at: chrono::DateTime::parse_from_rfc3339(&sa)
            .unwrap()
            .with_timezone(&chrono::Utc),
        signature: r.get("signature"),
        public_key: r.get("public_key"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::SqliteStore;

    async fn in_memory_store() -> (SqliteStore, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("checkpoint_test.db");
        let store = SqliteStore::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .unwrap();
        store.apply_embedded_migrations().await.unwrap();
        (store, tmp)
    }

    fn dummy_checkpoint(window_start: chrono::DateTime<chrono::Utc>) -> AuditCheckpoint {
        AuditCheckpoint {
            window_start,
            merkle_root: "abc123".to_string(),
            entry_count: 42,
            signer_id: "operator-1".to_string(),
            signer_key_fingerprint: "fp123".to_string(),
            signed_at: chrono::Utc::now(),
            signature: "sig123".to_string(),
            public_key: "pk123".to_string(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let (store, _tmp) = in_memory_store().await;
        let repo = SqliteAuditCheckpointRepo::new(store.pool().clone());

        let window = chrono::Utc::now();
        let cp = dummy_checkpoint(window);
        repo.insert(&cp).await.unwrap();

        let got = repo.get(window).await.unwrap().expect("should exist");
        assert_eq!(got.merkle_root, cp.merkle_root);
        assert_eq!(got.entry_count, cp.entry_count);
        assert_eq!(got.signer_id, cp.signer_id);
        assert_eq!(got.signature, cp.signature);
    }

    #[tokio::test]
    async fn test_list_and_pagination() {
        let (store, _tmp) = in_memory_store().await;
        let repo = SqliteAuditCheckpointRepo::new(store.pool().clone());

        let base = chrono::Utc::now();
        for i in 0..3 {
            let mut cp = dummy_checkpoint(base - chrono::Duration::hours(i as i64));
            cp.merkle_root = format!("root{}", i);
            repo.insert(&cp).await.unwrap();
        }

        let (page1, cursor1) = repo.list(None, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(cursor1.is_some());

        let (page2, cursor2) = repo.list(cursor1.as_deref(), 2).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert!(cursor2.is_none());
    }
}
