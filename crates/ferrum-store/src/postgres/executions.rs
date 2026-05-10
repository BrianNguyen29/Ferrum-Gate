//! PostgreSQL ExecutionRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{CapabilityId, ExecutionId, ExecutionRecord, ExecutionState, IntentId};
use sqlx::PgPool;

use super::skeleton_error;
use crate::{ExecutionRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresExecutionRepo {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresExecutionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionRepo for PostgresExecutionRepo {
    async fn insert(&self, _execution: &ExecutionRecord) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _execution_id: ExecutionId) -> Result<Option<ExecutionRecord>> {
        Err(skeleton_error())
    }

    async fn update(&self, _execution: &ExecutionRecord) -> Result<()> {
        Err(skeleton_error())
    }

    async fn update_state(&self, _execution_id: ExecutionId, _state: ExecutionState) -> Result<()> {
        Err(skeleton_error())
    }

    async fn list_by_intent(&self, _intent_id: IntentId) -> Result<Vec<ExecutionRecord>> {
        Err(skeleton_error())
    }

    async fn list_by_capability(
        &self,
        _capability_id: CapabilityId,
    ) -> Result<Vec<ExecutionRecord>> {
        Err(skeleton_error())
    }
}
