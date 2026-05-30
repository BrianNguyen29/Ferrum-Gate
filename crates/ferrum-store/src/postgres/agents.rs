use async_trait::async_trait;
use ferrum_proto::AgentRecord;
use sqlx::{PgPool, Row};

use crate::{AgentRepo, Result};

#[derive(Clone)]
pub struct PostgresAgentRepo {
    pool: PgPool,
}

impl PostgresAgentRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Result<AgentRecord> {
    let scopes_str: String = row.try_get("allowed_scopes")?;
    let scopes: Vec<String> = serde_json::from_str(&scopes_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid allowed_scopes: {}", e)))?;
    let created_at_str: String = row.try_get("created_at")?;
    let revoked_at_str: Option<String> = row.try_get("revoked_at")?;

    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid created_at: {}", e)))?
        .with_timezone(&chrono::Utc);
    let revoked_at = revoked_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid revoked_at: {}", e)))
        })
        .transpose()?;

    Ok(AgentRecord {
        agent_id: row.try_get("agent_id")?,
        public_key: row.try_get("public_key")?,
        key_fingerprint: row.try_get("key_fingerprint")?,
        allowed_scopes: scopes,
        created_at,
        revoked_at,
        description: row.try_get("description")?,
    })
}

#[async_trait]
impl AgentRepo for PostgresAgentRepo {
    async fn insert(&self, agent: &AgentRecord) -> Result<()> {
        let scopes = serde_json::to_string(&agent.allowed_scopes)?;
        sqlx::query(
            "INSERT INTO agent_registry (
                agent_id, public_key, key_fingerprint, allowed_scopes,
                created_at, revoked_at, description
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&agent.agent_id)
        .bind(&agent.public_key)
        .bind(&agent.key_fingerprint)
        .bind(scopes)
        .bind(agent.created_at.to_rfc3339())
        .bind(agent.revoked_at.map(|t| t.to_rfc3339()))
        .bind(&agent.description)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, agent_id: &str) -> Result<Option<AgentRecord>> {
        let row = sqlx::query("SELECT * FROM agent_registry WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_agent(&row)?)),
            None => Ok(None),
        }
    }

    async fn get_by_fingerprint(&self, fingerprint: &str) -> Result<Option<AgentRecord>> {
        let row = sqlx::query("SELECT * FROM agent_registry WHERE key_fingerprint = $1")
            .bind(fingerprint)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_agent(&row)?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        active_only: bool,
        limit: u32,
        _cursor: Option<&str>,
    ) -> Result<(Vec<AgentRecord>, Option<String>)> {
        let mut sql = String::from("SELECT * FROM agent_registry WHERE 1=1");
        if active_only {
            sql.push_str(" AND revoked_at IS NULL");
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT $1");

        let rows = sqlx::query(&sql)
            .bind((limit + 1) as i64)
            .fetch_all(&self.pool)
            .await?;

        let mut agents = Vec::new();
        for row in &rows {
            agents.push(row_to_agent(row)?);
        }

        let next_cursor = if agents.len() > limit as usize {
            agents.pop();
            agents.last().map(|a| a.agent_id.clone())
        } else {
            None
        };

        Ok((agents, next_cursor))
    }

    async fn count(&self, active_only: bool) -> Result<usize> {
        let mut sql = String::from("SELECT COUNT(*) FROM agent_registry WHERE 1=1");
        if active_only {
            sql.push_str(" AND revoked_at IS NULL");
        }
        let row = sqlx::query(&sql).fetch_one(&self.pool).await?;
        let count: i64 = row.try_get(0)?;
        Ok(count as usize)
    }

    async fn revoke(&self, agent_id: &str) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE agent_registry SET revoked_at = $1 WHERE agent_id = $2 AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
