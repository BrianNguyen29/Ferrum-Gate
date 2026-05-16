//! PostgreSQL CapabilityRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{CapabilityId, CapabilityLease, CapabilityStatus, IntentId};
use sqlx::PgPool;

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};
use crate::{CapabilityRepo, Result, transitions};

#[derive(Clone)]
pub struct PostgresCapabilityRepo {
    pool: PgPool,
}

impl PostgresCapabilityRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CapabilityRepo for PostgresCapabilityRepo {
    async fn insert(&self, capability: &CapabilityLease) -> Result<()> {
        let raw_json = to_json(capability)?;
        sqlx::query(
            "INSERT INTO capabilities (
                capability_id, intent_id, proposal_id, server_name, tool_name, status,
                issued_at, expires_at, revoked_at, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(capability.capability_id.to_string())
        .bind(capability.intent_id.to_string())
        .bind(capability.proposal_id.to_string())
        .bind(&capability.tool_binding.server_name)
        .bind(&capability.tool_binding.tool_name)
        .bind(enum_text(&capability.status)?)
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .bind(capability.revoked_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, capability_id: CapabilityId) -> Result<Option<CapabilityLease>> {
        fetch_entity_by_id(
            &self.pool,
            "capabilities",
            "capability_id",
            &capability_id.to_string(),
        )
        .await
    }

    async fn update(&self, capability: &CapabilityLease) -> Result<()> {
        let raw_json = to_json(capability)?;
        sqlx::query(
            "UPDATE capabilities
             SET status = $2,
                 issued_at = $3,
                 expires_at = $4,
                 revoked_at = $5,
                 raw_json = $6
             WHERE capability_id = $1",
        )
        .bind(capability.capability_id.to_string())
        .bind(enum_text(&capability.status)?)
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .bind(capability.revoked_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_status(
        &self,
        capability_id: CapabilityId,
        status: CapabilityStatus,
    ) -> Result<()> {
        let Some(mut capability) = self.get(capability_id).await? else {
            return Ok(());
        };
        if !transitions::is_valid_capability_transition(&capability.status, &status) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid capability transition from {:?} to {:?}",
                capability.status, status
            )));
        }
        capability.status = status;
        self.update(&capability).await
    }

    async fn update_status_if_active(
        &self,
        capability_id: CapabilityId,
        status: CapabilityStatus,
    ) -> Result<bool> {
        let status_text = enum_text(&status)?;
        let active_text = enum_text(&CapabilityStatus::Active)?;
        let result = sqlx::query(
            "UPDATE capabilities
             SET status = $2,
                 raw_json = jsonb_set(raw_json, '{status}', to_jsonb($2::text))
             WHERE capability_id = $1 AND status = $3",
        )
        .bind(capability_id.to_string())
        .bind(status_text)
        .bind(active_text)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<CapabilityLease>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM capabilities WHERE intent_id = $1 ORDER BY issued_at DESC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }
}
