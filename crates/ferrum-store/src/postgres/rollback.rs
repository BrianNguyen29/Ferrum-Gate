//! PostgreSQL RollbackRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{ExecutionId, RollbackContract, RollbackContractId, RollbackState};
use sqlx::PgPool;

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};
use crate::{Result, RollbackRepo};

#[derive(Clone)]
pub struct PostgresRollbackRepo {
    pool: PgPool,
}

impl PostgresRollbackRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RollbackRepo for PostgresRollbackRepo {
    async fn insert(&self, contract: &RollbackContract) -> Result<()> {
        let raw_json = to_json(contract)?;
        sqlx::query(
            "INSERT INTO rollback_contracts (
                contract_id, intent_id, proposal_id, execution_id, adapter_key,
                action_type, rollback_class, state, auto_commit, created_at, expires_at, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(contract.contract_id.to_string())
        .bind(contract.intent_id.to_string())
        .bind(contract.proposal_id.to_string())
        .bind(contract.execution_id.to_string())
        .bind(&contract.adapter_key)
        .bind(enum_text(&contract.action_type)?)
        .bind(enum_text(&contract.rollback_class)?)
        .bind(enum_text(&contract.state)?)
        .bind(contract.auto_commit)
        .bind(contract.created_at)
        .bind(contract.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, contract_id: RollbackContractId) -> Result<Option<RollbackContract>> {
        fetch_entity_by_id(
            &self.pool,
            "rollback_contracts",
            "contract_id",
            &contract_id.to_string(),
        )
        .await
    }

    async fn update(&self, contract: &RollbackContract) -> Result<()> {
        let raw_json = to_json(contract)?;
        sqlx::query(
            "UPDATE rollback_contracts
             SET state = $2,
                 auto_commit = $3,
                 expires_at = $4,
                 raw_json = $5
             WHERE contract_id = $1",
        )
        .bind(contract.contract_id.to_string())
        .bind(enum_text(&contract.state)?)
        .bind(contract.auto_commit)
        .bind(contract.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_state(
        &self,
        contract_id: RollbackContractId,
        state: RollbackState,
    ) -> Result<()> {
        let Some(mut contract) = self.get(contract_id).await? else {
            return Ok(());
        };
        contract.state = state;
        let raw_json = to_json(&contract)?;
        sqlx::query(
            "UPDATE rollback_contracts SET state = $2, raw_json = $3 WHERE contract_id = $1",
        )
        .bind(contract_id.to_string())
        .bind(enum_text(&contract.state)?)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_execution(&self, execution_id: ExecutionId) -> Result<Vec<RollbackContract>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM rollback_contracts WHERE execution_id = $1 ORDER BY created_at DESC",
            |query| query.bind(execution_id.to_string()),
        )
        .await
    }
}
