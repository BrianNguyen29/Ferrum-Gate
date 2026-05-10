//! PostgreSQL ApprovalRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{ApprovalId, ApprovalRequest, ApprovalState, ProposalId, Timestamp};
use sqlx::PgPool;

use super::skeleton_error;
use crate::{ApprovalRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresApprovalRepo {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresApprovalRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApprovalRepo for PostgresApprovalRepo {
    async fn insert(&self, _approval: &ApprovalRequest) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _approval_id: ApprovalId) -> Result<Option<ApprovalRequest>> {
        Err(skeleton_error())
    }

    async fn update(&self, _approval: &ApprovalRequest) -> Result<()> {
        Err(skeleton_error())
    }

    async fn resolve(&self, _approval_id: ApprovalId, _state: ApprovalState) -> Result<()> {
        Err(skeleton_error())
    }

    async fn list_pending(&self) -> Result<Vec<ApprovalRequest>> {
        Err(skeleton_error())
    }

    async fn list_pending_paginated(
        &self,
        _limit: u32,
        _offset: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        Err(skeleton_error())
    }

    async fn list_pending_by_proposal_paginated(
        &self,
        _proposal_id: ProposalId,
        _limit: u32,
        _offset: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        Err(skeleton_error())
    }

    async fn list_pending_cursor(
        &self,
        _created_after: Timestamp,
        _approval_id_after: ApprovalId,
        _limit: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        Err(skeleton_error())
    }

    async fn list_pending_by_proposal_cursor(
        &self,
        _proposal_id: ProposalId,
        _created_after: Timestamp,
        _approval_id_after: ApprovalId,
        _limit: u32,
    ) -> Result<Vec<ApprovalRequest>> {
        Err(skeleton_error())
    }
}
