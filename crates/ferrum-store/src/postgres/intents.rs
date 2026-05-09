//! PostgreSQL IntentRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{IntentEnvelope, IntentId, IntentStatus};

use super::skeleton_error;
use crate::{IntentRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresIntentRepo {
    _private: (),
}

impl PostgresIntentRepo {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for PostgresIntentRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IntentRepo for PostgresIntentRepo {
    async fn insert(&self, _intent: &IntentEnvelope) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _intent_id: IntentId) -> Result<Option<IntentEnvelope>> {
        Err(skeleton_error())
    }

    async fn update(&self, _intent: &IntentEnvelope) -> Result<()> {
        Err(skeleton_error())
    }

    async fn update_status(&self, _intent_id: IntentId, _status: IntentStatus) -> Result<()> {
        Err(skeleton_error())
    }

    async fn list_by_status(&self, _status: IntentStatus) -> Result<Vec<IntentEnvelope>> {
        Err(skeleton_error())
    }

    async fn list_intents(
        &self,
        _intent_id: Option<IntentId>,
        _statuses: &[IntentStatus],
        _cursor: Option<&str>,
        _limit: u32,
    ) -> Result<(Vec<IntentEnvelope>, Option<String>)> {
        Err(skeleton_error())
    }

    async fn list_intents_with_exec_state(
        &self,
        _intent_id: Option<IntentId>,
        _statuses: &[IntentStatus],
        _cursor: Option<&str>,
        _limit: u32,
    ) -> Result<(Vec<(IntentEnvelope, Option<String>)>, Option<String>)> {
        Err(skeleton_error())
    }
}
