use async_trait::async_trait;
use ferrum_proto::{EventId, ExecutionId, ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest};
use sqlx::{Row, SqlitePool};

use crate::{ProvenanceRepo, Result};

use super::helpers::{enum_text, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteProvenanceRepo {
    pool: SqlitePool,
}

impl SqliteProvenanceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProvenanceRepo for SqliteProvenanceRepo {
    async fn append_event(&self, event: &ProvenanceEvent) -> Result<()> {
        let raw_json = to_json(event)?;
        sqlx::query(
            "INSERT INTO provenance_events (
                event_id, kind, occurred_at, intent_id, proposal_id, execution_id,
                capability_id, rollback_contract_id, policy_bundle_id, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(event.event_id.to_string())
        .bind(enum_text(&event.kind)?)
        .bind(event.occurred_at)
        .bind(event.intent_id.map(|id| id.to_string()))
        .bind(event.proposal_id.map(|id| id.to_string()))
        .bind(event.execution_id.map(|id| id.to_string()))
        .bind(event.capability_id.map(|id| id.to_string()))
        .bind(event.rollback_contract_id.map(|id| id.to_string()))
        .bind(event.policy_bundle_id.map(|id| id.to_string()))
        .bind(raw_json)
        .execute(&self.pool)
        .await?;

        // Persist parent edges if present
        if !event.parent_edges.is_empty() {
            self.append_edges(event.event_id, &event.parent_edges)
                .await?;
        }

        Ok(())
    }

    async fn get_event(&self, event_id: EventId) -> Result<Option<ProvenanceEvent>> {
        fetch_entity_by_id(
            &self.pool,
            "provenance_events",
            "event_id",
            &event_id.to_string(),
        )
        .await
    }

    async fn append_edges(&self, to_event_id: EventId, edges: &[ProvenanceEdge]) -> Result<()> {
        for edge in edges {
            sqlx::query(
                "INSERT INTO provenance_edges (to_event_id, from_event_id, edge_type, summary)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(to_event_id.to_string())
            .bind(edge.from_event_id.to_string())
            .bind(enum_text(&edge.edge_type)?)
            .bind(&edge.summary)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn query(&self, request: &ProvenanceQueryRequest) -> Result<Vec<ProvenanceEvent>> {
        let rows = sqlx::query(
            "SELECT raw_json, kind, intent_id, execution_id, capability_id
             FROM provenance_events
             ORDER BY occurred_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let requested_kind = request.event_kind.as_ref().map(enum_text).transpose()?;
        let mut events = Vec::with_capacity(rows.len());

        for row in rows {
            let kind: String = row.try_get("kind")?;
            let intent_id: Option<String> = row.try_get("intent_id")?;
            let execution_id: Option<String> = row.try_get("execution_id")?;
            let capability_id: Option<String> = row.try_get("capability_id")?;

            if let Some(filter_intent) = request.intent_id {
                if intent_id.as_deref() != Some(&filter_intent.to_string()) {
                    continue;
                }
            }

            if let Some(filter_execution) = request.execution_id {
                if execution_id.as_deref() != Some(&filter_execution.to_string()) {
                    continue;
                }
            }

            if let Some(filter_capability) = request.capability_id {
                if capability_id.as_deref() != Some(&filter_capability.to_string()) {
                    continue;
                }
            }

            if let Some(filter_kind) = requested_kind.as_deref() {
                if kind != filter_kind {
                    continue;
                }
            }

            let raw_json: String = row.try_get("raw_json")?;
            let event: ProvenanceEvent = serde_json::from_str(&raw_json)?;

            if let Some(since) = request.since {
                if event.occurred_at < since {
                    continue;
                }
            }

            if let Some(until) = request.until {
                if event.occurred_at > until {
                    continue;
                }
            }

            events.push(event);
        }

        Ok(events)
    }

    async fn get_edges_to(&self, event_id: EventId) -> Result<Vec<ProvenanceEdge>> {
        let rows = sqlx::query(
            "SELECT from_event_id, edge_type, summary
             FROM provenance_edges
             WHERE to_event_id = ?1",
        )
        .bind(event_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut edges = Vec::with_capacity(rows.len());
        for row in rows {
            let from_event_id_str: String = row.try_get("from_event_id")?;
            let uuid = uuid::Uuid::parse_str(&from_event_id_str).map_err(|e| {
                crate::StoreError::Internal(format!("invalid event_id in edge: {}", e))
            })?;
            let from_event_id = EventId(uuid);

            let edge_type_str: String = row.try_get("edge_type")?;
            let edge_type = match edge_type_str.as_str() {
                "DerivedFrom" => ferrum_proto::ProvenanceEdgeType::DerivedFrom,
                "AuthorizedBy" => ferrum_proto::ProvenanceEdgeType::AuthorizedBy,
                "ApprovedBy" => ferrum_proto::ProvenanceEdgeType::ApprovedBy,
                "TaintedBy" => ferrum_proto::ProvenanceEdgeType::TaintedBy,
                "UsesManifest" => ferrum_proto::ProvenanceEdgeType::UsesManifest,
                "EvaluatedByPolicy" => ferrum_proto::ProvenanceEdgeType::EvaluatedByPolicy,
                "Caused" => ferrum_proto::ProvenanceEdgeType::Caused,
                "Compensates" => ferrum_proto::ProvenanceEdgeType::Compensates,
                "Verifies" => ferrum_proto::ProvenanceEdgeType::Verifies,
                "References" => ferrum_proto::ProvenanceEdgeType::References,
                _other => ferrum_proto::ProvenanceEdgeType::References, // fallback
            };

            let summary: Option<String> = row.try_get("summary")?;

            edges.push(ProvenanceEdge {
                edge_type,
                from_event_id,
                summary,
            });
        }

        Ok(edges)
    }

    async fn get_lineage_by_execution(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Vec<ProvenanceEvent>> {
        use std::collections::HashSet;

        // Phase 1: collect all event_ids reachable from this execution
        // Start with events directly tagged with execution_id
        let direct_rows =
            sqlx::query("SELECT event_id FROM provenance_events WHERE execution_id = ?1")
                .bind(execution_id.to_string())
                .fetch_all(&self.pool)
                .await?;

        let mut frontier: Vec<EventId> = Vec::new();
        let mut visited: HashSet<EventId> = HashSet::new();

        for row in direct_rows {
            let event_id_str: String = row.try_get("event_id")?;
            let uuid = uuid::Uuid::parse_str(&event_id_str)
                .map_err(|e| crate::StoreError::Internal(format!("invalid event_id: {}", e)))?;
            let event_id = EventId(uuid);
            visited.insert(event_id);
            frontier.push(event_id);
        }

        // Phase 2: iteratively walk backwards via edges (BFS)
        while let Some(current_event_id) = frontier.pop() {
            let edges = self.get_edges_to(current_event_id).await?;
            for edge in edges {
                if !visited.contains(&edge.from_event_id) {
                    visited.insert(edge.from_event_id);
                    frontier.push(edge.from_event_id);
                }
            }
        }

        // Phase 3: fetch full event records for all visited ids
        if visited.is_empty() {
            return Ok(Vec::new());
        }

        let mut events: Vec<ProvenanceEvent> = Vec::with_capacity(visited.len());

        // Build a query with IN clause using placeholders
        // SQLite LIMIT for IN clause: we'll batch in chunks of 50
        let visited_vec: Vec<EventId> = visited.into_iter().collect();
        let chunk_size = 50;

        for chunk in visited_vec.chunks(chunk_size) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let sql = format!(
                "SELECT raw_json FROM provenance_events WHERE event_id IN ({})",
                placeholders.join(",")
            );

            let mut query = sqlx::query(&sql);
            for id in chunk {
                query = query.bind(id.to_string());
            }

            let rows = query.fetch_all(&self.pool).await?;
            for row in rows {
                let raw_json: String = row.try_get("raw_json")?;
                let event: ProvenanceEvent = serde_json::from_str(&raw_json).map_err(|e| {
                    crate::StoreError::Internal(format!("failed to deserialize event: {}", e))
                })?;
                events.push(event);
            }
        }

        // Sort by occurred_at ascending
        events.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));

        Ok(events)
    }
}
