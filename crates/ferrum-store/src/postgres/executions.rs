//! PostgreSQL ExecutionRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{
    CapabilityId, ExecutionId, ExecutionRecord, ExecutionState, IntentId, Timestamp,
};
use sqlx::{PgPool, Row};

use super::helpers::{
    enum_text, fetch_entities, fetch_entity_by_id, from_json, opt_rfc3339_utc, rfc3339_utc, to_json,
};
use crate::{ExecutionRepo, Result, transitions};

#[derive(Clone)]
pub struct PostgresExecutionRepo {
    pool: PgPool,
}

impl PostgresExecutionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionRepo for PostgresExecutionRepo {
    async fn insert(&self, execution: &ExecutionRecord) -> Result<()> {
        let raw_json = to_json(execution)?;
        sqlx::query(
            "INSERT INTO executions (
                execution_id, intent_id, proposal_id, capability_id, rollback_contract_id,
                decision, state, started_at, finished_at, result_digest, owner_actor_id, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.intent_id.to_string())
        .bind(execution.proposal_id.to_string())
        .bind(execution.capability_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(rfc3339_utc(execution.started_at))
        .bind(opt_rfc3339_utc(execution.finished_at))
        .bind(&execution.result_digest)
        .bind(&execution.owner_actor_id)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, execution_id: ExecutionId) -> Result<Option<ExecutionRecord>> {
        fetch_entity_by_id(
            &self.pool,
            "executions",
            "execution_id",
            &execution_id.to_string(),
        )
        .await
    }

    async fn update(&self, execution: &ExecutionRecord) -> Result<()> {
        let raw_json = to_json(execution)?;
        sqlx::query(
            "UPDATE executions
             SET rollback_contract_id = $2,
                 decision = $3,
                 state = $4,
                 finished_at = $5,
                 result_digest = $6,
                 owner_actor_id = $7,
                 raw_json = $8
             WHERE execution_id = $1",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(opt_rfc3339_utc(execution.finished_at))
        .bind(&execution.result_digest)
        .bind(&execution.owner_actor_id)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_state(&self, execution_id: ExecutionId, state: ExecutionState) -> Result<()> {
        let Some(execution) = self.get(execution_id).await? else {
            return Ok(());
        };
        if !transitions::is_valid_execution_transition(&execution.state, &state) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid execution transition from {:?} to {:?}",
                execution.state, state
            )));
        }
        let state_text = enum_text(&state)?;
        sqlx::query(
            "UPDATE executions \
             SET state = $2, \
                 raw_json = (raw_json::jsonb || jsonb_build_object('state', $2::text))::text \
             WHERE execution_id = $1",
        )
        .bind(execution_id.to_string())
        .bind(state_text)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn compare_and_set_state(
        &self,
        execution_id: ExecutionId,
        expected_states: &[ExecutionState],
        new_state: ExecutionState,
    ) -> Result<bool> {
        if expected_states.is_empty() {
            return Ok(false);
        }
        let new_state_text = enum_text(&new_state)?;
        let expected = expected_states
            .iter()
            .map(enum_text)
            .collect::<Result<Vec<_>>>()?;
        let terminal = transitions::execution_state_is_terminal(&new_state);
        let result = if terminal {
            let finished_at = rfc3339_utc(chrono::Utc::now());
            sqlx::query(
                "UPDATE executions
                 SET state = $2,
                     finished_at = $3,
                     raw_json = (raw_json::jsonb || jsonb_build_object(
                         'state', $2::text,
                         'finished_at', $3::text
                     ))::text
                 WHERE execution_id = $1 AND state = ANY($4)",
            )
            .bind(execution_id.to_string())
            .bind(&new_state_text)
            .bind(&finished_at)
            .bind(expected)
            .execute(&self.pool)
            .await?
        } else {
            sqlx::query(
                "UPDATE executions
                 SET state = $2,
                     raw_json = (raw_json::jsonb || jsonb_build_object('state', $2::text))::text
                 WHERE execution_id = $1 AND state = ANY($3)",
            )
            .bind(execution_id.to_string())
            .bind(new_state_text)
            .bind(expected)
            .execute(&self.pool)
            .await?
        };
        Ok(result.rows_affected() == 1)
    }

    async fn list_stale_in_flight(
        &self,
        stale_before: Timestamp,
        states: &[ExecutionState],
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }
        let state_texts = states.iter().map(enum_text).collect::<Result<Vec<_>>>()?;
        let rows = sqlx::query(
            "SELECT raw_json FROM executions \
             WHERE state = ANY($1) \
               AND started_at < $2 \
               AND finished_at IS NULL \
             ORDER BY started_at ASC, execution_id ASC \
             LIMIT $3",
        )
        .bind(state_texts)
        .bind(rfc3339_utc(stale_before))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let raw: String = row.try_get("raw_json")?;
                from_json(&raw)
            })
            .collect()
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<ExecutionRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM executions WHERE intent_id = $1 ORDER BY started_at DESC, execution_id DESC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }

    async fn list_by_capability(
        &self,
        capability_id: CapabilityId,
    ) -> Result<Vec<ExecutionRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM executions WHERE capability_id = $1 ORDER BY started_at DESC, execution_id DESC",
            |query| query.bind(capability_id.to_string()),
        )
        .await
    }
}
