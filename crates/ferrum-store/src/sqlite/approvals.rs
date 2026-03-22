use async_trait::async_trait;
use ferrum_proto::{ApprovalId, ApprovalRequest, ApprovalState};
use sqlx::SqlitePool;

use crate::{ApprovalRepo, Result};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteApprovalRepo {
    pool: SqlitePool,
}

impl SqliteApprovalRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApprovalRepo for SqliteApprovalRepo {
    async fn insert(&self, approval: &ApprovalRequest) -> Result<()> {
        let raw_json = to_json(approval)?;
        sqlx::query(
            "INSERT INTO approvals (
                approval_id, intent_id, proposal_id, execution_id, action_digest,
                state, expires_at, created_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.intent_id.to_string())
        .bind(approval.proposal_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at)
        .bind(approval.created_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, approval_id: ApprovalId) -> Result<Option<ApprovalRequest>> {
        fetch_entity_by_id(
            &self.pool,
            "approvals",
            "approval_id",
            &approval_id.to_string(),
        )
        .await
    }

    async fn update(&self, approval: &ApprovalRequest) -> Result<()> {
        let raw_json = to_json(approval)?;
        sqlx::query(
            "UPDATE approvals
             SET execution_id = ?2,
                 action_digest = ?3,
                 state = ?4,
                 expires_at = ?5,
                 raw_json = ?6
             WHERE approval_id = ?1",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resolve(&self, approval_id: ApprovalId, state: ApprovalState) -> Result<()> {
        let Some(mut approval) = self.get(approval_id).await? else {
            return Ok(());
        };
        approval.state = state;
        self.update(&approval).await
    }

    async fn list_pending(&self) -> Result<Vec<ApprovalRequest>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM approvals WHERE state = ?1 ORDER BY created_at DESC",
            enum_text(&ApprovalState::Pending)?,
        )
        .await
    }
}
