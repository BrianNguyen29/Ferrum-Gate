//! PostgreSQL IntentRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ferrum_proto::{IntentEnvelope, IntentId, IntentStatus};
use sqlx::{PgPool, Row};

use crate::{IntentRepo, Result, StoreError};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct PostgresIntentRepo {
    pool: PgPool,
}

impl PostgresIntentRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IntentRepo for PostgresIntentRepo {
    async fn insert(&self, intent: &IntentEnvelope) -> Result<()> {
        let raw_json = to_json(intent)?;
        sqlx::query(
            "INSERT INTO intents (
                intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode,
                default_rollback_class, created_at, expires_at, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
        let raw_json = to_json(intent)?;
        sqlx::query(
            "UPDATE intents
             SET normalized_goal = $2,
                 status = $3,
                 risk_tier = $4,
                 approval_mode = $5,
                 default_rollback_class = $6,
                 expires_at = $7,
                 raw_json = $8
             WHERE intent_id = $1",
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
        sqlx::query("UPDATE intents SET status = $2 WHERE intent_id = $1")
            .bind(intent_id.to_string())
            .bind(enum_text(&status)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_by_status(&self, status: IntentStatus) -> Result<Vec<IntentEnvelope>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM intents WHERE status = $1 ORDER BY created_at DESC",
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
        let mut conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();
        let mut next_idx = 1;

        if let Some(ref id) = intent_id {
            conditions.push(format!("intent_id = ${}", next_idx));
            params.push(id.to_string());
            next_idx += 1;
        }

        if !statuses.is_empty() {
            let placeholders: Vec<String> = (0..statuses.len())
                .map(|i| format!("${}", next_idx + i))
                .collect();
            conditions.push(format!("status IN ({})", placeholders.join(", ")));
            for s in statuses {
                params.push(enum_text(s).expect("intent status should serialize"));
            }
            next_idx += statuses.len();
        }

        if let Some(c) = cursor {
            let decoded = URL_SAFE_NO_PAD
                .decode(c)
                .map_err(|_| StoreError::Other("invalid cursor encoding".to_string()))?;
            let decoded_str = String::from_utf8(decoded)
                .map_err(|_| StoreError::Other("invalid cursor string".to_string()))?;
            let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
            if parts.len() == 2 {
                let cursor_created_at = parts[0];
                let cursor_intent_id = parts[1];
                conditions.push(format!(
                    "(created_at < ${a} OR (created_at = ${b} AND intent_id < ${c}))",
                    a = next_idx,
                    b = next_idx + 1,
                    c = next_idx + 2
                ));
                params.push(cursor_created_at.to_string());
                params.push(cursor_created_at.to_string());
                params.push(cursor_intent_id.to_string());
                next_idx += 3;
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let limit_idx = next_idx;
        let sql = format!(
            "SELECT raw_json, intent_id, created_at FROM intents {} ORDER BY created_at DESC, intent_id DESC LIMIT ${}",
            where_clause, limit_idx
        );

        let mut query = sqlx::query(&sql);
        for param in &params {
            query = query.bind(param);
        }
        query = query.bind((limit + 1) as i64);

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

    async fn list_intents_with_exec_state(
        &self,
        intent_id: Option<IntentId>,
        statuses: &[IntentStatus],
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<(IntentEnvelope, Option<String>)>, Option<String>)> {
        let mut conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();
        let mut next_idx = 1;

        if let Some(ref id) = intent_id {
            conditions.push(format!("i.intent_id = ${}", next_idx));
            params.push(id.to_string());
            next_idx += 1;
        }

        if !statuses.is_empty() {
            let placeholders: Vec<String> = (0..statuses.len())
                .map(|i| format!("${}", next_idx + i))
                .collect();
            conditions.push(format!("i.status IN ({})", placeholders.join(", ")));
            for s in statuses {
                params.push(enum_text(s).expect("intent status should serialize"));
            }
            next_idx += statuses.len();
        }

        if let Some(c) = cursor {
            let decoded = URL_SAFE_NO_PAD
                .decode(c)
                .map_err(|_| StoreError::Other("invalid cursor encoding".to_string()))?;
            let decoded_str = String::from_utf8(decoded)
                .map_err(|_| StoreError::Other("invalid cursor string".to_string()))?;
            let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
            if parts.len() == 2 {
                let cursor_created_at = parts[0];
                let cursor_intent_id = parts[1];
                conditions.push(format!(
                    "(i.created_at < ${a} OR (i.created_at = ${b} AND i.intent_id < ${c}))",
                    a = next_idx,
                    b = next_idx + 1,
                    c = next_idx + 2
                ));
                params.push(cursor_created_at.to_string());
                params.push(cursor_created_at.to_string());
                params.push(cursor_intent_id.to_string());
                next_idx += 3;
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let limit_idx = next_idx;
        let sql = format!(
            r#"WITH latest_executions AS (
                SELECT
                    intent_id,
                    state,
                    started_at,
                    ROW_NUMBER() OVER (PARTITION BY intent_id ORDER BY started_at DESC) as rn
                FROM executions
            )
            SELECT
                i.raw_json,
                i.intent_id,
                i.created_at,
                le.state as exec_state
            FROM intents i
            LEFT JOIN latest_executions le ON i.intent_id = le.intent_id AND le.rn = 1
            {} ORDER BY i.created_at DESC, i.intent_id DESC LIMIT ${}"#,
            where_clause, limit_idx
        );

        let mut query = sqlx::query(&sql);
        for param in &params {
            query = query.bind(param);
        }
        query = query.bind((limit + 1) as i64);

        let rows = query.fetch_all(&self.pool).await?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<(IntentEnvelope, Option<String>)> = rows
            .iter()
            .take(limit as usize)
            .map(|row| {
                let raw: String = row.get("raw_json");
                let intent: IntentEnvelope =
                    serde_json::from_str(&raw).map_err(|e| StoreError::Other(e.to_string()))?;
                let exec_state: Option<String> = row.get("exec_state");
                Ok::<_, StoreError>((intent, exec_state))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let next_cursor = if has_more {
            items.last().map(|(intent, _)| {
                let cursor_data = format!("{}:{}", intent.created_at, intent.intent_id);
                URL_SAFE_NO_PAD.encode(cursor_data.as_bytes())
            })
        } else {
            None
        };

        Ok((items, next_cursor))
    }
}
