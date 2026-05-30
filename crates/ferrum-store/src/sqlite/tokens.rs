use async_trait::async_trait;
use ferrum_proto::{ScopedToken, TokenRole};
use sqlx::{Row, SqlitePool};

use crate::{Result, TokenRepo};

#[derive(Clone)]
pub struct SqliteTokenRepo {
    pool: SqlitePool,
}

impl SqliteTokenRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn token_to_row(token: &ScopedToken) -> Result<(String, String)> {
    let scopes = serde_json::to_string(&token.scopes)?;
    let raw_json = serde_json::to_string(token)?;
    Ok((scopes, raw_json))
}

fn row_to_token(row: &sqlx::sqlite::SqliteRow) -> Result<ScopedToken> {
    let role_str: String = row.try_get("role")?;
    let role = role_str
        .parse::<TokenRole>()
        .map_err(|e| crate::StoreError::Other(format!("invalid token role in database: {}", e)))?;
    let scopes_str: String = row.try_get("scopes")?;
    let scopes: Vec<String> = serde_json::from_str(&scopes_str)?;
    let expires_at_str: String = row.try_get("expires_at")?;
    let created_at_str: String = row.try_get("created_at")?;
    let last_used_at_str: Option<String> = row.try_get("last_used_at")?;
    let revoked_at_str: Option<String> = row.try_get("revoked_at")?;

    let expires_at = chrono::DateTime::parse_from_rfc3339(&expires_at_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid expires_at: {}", e)))?
        .with_timezone(&chrono::Utc);
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid created_at: {}", e)))?
        .with_timezone(&chrono::Utc);
    let last_used_at = last_used_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid last_used_at: {}", e)))
        })
        .transpose()?;
    let revoked_at = revoked_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid revoked_at: {}", e)))
        })
        .transpose()?;

    Ok(ScopedToken {
        token_id: row.try_get("token_id")?,
        actor_id: row.try_get("actor_id")?,
        role,
        scopes,
        description: row.try_get("description")?,
        expires_at,
        created_at,
        last_used_at,
        revoked_at,
        revoked_reason: row.try_get("revoked_reason")?,
        rotated_from: row.try_get("rotated_from")?,
        token_lookup_hash: row.try_get("token_lookup_hash")?,
        token_hash: row.try_get("token_hash")?,
        token_salt: row.try_get("token_salt")?,
    })
}

#[async_trait]
impl TokenRepo for SqliteTokenRepo {
    async fn insert(&self, token: &ScopedToken) -> Result<()> {
        let (scopes, _) = token_to_row(token)?;
        sqlx::query(
            "INSERT INTO scoped_tokens (
                token_id, actor_id, role, scopes, description,
                expires_at, created_at, last_used_at, revoked_at, revoked_reason,
                rotated_from, token_lookup_hash, token_hash, token_salt
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(&token.token_id)
        .bind(&token.actor_id)
        .bind(token.role.to_string())
        .bind(scopes)
        .bind(&token.description)
        .bind(token.expires_at.to_rfc3339())
        .bind(token.created_at.to_rfc3339())
        .bind(token.last_used_at.map(|t| t.to_rfc3339()))
        .bind(token.revoked_at.map(|t| t.to_rfc3339()))
        .bind(&token.revoked_reason)
        .bind(&token.rotated_from)
        .bind(&token.token_lookup_hash)
        .bind(&token.token_hash)
        .bind(&token.token_salt)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, token_id: &str) -> Result<Option<ScopedToken>> {
        let row = sqlx::query("SELECT * FROM scoped_tokens WHERE token_id = ?1")
            .bind(token_id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_token(&row)?)),
            None => Ok(None),
        }
    }

    async fn get_by_lookup_hash(&self, lookup_hash: &str) -> Result<Option<ScopedToken>> {
        let row = sqlx::query("SELECT * FROM scoped_tokens WHERE token_lookup_hash = ?1")
            .bind(lookup_hash)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_token(&row)?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        actor_id: Option<&str>,
        role: Option<&str>,
        active_only: bool,
        limit: u32,
        _cursor: Option<&str>,
    ) -> Result<(Vec<ScopedToken>, Option<String>)> {
        let mut sql = String::from("SELECT * FROM scoped_tokens WHERE 1=1");
        if active_only {
            sql.push_str(" AND revoked_at IS NULL AND expires_at > datetime('now')");
        }
        if actor_id.is_some() {
            sql.push_str(" AND actor_id = ?");
        }
        if role.is_some() {
            sql.push_str(" AND role = ?");
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");

        let mut query = sqlx::query(&sql);
        if let Some(actor_id) = actor_id {
            query = query.bind(actor_id);
        }
        if let Some(role) = role {
            query = query.bind(role);
        }
        query = query.bind(limit + 1);

        let rows = query.fetch_all(&self.pool).await?;
        let mut tokens = Vec::new();
        for row in &rows {
            tokens.push(row_to_token(row)?);
        }

        let next_cursor = if tokens.len() > limit as usize {
            tokens.pop();
            // Simple cursor: last token_id
            tokens.last().map(|t| t.token_id.clone())
        } else {
            None
        };

        Ok((tokens, next_cursor))
    }

    async fn revoke(&self, token_id: &str, reason: Option<&str>) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE scoped_tokens SET revoked_at = ?1, revoked_reason = ?2 WHERE token_id = ?3 AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(token_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn touch(&self, token_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE scoped_tokens SET last_used_at = ?1 WHERE token_id = ?2")
            .bind(now)
            .bind(token_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
