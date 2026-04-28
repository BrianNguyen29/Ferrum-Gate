use async_trait::async_trait;
use ferrum_proto::{CapabilityId, CapabilityLease, CapabilityStatus, IntentId};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{CapabilityRepo, Result, transitions};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteCapabilityRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteCapabilityRepo {
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
impl CapabilityRepo for SqliteCapabilityRepo {
    async fn insert(&self, capability: &CapabilityLease) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertCapability {
                data: capability.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(capability)?;
        sqlx::query(
            "INSERT INTO capabilities (
                capability_id, intent_id, proposal_id, server_name, tool_name, status,
                issued_at, expires_at, revoked_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateCapability {
                data: capability.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(capability)?;
        sqlx::query(
            "UPDATE capabilities
             SET status = ?2,
                 issued_at = ?3,
                 expires_at = ?4,
                 revoked_at = ?5,
                 raw_json = ?6
             WHERE capability_id = ?1",
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
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateCapabilityStatus {
                capability_id,
                status,
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let Some(mut capability) = self.get(capability_id).await? else {
            return Ok(());
        };
        // Validate state transition
        if !transitions::is_valid_capability_transition(&capability.status, &status) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid capability transition from {:?} to {:?}",
                capability.status, status
            )));
        }
        capability.status = status;
        self.update(&capability).await
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<CapabilityLease>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM capabilities WHERE intent_id = ?1 ORDER BY issued_at DESC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }
}
