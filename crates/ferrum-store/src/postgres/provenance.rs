//! PostgreSQL ProvenanceRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::{EventId, ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest};
use sqlx::{PgPool, Row};

use crate::{ProvenanceRepo, Result, StoreError};

use super::helpers::{enum_text, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct PostgresProvenanceRepo {
    pool: PgPool,
}

impl PostgresProvenanceRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProvenanceRepo for PostgresProvenanceRepo {
    async fn append_event(&self, event: &ProvenanceEvent) -> Result<()> {
        let raw_json = to_json(event)?;
        sqlx::query(
            "INSERT INTO provenance_events (
                event_id, kind, occurred_at, intent_id, proposal_id, execution_id,
                capability_id, rollback_contract_id, policy_bundle_id, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
        Ok(())
    }

    async fn append_event_with_edges(
        &self,
        event: &ProvenanceEvent,
        edges: &[ProvenanceEdge],
    ) -> Result<()> {
        for edge in edges {
            if edge.to_event_id.is_some_and(|id| id != event.event_id) {
                return Err(StoreError::Other(
                    "provenance edge to_event_id does not match appended event".to_string(),
                ));
            }
        }

        let raw_json = to_json(event)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO provenance_events (
                event_id, kind, occurred_at, intent_id, proposal_id, execution_id,
                capability_id, rollback_contract_id, policy_bundle_id, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
        .execute(&mut *tx)
        .await?;

        for edge in edges {
            sqlx::query(
                "INSERT INTO provenance_edges (to_event_id, from_event_id, edge_type, summary)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(event.event_id.to_string())
            .bind(edge.from_event_id.to_string())
            .bind(enum_text(&edge.edge_type)?)
            .bind(&edge.summary)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
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
                 VALUES ($1, $2, $3, $4)",
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

        for row in &rows {
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

            if !request.edge_types.is_empty() {
                let parent_edges = self.get_edges_to(event.event_id).await?;
                let has_matching_edge = parent_edges
                    .iter()
                    .any(|e| request.edge_types.contains(&e.edge_type));
                if !has_matching_edge {
                    continue;
                }
            }

            events.push(event);
        }

        Ok(events)
    }

    async fn get_edges_to(&self, to_event_id: EventId) -> Result<Vec<ProvenanceEdge>> {
        let rows = sqlx::query(
            "SELECT from_event_id, edge_type, summary
             FROM provenance_edges
             WHERE to_event_id = $1",
        )
        .bind(to_event_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut edges = Vec::with_capacity(rows.len());
        for row in rows {
            let from_event_id: String = row.try_get("from_event_id")?;
            let edge_type: String = row.try_get("edge_type")?;
            let summary: Option<String> = row.try_get("summary")?;

            let from_id = from_event_id
                .parse::<uuid::Uuid>()
                .map_err(|e| StoreError::Other(e.to_string()))?;
            let edge_type_parsed = serde_json::from_str(&format!("\"{edge_type}\""))?;

            edges.push(ProvenanceEdge {
                edge_type: edge_type_parsed,
                from_event_id: EventId(from_id),
                to_event_id: Some(to_event_id),
                summary,
            });
        }
        Ok(edges)
    }

    async fn get_edges_from(&self, from_event_ids: &[EventId]) -> Result<Vec<ProvenanceEdge>> {
        if from_event_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = (1..=from_event_ids.len())
            .map(|i| format!("${i}"))
            .collect();
        let query = format!(
            "SELECT to_event_id, from_event_id, edge_type, summary
             FROM provenance_edges
             WHERE from_event_id IN ({})",
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query(&query);
        for id in from_event_ids {
            query_builder = query_builder.bind(id.to_string());
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut edges = Vec::with_capacity(rows.len());
        for row in rows {
            let to_event_id_str: String = row.try_get("to_event_id")?;
            let from_event_id_str: String = row.try_get("from_event_id")?;
            let edge_type: String = row.try_get("edge_type")?;
            let summary: Option<String> = row.try_get("summary")?;

            let from_id = from_event_id_str
                .parse::<uuid::Uuid>()
                .map_err(|e| StoreError::Other(e.to_string()))?;
            let to_id = to_event_id_str
                .parse::<uuid::Uuid>()
                .map_err(|e| StoreError::Other(e.to_string()))?;
            let edge_type_parsed = serde_json::from_str(&format!("\"{edge_type}\""))?;

            edges.push(ProvenanceEdge {
                edge_type: edge_type_parsed,
                from_event_id: EventId(from_id),
                to_event_id: Some(EventId(to_id)),
                summary,
            });
        }
        Ok(edges)
    }
}
