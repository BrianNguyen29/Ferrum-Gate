//! PostgreSQL ApprovalRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{ApprovalId, ApprovalRequest, ApprovalState, ProposalId, Timestamp};
use sqlx::{PgPool, Row};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, from_json, to_json};
use crate::{ApprovalRepo, Result, transitions};

#[derive(Clone)]
pub struct PostgresApprovalRepo {
    pool: PgPool,
}

impl PostgresApprovalRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApprovalRepo for PostgresApprovalRepo {
    async fn insert(&self, approval: &ApprovalRequest) -> Result<()> {
        let raw_json = to_json(approval)?;
        sqlx::query(
            "INSERT INTO approvals (
                approval_id, intent_id, proposal_id, execution_id, action_digest,
                state, expires_at, created_at, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.intent_id.to_string())
        .bind(approval.proposal_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at.to_rfc3339())
        .bind(approval.created_at.to_rfc3339())
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
             SET execution_id = $2,
                 action_digest = $3,
                 state = $4,
                 expires_at = $5,
                 raw_json = $6
             WHERE approval_id = $1",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at.to_rfc3339())
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resolve(&self, approval_id: ApprovalId, state: ApprovalState) -> Result<()> {
        let Some(mut approval) = self.get(approval_id).await? else {
            return Ok(());
        };
        if !transitions::is_valid_approval_transition(&approval.state, &state) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid approval transition from {:?} to {:?}",
                approval.state, state
            )));
        }
        approval.state = state;
        self.update(&approval).await
    }

    async fn list_pending(&self) -> Result<Vec<ApprovalRequest>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM approvals WHERE state = $1 ORDER BY created_at DESC",
            |query| query.bind("Pending"),
        )
        .await
    }

    async fn list_pending_paginated(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals WHERE state = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect()
    }

    async fn list_pending_by_proposal_paginated(
        &self,
        proposal_id: ProposalId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals WHERE state = $1 AND proposal_id = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(proposal_id.to_string())
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect()
    }

    async fn list_pending_cursor(
        &self,
        created_after: Timestamp,
        approval_id_after: ApprovalId,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals
            WHERE state = $1
              AND (created_at, approval_id) < ($2, $3)
            ORDER BY created_at DESC, approval_id DESC
            LIMIT $4";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(created_after.to_rfc3339())
            .bind(approval_id_after.to_string())
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect()
    }

    async fn list_pending_by_proposal_cursor(
        &self,
        proposal_id: ProposalId,
        created_after: Timestamp,
        approval_id_after: ApprovalId,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals
            WHERE state = $1 AND proposal_id = $2
              AND (created_at, approval_id) < ($3, $4)
            ORDER BY created_at DESC, approval_id DESC
            LIMIT $5";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(proposal_id.to_string())
            .bind(created_after.to_rfc3339())
            .bind(approval_id_after.to_string())
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect()
    }
}
