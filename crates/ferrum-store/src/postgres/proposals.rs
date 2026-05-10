//! PostgreSQL ProposalRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{ActionProposal, IntentId, ProposalId};
use sqlx::PgPool;

use super::skeleton_error;
use crate::{ProposalRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresProposalRepo {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresProposalRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProposalRepo for PostgresProposalRepo {
    async fn insert(&self, _proposal: &ActionProposal) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _proposal_id: ProposalId) -> Result<Option<ActionProposal>> {
        Err(skeleton_error())
    }

    async fn list_by_intent(&self, _intent_id: IntentId) -> Result<Vec<ActionProposal>> {
        Err(skeleton_error())
    }
}
