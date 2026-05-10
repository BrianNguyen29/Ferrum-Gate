//! PostgreSQL CapabilityRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{CapabilityId, CapabilityLease, CapabilityStatus, IntentId};
use sqlx::PgPool;

use super::skeleton_error;
use crate::{CapabilityRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresCapabilityRepo {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresCapabilityRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CapabilityRepo for PostgresCapabilityRepo {
    async fn insert(&self, _capability: &CapabilityLease) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _capability_id: CapabilityId) -> Result<Option<CapabilityLease>> {
        Err(skeleton_error())
    }

    async fn update(&self, _capability: &CapabilityLease) -> Result<()> {
        Err(skeleton_error())
    }

    async fn update_status(
        &self,
        _capability_id: CapabilityId,
        _status: CapabilityStatus,
    ) -> Result<()> {
        Err(skeleton_error())
    }

    async fn list_by_intent(&self, _intent_id: IntentId) -> Result<Vec<CapabilityLease>> {
        Err(skeleton_error())
    }
}
