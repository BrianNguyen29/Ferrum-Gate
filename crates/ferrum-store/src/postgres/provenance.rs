//! PostgreSQL ProvenanceRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::{EventId, ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest};
use sqlx::PgPool;

use super::skeleton_error;
use crate::{ProvenanceRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresProvenanceRepo {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresProvenanceRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProvenanceRepo for PostgresProvenanceRepo {
    async fn append_event(&self, _event: &ProvenanceEvent) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get_event(&self, _event_id: EventId) -> Result<Option<ProvenanceEvent>> {
        Err(skeleton_error())
    }

    async fn append_edges(&self, _to_event_id: EventId, _edges: &[ProvenanceEdge]) -> Result<()> {
        Err(skeleton_error())
    }

    async fn query(&self, _request: &ProvenanceQueryRequest) -> Result<Vec<ProvenanceEvent>> {
        Err(skeleton_error())
    }

    async fn get_edges_to(&self, _to_event_id: EventId) -> Result<Vec<ProvenanceEdge>> {
        Err(skeleton_error())
    }

    async fn get_edges_from(&self, _from_event_ids: &[EventId]) -> Result<Vec<ProvenanceEdge>> {
        Err(skeleton_error())
    }
}
