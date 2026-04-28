use async_trait::async_trait;
use ferrum_proto::{IntentEnvelope, IntentId, IntentStatus};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{IntentRepo, Result};

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
}
