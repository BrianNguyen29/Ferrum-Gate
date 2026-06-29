use async_trait::async_trait;
use ferrum_proto::{MfaCredentialRecord, MfaFactorStatus, MfaFactorType};
use sqlx::{PgPool, Row};

use crate::{MfaCredentialRepo, Result};

#[derive(Clone)]
pub struct PostgresMfaCredentialRepo {
    pool: PgPool,
}

impl PostgresMfaCredentialRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<MfaCredentialRecord> {
    let factor_type_str: String = row.try_get("factor_type")?;
    let factor_type = factor_type_str
        .parse::<MfaFactorType>()
        .map_err(|e| crate::StoreError::Other(format!("invalid factor_type in database: {}", e)))?;

    let status_str: String = row.try_get("status")?;
    let status = status_str
        .parse::<MfaFactorStatus>()
        .map_err(|e| crate::StoreError::Other(format!("invalid status in database: {}", e)))?;

    let created_at_str: String = row.try_get("created_at")?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid created_at: {}", e)))?
        .with_timezone(&chrono::Utc);

    let verified_at_str: Option<String> = row.try_get("verified_at")?;
    let verified_at = verified_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid verified_at: {}", e)))
        })
        .transpose()?;

    let last_used_at_str: Option<String> = row.try_get("last_used_at")?;
    let last_used_at = last_used_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid last_used_at: {}", e)))
        })
        .transpose()?;

    let last_used_counter: Option<i64> = row.try_get("last_used_counter")?;

    let revoked_at_str: Option<String> = row.try_get("revoked_at")?;
    let revoked_at = revoked_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid revoked_at: {}", e)))
        })
        .transpose()?;

    let failed_attempts: i64 = row.try_get("failed_attempts")?;
    let locked_until_str: Option<String> = row.try_get("locked_until")?;
    let locked_until = locked_until_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid locked_until: {}", e)))
        })
        .transpose()?;

    let last_failed_at_str: Option<String> = row.try_get("last_failed_at")?;
    let last_failed_at = last_failed_at_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| crate::StoreError::Other(format!("invalid last_failed_at: {}", e)))
        })
        .transpose()?;

    let lockout_count: i64 = row.try_get("lockout_count")?;

    let raw_json_str: String = row.try_get("raw_json")?;
    let raw_json: serde_json::Value = serde_json::from_str(&raw_json_str)
        .map_err(|e| crate::StoreError::Other(format!("invalid raw_json: {}", e)))?;

    Ok(MfaCredentialRecord {
        mfa_factor_id: ferrum_proto::MfaFactorId(
            uuid::Uuid::parse_str(row.try_get::<String, _>("mfa_factor_id")?.as_str())
                .map_err(|e| crate::StoreError::Other(format!("invalid mfa_factor_id: {}", e)))?,
        ),
        agent_id: row.try_get("agent_id")?,
        factor_type,
        status,
        encrypted_secret: row.try_get("encrypted_secret")?,
        secret_nonce: row.try_get("secret_nonce")?,
        encryption_key_id: row.try_get("encryption_key_id")?,
        label: row.try_get("label")?,
        created_at,
        verified_at,
        last_used_at,
        last_used_counter: last_used_counter.map(|c| c as u64),
        revoked_at,
        failed_attempts: failed_attempts as u32,
        locked_until,
        last_failed_at,
        lockout_count: lockout_count as u32,
        raw_json,
    })
}

