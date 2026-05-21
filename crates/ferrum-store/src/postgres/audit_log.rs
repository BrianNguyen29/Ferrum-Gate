use async_trait::async_trait;
use ferrum_proto::{AuditAction, AuditLogEntry, AuditResourceType};
use sqlx::{PgPool, Row};

use crate::{AuditLogRepo, Result};

#[derive(Clone)]
pub struct PostgresAuditLogRepo {
    pool: PgPool,
}

impl PostgresAuditLogRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_entry(row: &sqlx::postgres::PgRow) -> Result<AuditLogEntry> {
    let action_str: String = row.try_get("action")?;
    let action = action_str.parse::<AuditAction>().map_err(|e| {
        crate::StoreError::Other(format!("invalid audit action in database: {}", e))
    })?;
    let resource_type_str: String = row.try_get("resource_type")?;
    let resource_type = resource_type_str
        .parse::<AuditResourceType>()
        .map_err(|e| {
            crate::StoreError::Other(format!("invalid audit resource type in database: {}", e))
        })?;
    let created_at_str: String = row.try_get("created_at")?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid created_at: {}", e)))?
        .with_timezone(&chrono::Utc);
    let metadata: Option<String> = row.try_get("metadata")?;
    let metadata = metadata
        .map(|m| serde_json::from_str(&m))
        .transpose()
        .map_err(|e| crate::StoreError::Other(format!("invalid metadata: {}", e)))?;

    Ok(AuditLogEntry {
        id: row.try_get::<i64, _>("id")?,
        actor_id: row.try_get("actor_id")?,
        action,
        resource_type,
        resource_id: row.try_get("resource_id")?,
        result: row.try_get("result")?,
        metadata,
        created_at,
    })
}

#[async_trait]
impl AuditLogRepo for PostgresAuditLogRepo {
    async fn append(&self, entry: &AuditLogEntry) -> Result<()> {
        let metadata = entry
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        sqlx::query(
            "INSERT INTO audit_log (
                actor_id, action, resource_type, resource_id, result, metadata, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&entry.actor_id)
        .bind(entry.action.to_string())
        .bind(entry.resource_type.to_string())
        .bind(&entry.resource_id)
        .bind(&entry.result)
        .bind(metadata)
        .bind(entry.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list(
        &self,
        action: Option<AuditAction>,
        resource_type: Option<AuditResourceType>,
        resource_id: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<AuditLogEntry>, Option<String>)> {
        let cursor_id = cursor.and_then(|c| c.parse::<i64>().ok());

        let mut sql = String::from("SELECT * FROM audit_log WHERE 1=1");
        let mut param_idx = 1i32;

        if action.is_some() {
            sql.push_str(&format!(" AND action = ${}", param_idx));
            param_idx += 1;
        }
        if resource_type.is_some() {
            sql.push_str(&format!(" AND resource_type = ${}", param_idx));
            param_idx += 1;
        }
        if resource_id.is_some() {
            sql.push_str(&format!(" AND resource_id = ${}", param_idx));
            param_idx += 1;
        }
        if cursor_id.is_some() {
            sql.push_str(&format!(" AND id < ${}", param_idx));
            param_idx += 1;
        }
        sql.push_str(&format!(
            " ORDER BY created_at DESC, id DESC LIMIT ${}",
            param_idx
        ));

        let mut query = sqlx::query(&sql);
        if let Some(action) = action {
            query = query.bind(action.to_string());
        }
        if let Some(resource_type) = resource_type {
            query = query.bind(resource_type.to_string());
        }
        if let Some(resource_id) = resource_id {
            query = query.bind(resource_id);
        }
        if let Some(cursor_id) = cursor_id {
            query = query.bind(cursor_id);
        }
        query = query.bind((limit + 1) as i64);

        let rows = query.fetch_all(&self.pool).await?;
        let mut entries = Vec::new();
        for row in &rows {
            entries.push(row_to_entry(row)?);
        }

        let next_cursor = if entries.len() > limit as usize {
            entries.pop();
            entries.last().map(|e| e.id.to_string())
        } else {
            None
        };

        Ok((entries, next_cursor))
    }
}
