//! PostgreSQL ProposalRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{ActionProposal, IntentId, ProposalId};
use sqlx::PgPool;

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};
use crate::{ProposalRepo, Result};

#[derive(Clone)]
pub struct PostgresProposalRepo {
    pool: PgPool,
}

impl PostgresProposalRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProposalRepo for PostgresProposalRepo {
    async fn insert(&self, proposal: &ActionProposal) -> Result<()> {
        let raw_json = to_json(proposal)?;
        sqlx::query(
            "INSERT INTO proposals (
                proposal_id, intent_id, step_index, server_name, tool_name,
                estimated_risk, requested_rollback_class, created_at, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(proposal.proposal_id.to_string())
        .bind(proposal.intent_id.to_string())
        .bind(i64::from(proposal.step_index))
        .bind(&proposal.server_name)
        .bind(&proposal.tool_name)
        .bind(enum_text(&proposal.estimated_risk)?)
        .bind(enum_text(&proposal.requested_rollback_class)?)
        .bind(proposal.created_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, proposal_id: ProposalId) -> Result<Option<ActionProposal>> {
        fetch_entity_by_id(
            &self.pool,
            "proposals",
            "proposal_id",
            &proposal_id.to_string(),
        )
        .await
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<ActionProposal>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM proposals WHERE intent_id = $1 ORDER BY step_index ASC, created_at ASC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }
}
