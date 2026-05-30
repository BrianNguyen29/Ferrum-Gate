use async_trait::async_trait;
use ferrum_proto::{AuditAction, AuditLogEntry, AuditResourceType};
use sha2::{Digest, Sha256};
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

/// Compute a deterministic SHA-256 content hash for an audit log entry.
///
/// The hash covers canonical fields (actor_id, action, resource_type,
/// resource_id, result, metadata, created_at) and excludes id, content_hash,
/// and previous_hash to avoid circularity.
fn compute_content_hash(entry: &AuditLogEntry) -> String {
    let canonical = serde_json::json!({
        "actor_id": entry.actor_id,
        "action": entry.action.to_string(),
        "resource_type": entry.resource_type.to_string(),
        "resource_id": entry.resource_id,
        "result": entry.result,
        "metadata": entry.metadata,
        "created_at": entry.created_at.to_rfc3339(),
    });
    let bytes = serde_json::to_vec(&canonical).expect("canonical serialization");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize())
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
        content_hash: row.try_get::<Option<String>, _>("content_hash")?,
        previous_hash: row.try_get::<Option<String>, _>("previous_hash")?,
    })
}

#[async_trait]
impl AuditLogRepo for PostgresAuditLogRepo {
    async fn append(&self, entry: &AuditLogEntry) -> Result<()> {
        let content_hash = compute_content_hash(entry);
        // Link to the most recent entry that already has a content_hash.
        let previous_hash: Option<String> =
            sqlx::query_scalar("SELECT content_hash FROM audit_log WHERE content_hash IS NOT NULL ORDER BY id DESC LIMIT 1")
                .fetch_optional(&self.pool)
                .await?;

        let metadata = entry
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        sqlx::query(
            "INSERT INTO audit_log (
                actor_id, action, resource_type, resource_id, result, metadata, created_at,
                content_hash, previous_hash
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&entry.actor_id)
        .bind(entry.action.to_string())
        .bind(entry.resource_type.to_string())
        .bind(&entry.resource_id)
        .bind(&entry.result)
        .bind(metadata)
        .bind(entry.created_at.to_rfc3339())
        .bind(&content_hash)
        .bind(previous_hash)
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
        since: Option<chrono::DateTime<chrono::Utc>>,
        until: Option<chrono::DateTime<chrono::Utc>>,
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
        if since.is_some() {
            sql.push_str(&format!(" AND created_at >= ${}", param_idx));
            param_idx += 1;
        }
        if until.is_some() {
            sql.push_str(&format!(" AND created_at <= ${}", param_idx));
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
        if let Some(since) = since {
            query = query.bind(since.to_rfc3339());
        }
        if let Some(until) = until {
            query = query.bind(until.to_rfc3339());
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

    async fn verify_chain(&self) -> Result<()> {
        let rows = sqlx::query("SELECT * FROM audit_log ORDER BY id ASC")
            .fetch_all(&self.pool)
            .await?;

        let mut prior_content_hash: Option<String> = None;

        for row in &rows {
            let entry = row_to_entry(row)?;
            let id = entry.id;
            let content_hash = entry.content_hash.clone();
            let previous_hash = entry.previous_hash.clone();

            if content_hash.is_none() {
                // Legacy entry without hash; skip chain validation but do not
                // reset prior_content_hash so that subsequent hashed entries
                // can still link to the last hashed entry before this gap.
                continue;
            }

            let stored_hash = content_hash.unwrap();
            let recomputed = compute_content_hash(&entry);
            if stored_hash != recomputed {
                return Err(crate::StoreError::InvalidState(format!(
                    "audit log entry {} has tampered content: stored content_hash '{}' != recomputed '{}'",
                    id, stored_hash, recomputed
                )));
            }

            if let Some(ref prior) = prior_content_hash {
                let prev = previous_hash.as_deref().ok_or_else(|| {
                    crate::StoreError::InvalidState(format!(
                        "audit log entry {} has content_hash but missing previous_hash",
                        id
                    ))
                })?;
                if prev != prior {
                    return Err(crate::StoreError::InvalidState(format!(
                        "audit log entry {} has broken chain: previous_hash '{}' != prior content_hash '{}'",
                        id, prev, prior
                    )));
                }
            } else {
                // First hashed entry (genesis of hash chain)
                if previous_hash.is_some() {
                    return Err(crate::StoreError::InvalidState(format!(
                        "audit log entry {} is the first hashed entry but has previous_hash",
                        id
                    )));
                }
            }

            prior_content_hash = Some(stored_hash);
        }

        Ok(())
    }
}
