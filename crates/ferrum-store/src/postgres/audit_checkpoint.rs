use async_trait::async_trait;
use ferrum_proto::AuditCheckpoint;
use sqlx::{PgPool, Row};

use crate::{AuditCheckpointRepo, Result};

#[derive(Clone)]
pub struct PostgresAuditCheckpointRepo {
    pool: PgPool,
}

impl PostgresAuditCheckpointRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditCheckpointRepo for PostgresAuditCheckpointRepo {
    async fn insert(&self, checkpoint: &AuditCheckpoint) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_checkpoints (window_start, merkle_root, entry_count, signer_id, signer_key_fingerprint, signed_at, signature, public_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
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
            "SELECT window_start, merkle_root, entry_count, signer_id, signer_key_fingerprint, signed_at, signature, public_key FROM audit_checkpoints WHERE window_start = $1"
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

fn parse_row(r: &sqlx::postgres::PgRow) -> AuditCheckpoint {
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
