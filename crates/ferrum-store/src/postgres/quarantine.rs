//! PostgreSQL QuarantineHoldRepo implementation.

use async_trait::async_trait;
use ferrum_proto::{
    ActorRef, ProposalId, QuarantineHold, QuarantineHoldId, QuarantineHoldState, Timestamp,
};
use sqlx::{PgPool, Row};

use super::helpers::{enum_text, fetch_entity_by_id, from_json, to_json};
use crate::{QuarantineHoldRepo, Result};

#[derive(Clone)]
pub struct PostgresQuarantineHoldRepo {
    pool: PgPool,
}

impl PostgresQuarantineHoldRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QuarantineHoldRepo for PostgresQuarantineHoldRepo {
    async fn insert(&self, hold: &QuarantineHold) -> Result<()> {
        let raw_json = to_json(hold)?;
        sqlx::query(
            "INSERT INTO quarantine_holds (
                hold_id, intent_id, proposal_id, state, reason, matched_rule_ids,
                policy_bundle_id, expires_at, created_at, owner_actor_id, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(hold.hold_id.to_string())
        .bind(hold.intent_id.to_string())
        .bind(hold.proposal_id.to_string())
        .bind(enum_text(&hold.state)?)
        .bind(&hold.reason)
        .bind(serde_json::to_string(&hold.matched_rule_ids)?)
        .bind(hold.policy_bundle_id.as_ref())
        .bind(hold.expires_at.to_rfc3339())
        .bind(hold.created_at.to_rfc3339())
        .bind(&hold.owner_actor_id)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, hold_id: QuarantineHoldId) -> Result<Option<QuarantineHold>> {
        fetch_entity_by_id(
            &self.pool,
            "quarantine_holds",
            "hold_id",
            &hold_id.to_string(),
        )
        .await
    }

    async fn get_by_proposal(&self, proposal_id: ProposalId) -> Result<Option<QuarantineHold>> {
        let sql = "SELECT raw_json FROM quarantine_holds WHERE proposal_id = $1 ORDER BY created_at DESC LIMIT 1";
        let row = sqlx::query(sql)
            .bind(proposal_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => {
                let raw_json: String = row.try_get("raw_json")?;
                Ok(Some(from_json(&raw_json)?))
            }
            None => Ok(None),
        }
    }

    async fn list_pending(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<QuarantineHold>, Option<String>)> {
        let sql = "SELECT raw_json FROM quarantine_holds WHERE state = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3";
        let rows = sqlx::query(sql)
            .bind("Pending")
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?;
        let items: Vec<QuarantineHold> = rows
            .into_iter()
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = if items.len() == limit as usize {
            Some(((offset + limit) as i64).to_string())
        } else {
            None
        };
        Ok((items, next_cursor))
    }

    async fn resolve(
        &self,
        hold_id: QuarantineHoldId,
        allow: bool,
        actor: &ActorRef,
        reason: Option<&str>,
        resolved_at: Timestamp,
    ) -> Result<bool> {
        let Some(mut hold) = self.get(hold_id).await? else {
            return Ok(false);
        };
        if !matches!(hold.state, QuarantineHoldState::Pending) {
            return Ok(false);
        }

        hold.state = if allow {
            QuarantineHoldState::Allowed
        } else {
            QuarantineHoldState::Denied
        };
        hold.resolved_at = Some(resolved_at);
        hold.resolved_by = Some(actor.clone());
        hold.resolution_reason = reason.map(String::from);
        let raw_json = to_json(&hold)?;

        let result = sqlx::query(
            "UPDATE quarantine_holds
             SET state = $2,
                 resolved_at = $3,
                 resolved_by = $4,
                 resolution_reason = $5,
                 raw_json = $6
             WHERE hold_id = $1
               AND state = $7",
        )
        .bind(hold_id.to_string())
        .bind(enum_text(&hold.state)?)
        .bind(resolved_at.to_rfc3339())
        .bind(serde_json::to_string(actor)?)
        .bind(reason)
        .bind(raw_json)
        .bind("Pending")
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn expire_stale_pending(
        &self,
        now: Timestamp,
        max_age_seconds: u64,
        batch_size: u32,
    ) -> Result<Vec<QuarantineHold>> {
        let age_cutoff = now - chrono::Duration::seconds(max_age_seconds as i64);
        let sql = "SELECT hold_id, raw_json FROM quarantine_holds
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
            let hold_id_str: String = row.try_get("hold_id")?;
            let raw_json: String = row.try_get("raw_json")?;
            let mut hold: QuarantineHold = from_json(&raw_json)?;
            hold.state = QuarantineHoldState::Expired;
            let new_raw_json = to_json(&hold)?;

            let result = sqlx::query(
                "UPDATE quarantine_holds
                 SET state = $2,
                     raw_json = $3
                 WHERE hold_id = $1
                   AND state = $4",
            )
            .bind(&hold_id_str)
            .bind(enum_text(&QuarantineHoldState::Expired)?)
            .bind(&new_raw_json)
            .bind("Pending")
            .execute(&self.pool)
            .await?;

            if result.rows_affected() > 0 {
                expired.push(hold);
            }
        }
        Ok(expired)
    }
}
