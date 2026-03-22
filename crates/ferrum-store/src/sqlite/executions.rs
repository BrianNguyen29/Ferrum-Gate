use async_trait::async_trait;
use ferrum_proto::{CapabilityId, ExecutionId, ExecutionRecord, ExecutionState, IntentId};
use sqlx::SqlitePool;

use crate::{ExecutionRepo, Result};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteExecutionRepo {
    pool: SqlitePool,
}

impl SqliteExecutionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionRepo for SqliteExecutionRepo {
    async fn insert(&self, execution: &ExecutionRecord) -> Result<()> {
        let raw_json = to_json(execution)?;
        sqlx::query(
            "INSERT INTO executions (
                execution_id, intent_id, proposal_id, capability_id, rollback_contract_id,
                decision, state, started_at, finished_at, result_digest, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
             SET rollback_contract_id = ?2,
                 decision = ?3,
                 state = ?4,
                 finished_at = ?5,
                 result_digest = ?6,
                 raw_json = ?7
             WHERE execution_id = ?1",
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
        let Some(mut execution) = self.get(execution_id).await? else {
            return Ok(());
        };
        execution.state = state;
        self.update(&execution).await
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<ExecutionRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM executions WHERE intent_id = ?1 ORDER BY started_at DESC",
            intent_id.to_string(),
        )
        .await
    }

    async fn list_by_capability(
        &self,
        capability_id: CapabilityId,
    ) -> Result<Vec<ExecutionRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM executions WHERE capability_id = ?1 ORDER BY started_at DESC",
            capability_id.to_string(),
        )
        .await
    }
}
