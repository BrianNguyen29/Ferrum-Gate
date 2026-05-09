//! PostgreSQL RollbackRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{ExecutionId, RollbackContract, RollbackContractId, RollbackState};

use super::skeleton_error;
use crate::{Result, RollbackRepo};

#[derive(Debug, Clone)]
pub struct PostgresRollbackRepo {
    _private: (),
}

impl PostgresRollbackRepo {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for PostgresRollbackRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RollbackRepo for PostgresRollbackRepo {
    async fn insert(&self, _contract: &RollbackContract) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _contract_id: RollbackContractId) -> Result<Option<RollbackContract>> {
        Err(skeleton_error())
    }

    async fn update(&self, _contract: &RollbackContract) -> Result<()> {
        Err(skeleton_error())
    }

    async fn update_state(
        &self,
        _contract_id: RollbackContractId,
        _state: RollbackState,
    ) -> Result<()> {
        Err(skeleton_error())
    }

    async fn list_by_execution(&self, _execution_id: ExecutionId) -> Result<Vec<RollbackContract>> {
        Err(skeleton_error())
    }
}