#[async_trait]
impl MfaCredentialRepo for PostgresMfaCredentialRepo {
    async fn insert(&self, record: &MfaCredentialRecord) -> Result<()> {
        let raw_json = serde_json::to_string(&record.raw_json)?;
        sqlx::query(
            "INSERT INTO mfa_credentials (
                mfa_factor_id, agent_id, factor_type, status,
                encrypted_secret, secret_nonce, encryption_key_id, label,
                created_at, verified_at, last_used_at, last_used_counter, revoked_at,
                failed_attempts, locked_until, last_failed_at, lockout_count, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)",
        )
        .bind(record.mfa_factor_id.to_string())
        .bind(&record.agent_id)
        .bind(record.factor_type.to_string())
        .bind(record.status.to_string())
        .bind(&record.encrypted_secret)
        .bind(&record.secret_nonce)
        .bind(&record.encryption_key_id)
        .bind(&record.label)
        .bind(record.created_at.to_rfc3339())
        .bind(record.verified_at.map(|t| t.to_rfc3339()))
        .bind(record.last_used_at.map(|t| t.to_rfc3339()))
        .bind(record.last_used_counter.map(|c| c as i64))
        .bind(record.revoked_at.map(|t| t.to_rfc3339()))
        .bind(record.failed_attempts as i64)
        .bind(record.locked_until.map(|t| t.to_rfc3339()))
        .bind(record.last_failed_at.map(|t| t.to_rfc3339()))
        .bind(record.lockout_count as i64)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(
        &self,
        mfa_factor_id: ferrum_proto::MfaFactorId,
    ) -> Result<Option<MfaCredentialRecord>> {
        let row = sqlx::query("SELECT * FROM mfa_credentials WHERE mfa_factor_id = $1")
            .bind(mfa_factor_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_record(&row)?)),
            None => Ok(None),
        }
    }

    async fn get_active_for_agent(&self, agent_id: &str) -> Result<Option<MfaCredentialRecord>> {
        let row = sqlx::query(
            "SELECT * FROM mfa_credentials
             WHERE agent_id = $1 AND status = 'Active' AND revoked_at IS NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(Some(row_to_record(&row)?)),
            None => Ok(None),
        }
    }

    async fn list_by_agent(&self, agent_id: &str) -> Result<Vec<MfaCredentialRecord>> {
        let rows = sqlx::query(
            "SELECT * FROM mfa_credentials WHERE agent_id = $1 ORDER BY created_at DESC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        let mut records = Vec::new();
        for row in &rows {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }

    async fn activate(&self, mfa_factor_id: ferrum_proto::MfaFactorId) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let now = chrono::Utc::now().to_rfc3339();

        // First, revoke any other active factors for the same agent to maintain
        // the invariant of at most one active factor per agent
        let _ = sqlx::query(
            "UPDATE mfa_credentials
             SET status = 'Inactive', revoked_at = $1
             WHERE agent_id = (SELECT agent_id FROM mfa_credentials WHERE mfa_factor_id = $2)
             AND mfa_factor_id != $2
             AND status = 'Active'
             AND revoked_at IS NULL",
        )
        .bind(&now)
        .bind(mfa_factor_id.to_string())
        .execute(&mut *tx)
        .await?;

        let result = sqlx::query(
            "UPDATE mfa_credentials
             SET status = 'Active', verified_at = $1
             WHERE mfa_factor_id = $2 AND status = 'Pending' AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(mfa_factor_id.to_string())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    async fn record_use(
        &self,
        mfa_factor_id: ferrum_proto::MfaFactorId,
        counter: u64,
    ) -> Result<bool> {
        if counter > i64::MAX as u64 {
            return Err(crate::StoreError::InvalidState(format!(
                "counter {} exceeds i64::MAX",
                counter
            )));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE mfa_credentials SET last_used_at = $1, last_used_counter = $2 WHERE mfa_factor_id = $3 AND (last_used_counter IS NULL OR last_used_counter < $2)",
        )
        .bind(now)
        .bind(counter as i64)
        .bind(mfa_factor_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn record_failed_attempt(
        &self,
        mfa_factor_id: ferrum_proto::MfaFactorId,
        max_attempts: u32,
        lockout_duration_secs: u64,
    ) -> Result<bool> {
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();
        let locked_until = now + chrono::Duration::seconds(lockout_duration_secs as i64);
        let locked_until_str = locked_until.to_rfc3339();

        let result = sqlx::query(
            "UPDATE mfa_credentials
             SET failed_attempts = failed_attempts + 1,
                 last_failed_at = $1,
                 locked_until = CASE WHEN (failed_attempts + 1) >= $2 THEN $3 ELSE locked_until END,
                 lockout_count = CASE WHEN (failed_attempts + 1) >= $2 THEN lockout_count + 1 ELSE lockout_count END
             WHERE mfa_factor_id = $4",
        )
        .bind(now_str)
        .bind(max_attempts as i64)
        .bind(locked_until_str)
        .bind(mfa_factor_id.to_string())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        let record = self.get(mfa_factor_id).await?;
        let locked = record
            .map(|r| r.locked_until.map(|lu| lu > now).unwrap_or(false))
            .unwrap_or(false);
        Ok(locked)
    }

    async fn reset_lockout(&self, mfa_factor_id: ferrum_proto::MfaFactorId) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE mfa_credentials
             SET failed_attempts = 0,
                 locked_until = NULL,
                 last_failed_at = NULL
             WHERE mfa_factor_id = $1",
        )
        .bind(mfa_factor_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn revoke(&self, mfa_factor_id: ferrum_proto::MfaFactorId) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE mfa_credentials
             SET status = 'Inactive', revoked_at = $1
             WHERE mfa_factor_id = $2 AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(mfa_factor_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
