//! PostgreSQL ApprovalRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{ApprovalId, ApprovalRequest, ApprovalState, ProposalId, Timestamp};
use sqlx::{PgPool, Row};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, from_json, to_json};
use crate::{ApprovalRepo, Result};

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

    async fn resolve(
        &self,
        approval_id: ApprovalId,
        state: ApprovalState,
        now: Timestamp,
    ) -> Result<bool> {
        let Some(mut approval) = self.get(approval_id).await? else {
            return Ok(false);
        };
        // Validate the requested resolve target: resolve only accepts a terminal
        // decision (Granted/Denied). Any other target is a malformed request and
        // is rejected regardless of the current row state.
        if !matches!(state, ApprovalState::Granted | ApprovalState::Denied) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid approval resolve target {:?}: must be Granted or Denied",
                state
            )));
        }
        // A terminal/expired snapshot means this resolver lost the race. Return
        // Ok(false) so the caller maps it to a conflict rather than a server
        // error. The conditional UPDATE below remains the atomic guard for
        // concurrent changes after this snapshot.
        if !matches!(approval.state, ApprovalState::Pending) {
            return Ok(false);
        }
        approval.state = state;
        // Stamp the resolution-evidence version in the SAME atomic CAS write as
        // the state transition. This marks the approval as resolved by the
        // hardened resolver so I6 role-bound binding requires authenticated
        // resolver evidence; the marker survives a later provenance-append
        // failure because it commits with the grant itself.
        approval.resolver_evidence_version = Some(ferrum_proto::CURRENT_RESOLVER_EVIDENCE_VERSION);
        let raw_json = to_json(&approval)?;

        let result = sqlx::query(
            "UPDATE approvals
             SET state = $2,
                 raw_json = $3
             WHERE approval_id = $1
               AND state = $4
               AND expires_at > $5",
        )
        .bind(approval_id.to_string())
        .bind(enum_text(&approval.state)?)
        .bind(&raw_json)
        .bind("Pending")
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
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

    async fn expire_stale_pending(
        &self,
        now: Timestamp,
        max_age_seconds: u64,
        batch_size: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let age_cutoff = now - chrono::Duration::seconds(max_age_seconds as i64);
        // Lock candidate rows to prevent concurrent reconcilers from racing; skip
        // rows that are already locked by another worker.
        let sql = "SELECT approval_id, raw_json FROM approvals
            WHERE state = $1
              AND (expires_at < $2 OR created_at < $3)
            LIMIT $4
            FOR UPDATE SKIP LOCKED";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(now.to_rfc3339())
            .bind(age_cutoff.to_rfc3339())
            .bind(batch_size as i64)
            .fetch_all(&self.pool)
            .await?;

        let mut expired = Vec::with_capacity(rows.len());
        for row in rows {
            let approval_id_str: String = row.try_get("approval_id")?;
            let raw_json: String = row.try_get("raw_json")?;
            let mut approval: ApprovalRequest = from_json(&raw_json)?;
            approval.state = ApprovalState::Expired;
            let new_raw_json = to_json(&approval)?;

            let result = sqlx::query(
                "UPDATE approvals
                 SET state = $2,
                     raw_json = $3
                 WHERE approval_id = $1
                   AND state = $4",
            )
            .bind(&approval_id_str)
            .bind(enum_text(&ApprovalState::Expired)?)
            .bind(&new_raw_json)
            .bind("Pending")
            .execute(&self.pool)
            .await?;

            if result.rows_affected() > 0 {
                expired.push(approval);
            }
        }
        Ok(expired)
    }
}
