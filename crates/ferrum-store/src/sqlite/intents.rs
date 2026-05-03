use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ferrum_proto::{IntentEnvelope, IntentId, IntentStatus};
use sqlx::{Row, SqlitePool};
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{IntentRepo, Result, StoreError};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteIntentRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteIntentRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            write_queue: None,
        }
    }

    pub fn with_write_queue(mut self, queue: WriteQueue) -> Self {
        self.write_queue = Some(queue);
        self
    }
}

#[async_trait]
impl IntentRepo for SqliteIntentRepo {
    async fn insert(&self, intent: &IntentEnvelope) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertIntent {
                data: intent.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(intent)?;
        sqlx::query(
            "INSERT INTO intents (
                intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode,
                default_rollback_class, created_at, expires_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(intent.intent_id.to_string())
        .bind(intent.principal_id.to_string())
        .bind(&intent.normalized_goal)
        .bind(enum_text(&intent.status)?)
        .bind(enum_text(&intent.risk_tier)?)
        .bind(enum_text(&intent.approval_mode)?)
        .bind(enum_text(&intent.default_rollback_class)?)
        .bind(intent.created_at)
        .bind(intent.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, intent_id: IntentId) -> Result<Option<IntentEnvelope>> {
        fetch_entity_by_id(&self.pool, "intents", "intent_id", &intent_id.to_string()).await
    }

    async fn update(&self, intent: &IntentEnvelope) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateIntent {
                data: intent.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(intent)?;
        sqlx::query(
            "UPDATE intents
             SET normalized_goal = ?2,
                 status = ?3,
                 risk_tier = ?4,
                 approval_mode = ?5,
                 default_rollback_class = ?6,
                 expires_at = ?7,
                 raw_json = ?8
             WHERE intent_id = ?1",
        )
        .bind(intent.intent_id.to_string())
        .bind(&intent.normalized_goal)
        .bind(enum_text(&intent.status)?)
        .bind(enum_text(&intent.risk_tier)?)
        .bind(enum_text(&intent.approval_mode)?)
        .bind(enum_text(&intent.default_rollback_class)?)
        .bind(intent.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_status(&self, intent_id: IntentId, status: IntentStatus) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateIntentStatus {
                intent_id,
                status,
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        // Direct SQL UPDATE - avoids read-modify-write overhead
        sqlx::query("UPDATE intents SET status = ?2 WHERE intent_id = ?1")
            .bind(intent_id.to_string())
            .bind(enum_text(&status)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_by_status(&self, status: IntentStatus) -> Result<Vec<IntentEnvelope>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM intents WHERE status = ?1 ORDER BY created_at DESC",
            |query| query.bind(enum_text(&status).expect("intent status should serialize")),
        )
        .await
    }

    async fn list_intents(
        &self,
        intent_id: Option<IntentId>,
        statuses: &[IntentStatus],
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<IntentEnvelope>, Option<String>)> {
        // Build dynamic WHERE clause
        let mut conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(ref id) = intent_id {
            conditions.push("intent_id = ?".to_string());
            params.push(id.to_string());
        }

        if !statuses.is_empty() {
            let placeholders: Vec<String> = statuses.iter().map(|_| "?".to_string()).collect();
            conditions.push(format!("status IN ({})", placeholders.join(", ")));
            for s in statuses {
                params.push(enum_text(s).expect("intent status should serialize"));
            }
        }

        // Cursor-based pagination: cursor encodes (created_at, intent_id)
        // Items are ordered by (created_at DESC, intent_id DESC)
        if let Some(c) = cursor {
            let decoded = URL_SAFE_NO_PAD
                .decode(c)
                .map_err(|_| StoreError::Other("invalid cursor encoding".to_string()))?;
            let decoded_str = String::from_utf8(decoded)
                .map_err(|_| StoreError::Other("invalid cursor string".to_string()))?;
            let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
            if parts.len() == 2 {
                // Cursor points to items older than this timestamp and intent_id
                let cursor_created_at = parts[0];
                let cursor_intent_id = parts[1];
                conditions
                    .push("(created_at < ? OR (created_at = ? AND intent_id < ?))".to_string());
                params.push(cursor_created_at.to_string());
                params.push(cursor_created_at.to_string());
                params.push(cursor_intent_id.to_string());
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Build SQL with ORDER BY and LIMIT (fetch limit+1 to check if there are more)
        let sql = format!(
            "SELECT raw_json, intent_id, created_at FROM intents {} ORDER BY created_at DESC, intent_id DESC LIMIT ?",
            where_clause
        );

        // Bind params dynamically
        let mut query = sqlx::query(&sql);
        for param in &params {
            query = query.bind(param);
        }
        query = query.bind(limit + 1); // Fetch one extra to check if there are more

        let rows = query.fetch_all(&self.pool).await?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<IntentEnvelope> = rows
            .iter()
            .take(limit as usize)
            .map(|row| {
                let raw: String = row.get("raw_json");
                serde_json::from_str(&raw).map_err(|e| StoreError::Other(e.to_string()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let next_cursor = if has_more {
            items.last().map(|intent| {
                let cursor_data = format!("{}:{}", intent.created_at, intent.intent_id);
                URL_SAFE_NO_PAD.encode(cursor_data.as_bytes())
            })
        } else {
            None
        };

        Ok((items, next_cursor))
    }
}
