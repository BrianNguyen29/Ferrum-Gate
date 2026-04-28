use async_trait::async_trait;
use ferrum_proto::{ActionProposal, IntentId, ProposalId};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{ProposalRepo, Result};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteProposalRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteProposalRepo {
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
impl ProposalRepo for SqliteProposalRepo {
    async fn insert(&self, proposal: &ActionProposal) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertProposal {
                data: proposal.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(proposal)?;
        sqlx::query(
            "INSERT INTO proposals (
                proposal_id, intent_id, step_index, server_name, tool_name,
                estimated_risk, requested_rollback_class, created_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
            "SELECT raw_json FROM proposals WHERE intent_id = ?1 ORDER BY step_index ASC, created_at ASC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }
}
