//! PostgreSQL LedgerRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::EventId;

use super::skeleton_error;
use crate::{LedgerEntry, LedgerRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresLedgerRepo {
    _private: (),
}

impl PostgresLedgerRepo {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for PostgresLedgerRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LedgerRepo for PostgresLedgerRepo {
    async fn append(&self, _entry: &LedgerEntry) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get_by_event(&self, _event_id: EventId) -> Result<Option<LedgerEntry>> {
        Err(skeleton_error())
    }

    async fn list_recent(&self, _limit: u32) -> Result<Vec<LedgerEntry>> {
        Err(skeleton_error())
    }

    async fn get_latest(&self) -> Result<Option<LedgerEntry>> {
        Err(skeleton_error())
    }

    async fn verify_chain(&self) -> Result<()> {
        Err(skeleton_error())
    }
}
