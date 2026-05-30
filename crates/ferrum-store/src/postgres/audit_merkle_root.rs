use async_trait::async_trait;
use ferrum_proto::AuditMerkleRoot;
use sqlx::{PgPool, Row};

use crate::{AuditMerkleRootRepo, Result};

#[derive(Clone)]
pub struct PostgresAuditMerkleRootRepo {
    pool: PgPool,
}

impl PostgresAuditMerkleRootRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditMerkleRootRepo for PostgresAuditMerkleRootRepo {
    async fn compute_and_cache_root(
        &self,
        window_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<AuditMerkleRoot> {
        let window_end = window_start + chrono::Duration::hours(1);

        if let Some(cached) = self.get_root(window_start).await? {
            return Ok(cached);
        }

        let rows = sqlx::query_scalar::<_, String>(
            "SELECT content_hash FROM audit_log WHERE content_hash IS NOT NULL AND created_at >= $1 AND created_at < $2 ORDER BY id ASC"
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
            "INSERT INTO audit_merkle_roots (window_start, root, entry_count, computed_at) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING"
        )
        .bind(window_start.to_rfc3339())
        .bind(&root)
        .bind(entry_count)
        .bind(computed_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

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
            "SELECT window_start, root, entry_count, computed_at FROM audit_merkle_roots WHERE window_start = $1"
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
        let mut param_idx = 1i32;
        if cursor.is_some() {
            sql.push_str(&format!(" AND window_start < ${}", param_idx));
            param_idx += 1;
        }
        sql.push_str(&format!(" ORDER BY window_start DESC LIMIT ${}", param_idx));

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
