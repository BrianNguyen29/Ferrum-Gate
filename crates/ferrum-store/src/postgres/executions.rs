//! PostgreSQL ExecutionRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{CapabilityId, ExecutionId, ExecutionRecord, ExecutionState, IntentId};
use sqlx::PgPool;

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};
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
                decision, state, started_at, finished_at, result_digest, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.intent_id.to_string())
        .bind(execution.proposal_id.to_string())
        .bind(execution.capability_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(execution.started_at)
        .bind(execution.finished_at)
        .bind(&execution.result_digest)
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
                 raw_json = $7
             WHERE execution_id = $1",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(execution.finished_at)
        .bind(&execution.result_digest)
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
        let mut execution = execution;
        let state_text = enum_text(&state)?;
        execution.state = state;
        let raw_json = to_json(&execution)?;
        sqlx::query("UPDATE executions SET state = $2, raw_json = $3 WHERE execution_id = $1")
            .bind(execution_id.to_string())
            .bind(state_text)
            .bind(raw_json)
            .execute(&self.pool)
            .await?;
        Ok(())
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
