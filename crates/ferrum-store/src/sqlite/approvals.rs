use async_trait::async_trait;
use ferrum_proto::{ApprovalId, ApprovalRequest, ApprovalState, Timestamp};
use sqlx::{Row, SqlitePool};
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{ApprovalRepo, Result};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, from_json, to_json};

#[derive(Clone)]
pub struct SqliteApprovalRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteApprovalRepo {
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
impl ApprovalRepo for SqliteApprovalRepo {
    async fn insert(&self, approval: &ApprovalRequest) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertApproval {
                data: approval.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(approval)?;
        sqlx::query(
            "INSERT INTO approvals (
                approval_id, intent_id, proposal_id, execution_id, action_digest,
                state, expires_at, created_at, owner_actor_id, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.intent_id.to_string())
        .bind(approval.proposal_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at)
        .bind(approval.created_at)
        .bind(&approval.owner_actor_id)
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
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateApproval {
                data: approval.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(approval)?;
        sqlx::query(
            "UPDATE approvals
             SET execution_id = ?2,
                 action_digest = ?3,
                 state = ?4,
                 expires_at = ?5,
                 owner_actor_id = ?6,
                 raw_json = ?7
             WHERE approval_id = ?1",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at)
        .bind(&approval.owner_actor_id)
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
        if let Some(ref queue) = self.write_queue {
            return queue.resolve_approval(approval_id, state, now).await;
        }
        resolve_approval_sqlite(&self.pool, approval_id, state, now).await
    }

    async fn list_pending(&self) -> Result<Vec<ApprovalRequest>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM approvals WHERE state = ?1 ORDER BY created_at DESC",
            |query| query.bind("Pending"),
        )
        .await
    }

    async fn list_pending_paginated(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals WHERE state = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect()
    }

    async fn list_pending_by_proposal_paginated(
        &self,
        proposal_id: ferrum_proto::ProposalId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals WHERE state = ?1 AND proposal_id = ?2 ORDER BY created_at DESC LIMIT ?3 OFFSET ?4";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(proposal_id.to_string())
            .bind(limit)
            .bind(offset)
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
        // Keyset pagination: WHERE (created_at, approval_id) < (cursor_created_at, cursor_approval_id)
        // for DESC ordering. We fetch one extra to determine if there are more results.
        let sql = "SELECT raw_json FROM approvals
            WHERE state = ?1
              AND (created_at, approval_id) < (?2, ?3)
            ORDER BY created_at DESC, approval_id DESC
            LIMIT ?4";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(created_after)
            .bind(approval_id_after.to_string())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect()
    }

    async fn list_pending_by_proposal_cursor(
        &self,
        proposal_id: ferrum_proto::ProposalId,
        created_after: Timestamp,
        approval_id_after: ApprovalId,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        let sql = "SELECT raw_json FROM approvals
            WHERE state = ?1 AND proposal_id = ?2
              AND (created_at, approval_id) < (?3, ?4)
            ORDER BY created_at DESC, approval_id DESC
            LIMIT ?5";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(proposal_id.to_string())
            .bind(created_after)
            .bind(approval_id_after.to_string())
            .bind(limit)
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
        if let Some(ref queue) = self.write_queue {
            return queue
                .expire_stale_pending(now, max_age_seconds, batch_size)
                .await;
        }
        expire_stale_pending_sqlite(&self.pool, now, max_age_seconds, batch_size).await
    }
}

/// Atomically resolve a pending approval to `state` using a conditional UPDATE
/// so only one concurrent resolver can win.
///
/// The predicate requires the row to still be `Pending` and not yet expired
/// (`expires_at > now`) at write time. Returns `Ok(true)` when this resolver
/// transitioned the row, `Ok(false)` when the row was not in a resolvable state
/// (missing, already terminal, or expired), and `Err` only for real storage
/// errors or an invalid resolve target (anything other than Granted/Denied).
pub(crate) async fn resolve_approval_sqlite(
    pool: &SqlitePool,
    approval_id: ApprovalId,
    state: ApprovalState,
    now: Timestamp,
) -> Result<bool> {
    let approval: Option<ApprovalRequest> =
        fetch_entity_by_id(pool, "approvals", "approval_id", &approval_id.to_string()).await?;
    let Some(mut approval) = approval else {
        return Ok(false);
    };
    // Validate the requested resolve target: resolve only accepts a terminal
    // decision (Granted/Denied). Any other target is a malformed request and is
    // rejected regardless of the current row state.
    if !matches!(state, ApprovalState::Granted | ApprovalState::Denied) {
        return Err(crate::StoreError::InvalidState(format!(
            "invalid approval resolve target {:?}: must be Granted or Denied",
            state
        )));
    }
    // A terminal/expired snapshot means this resolver lost the race (or the
    // approval was already decided). Return Ok(false) so the caller maps it to a
    // conflict rather than a server error. The conditional UPDATE below remains
    // the atomic guard for concurrent changes after this snapshot.
    if !matches!(approval.state, ApprovalState::Pending) {
        return Ok(false);
    }
    approval.state = state;
    // Stamp the resolution-evidence version in the SAME atomic CAS write as the
    // state transition. This marks the approval as resolved by the hardened
    // resolver so I6 role-bound binding requires authenticated resolver
    // evidence; the marker survives a later provenance-append failure because
    // it commits with the grant itself.
    approval.resolver_evidence_version = Some(ferrum_proto::CURRENT_RESOLVER_EVIDENCE_VERSION);
    let raw_json = to_json(&approval)?;

    let result = sqlx::query(
        "UPDATE approvals
         SET state = ?2,
             raw_json = ?3
         WHERE approval_id = ?1
           AND state = ?4
           AND expires_at > ?5",
    )
    .bind(approval_id.to_string())
    .bind(enum_text(&approval.state)?)
    .bind(&raw_json)
    .bind("Pending")
    .bind(now)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Atomically transition stale pending approvals to `Expired` using a
/// conditional UPDATE so concurrent resolves cannot be overwritten.
///
/// Only approvals whose database row is still `Pending` are transitioned and
/// returned. Rows that changed to a terminal state between selection and the
/// conditional UPDATE are silently skipped.
pub(crate) async fn expire_stale_pending_sqlite(
    pool: &SqlitePool,
    now: Timestamp,
    max_age_seconds: u64,
    batch_size: u32,
) -> Result<Vec<ApprovalRequest>> {
    let age_cutoff = now - chrono::Duration::seconds(max_age_seconds as i64);
    let sql = "SELECT approval_id, raw_json FROM approvals
        WHERE state = ?1
          AND (expires_at < ?2 OR created_at < ?3)
        LIMIT ?4";
    let rows = sqlx::query(sql)
        .bind("Pending")
        .bind(now)
        .bind(age_cutoff)
        .bind(batch_size)
        .fetch_all(pool)
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
             SET state = ?2,
                 raw_json = ?3
             WHERE approval_id = ?1
               AND state = ?4",
        )
        .bind(&approval_id_str)
        .bind(enum_text(&ApprovalState::Expired)?)
        .bind(&new_raw_json)
        .bind("Pending")
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            expired.push(approval);
        }
    }
    Ok(expired)
}
