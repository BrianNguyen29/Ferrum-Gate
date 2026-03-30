use async_trait::async_trait;
use ferrum_proto::{
    EventId, ExecutionId, FlaggedEvent, ProvenanceEdge, ProvenanceEvent, ProvenanceQueryRequest,
    ProvenanceStatsRequest, ProvenanceStatsResponse,
};
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

    async fn collect_lineage_event_ids(
        &self,
        seed_event_ids: Vec<EventId>,
    ) -> Result<std::collections::HashSet<EventId>> {
        use std::collections::HashSet;

        let mut frontier = seed_event_ids;
        let mut visited: HashSet<EventId> = frontier.iter().copied().collect();

        while let Some(current_event_id) = frontier.pop() {
            let edges = self.get_edges_to(current_event_id).await?;
            for edge in edges {
                if visited.insert(edge.from_event_id) {
                    frontier.push(edge.from_event_id);
                }
            }
        }

        Ok(visited)
    }

    async fn fetch_events_by_ids(
        &self,
        event_ids: &std::collections::HashSet<EventId>,
    ) -> Result<Vec<ProvenanceEvent>> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut events: Vec<ProvenanceEvent> = Vec::with_capacity(event_ids.len());
        let event_ids_vec: Vec<EventId> = event_ids.iter().copied().collect();
        let chunk_size = 50;

        for chunk in event_ids_vec.chunks(chunk_size) {
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

        events.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
        Ok(events)
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
        let (events, _) = self.query_paginated(request).await?;
        Ok(events)
    }

    async fn query_paginated(
        &self,
        request: &ProvenanceQueryRequest,
    ) -> Result<(Vec<ProvenanceEvent>, Option<String>)> {
        let limit = request.limit.unwrap_or(100).clamp(1, 10000);

        // Build dynamic WHERE clause using positional params ($1, $2, etc.)
        let mut conditions = Vec::new();
        let mut sql_params: Vec<String> = Vec::new();

        if let Some(ref intent_id) = request.intent_id {
            conditions.push(format!("intent_id = ${}", sql_params.len() + 1));
            sql_params.push(intent_id.to_string());
        }

        if let Some(ref proposal_id) = request.proposal_id {
            conditions.push(format!("proposal_id = ${}", sql_params.len() + 1));
            sql_params.push(proposal_id.to_string());
        }

        if let Some(ref execution_id) = request.execution_id {
            conditions.push(format!("execution_id = ${}", sql_params.len() + 1));
            sql_params.push(execution_id.to_string());
        }

        if let Some(ref capability_id) = request.capability_id {
            conditions.push(format!("capability_id = ${}", sql_params.len() + 1));
            sql_params.push(capability_id.to_string());
        }

        if let Some(ref event_kind) = request.event_kind {
            let kind_text = enum_text(event_kind)?;
            conditions.push(format!("kind = ${}", sql_params.len() + 1));
            sql_params.push(kind_text);
        }

        if let Some(since) = request.since {
            conditions.push(format!("occurred_at >= ${}", sql_params.len() + 1));
            sql_params.push(since.to_string());
        }

        if let Some(until) = request.until {
            conditions.push(format!("occurred_at <= ${}", sql_params.len() + 1));
            sql_params.push(until.to_string());
        }

        // Cursor decode: format is "occurred_at|event_id"
        // We need (occurred_at, event_id) > cursor for keyset pagination
        if let Some(ref cursor) = request.cursor {
            if let Some((cursor_ts, cursor_eid)) = cursor.split_once('|') {
                // Add cursor condition: (occurred_at > cursor_ts) OR (occurred_at = cursor_ts AND event_id > cursor_eid)
                let idx = sql_params.len() + 1;
                conditions.push(format!(
                    "(occurred_at > ${} OR (occurred_at = ${} AND event_id > ${}))",
                    idx,
                    idx,
                    idx + 1
                ));
                sql_params.push(cursor_ts.to_string());
                sql_params.push(cursor_eid.to_string());
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Fetch limit+1 to detect if there's a next page
        let fetch_limit = limit + 1;

        let sql = format!(
            "SELECT raw_json, occurred_at, event_id
             FROM provenance_events
             {}
             ORDER BY occurred_at ASC, event_id ASC
             LIMIT ${}",
            where_clause,
            sql_params.len() + 1
        );

        let mut query = sqlx::query(&sql);
        for param in &sql_params {
            query = query.bind(param);
        }
        query = query.bind(fetch_limit);

        let rows = query.fetch_all(&self.pool).await?;

        let has_next_page = rows.len() > limit as usize;
        let rows_to_return = if has_next_page {
            &rows[..limit as usize]
        } else {
            &rows
        };

        let mut events = Vec::with_capacity(rows_to_return.len());
        let mut next_cursor = None;

        for row in rows_to_return {
            let raw_json: String = row.try_get("raw_json")?;
            let occurred_at: chrono::DateTime<chrono::Utc> = row.try_get("occurred_at")?;
            let event_id_str: String = row.try_get("event_id")?;

            let event: ProvenanceEvent = serde_json::from_str(&raw_json).map_err(|e| {
                crate::StoreError::Internal(format!("failed to deserialize event: {}", e))
            })?;

            // Set next_cursor from the last event
            // Use | as separator since UUIDs only contain hex and hyphens, and timestamps use colons/dots
            next_cursor = Some(format!("{}|{}", occurred_at.to_rfc3339(), event_id_str));

            events.push(event);
        }

        // If there's a next page, we already have the cursor from the last item
        // If no next page, clear the cursor
        if !has_next_page {
            next_cursor = None;
        }

        Ok((events, next_cursor))
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
                "ObservedBy" => ferrum_proto::ProvenanceEdgeType::ObservedBy,
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

    async fn get_edges_from(&self, event_id: EventId) -> Result<Vec<ProvenanceEdge>> {
        let rows = sqlx::query(
            "SELECT to_event_id, edge_type, summary
             FROM provenance_edges
             WHERE from_event_id = ?1",
        )
        .bind(event_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut edges = Vec::with_capacity(rows.len());
        for row in rows {
            let to_event_id_str: String = row.try_get("to_event_id")?;
            let uuid = uuid::Uuid::parse_str(&to_event_id_str).map_err(|e| {
                crate::StoreError::Internal(format!("invalid event_id in edge: {}", e))
            })?;
            let to_event_id = EventId(uuid);

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
                "ObservedBy" => ferrum_proto::ProvenanceEdgeType::ObservedBy,
                _other => ferrum_proto::ProvenanceEdgeType::References, // fallback
            };

            let summary: Option<String> = row.try_get("summary")?;

            edges.push(ProvenanceEdge {
                edge_type,
                from_event_id: to_event_id, // Note: from_event_id in ProvenanceEdge is the source, which here is the to_event_id from the DB (child)
                summary,
            });
        }

        Ok(edges)
    }

    async fn get_lineage_by_event(&self, event_id: EventId) -> Result<Vec<ProvenanceEvent>> {
        let visited = self.collect_lineage_event_ids(vec![event_id]).await?;
        self.fetch_events_by_ids(&visited).await
    }

    async fn get_lineage_by_execution(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Vec<ProvenanceEvent>> {
        // Phase 1: collect all event_ids reachable from this execution
        // Start with events directly tagged with execution_id
        let direct_rows =
            sqlx::query("SELECT event_id FROM provenance_events WHERE execution_id = ?1")
                .bind(execution_id.to_string())
                .fetch_all(&self.pool)
                .await?;

        let mut direct_event_ids: Vec<EventId> = Vec::new();

        for row in direct_rows {
            let event_id_str: String = row.try_get("event_id")?;
            let uuid = uuid::Uuid::parse_str(&event_id_str)
                .map_err(|e| crate::StoreError::Internal(format!("invalid event_id: {}", e)))?;
            let event_id = EventId(uuid);
            direct_event_ids.push(event_id);
        }

        let visited = self.collect_lineage_event_ids(direct_event_ids).await?;
        self.fetch_events_by_ids(&visited).await
    }

    async fn query_stats(
        &self,
        request: &ProvenanceStatsRequest,
    ) -> Result<ProvenanceStatsResponse> {
        let max_events = request.max_events.unwrap_or(10_000).min(100_000) as usize;

        // Convert stats request to query request for fetching events
        let query_request = ProvenanceQueryRequest {
            intent_id: request.intent_id.clone(),
            proposal_id: request.proposal_id.clone(),
            execution_id: request.execution_id.clone(),
            capability_id: request.capability_id.clone(),
            event_kind: request.event_kind.clone(),
            terminal_only: None,
            since: request.since.clone(),
            until: request.until.clone(),
            limit: Some(max_events as u32),
            cursor: None,
        };

        let (events, _) = self.query_paginated(&query_request).await?;

        // Compute stats from events
        let mut total_events: u64 = 0;
        let mut kinds: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        let mut terminal_count: u64 = 0;
        let mut issue_count: u64 = 0;
        let mut events_without_execution_id: u64 = 0;
        let mut unique_intents: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut unique_proposals: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut unique_executions: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut flagged_events: Vec<FlaggedEvent> = Vec::new();

        // Terminal event kinds
        let terminal_kinds = [
            "SideEffectCommitted",
            "SideEffectCompensated",
            "SideEffectRolledBack",
            "ApprovalDenied",
            "Quarantined",
            "ErrorRaised",
        ];
        // Issue event kinds
        let issue_kinds = [
            "ErrorRaised",
            "Quarantined",
            "ApprovalDenied",
            "SideEffectRolledBack",
        ];

        for event in &events {
            total_events += 1;

            // Count by kind
            let kind_str = format!("{:?}", event.kind);
            *kinds.entry(kind_str.clone()).or_insert(0) += 1;

            // Check if terminal
            if terminal_kinds.contains(&kind_str.as_str()) {
                terminal_count += 1;
            }

            // Check if issue
            if issue_kinds.contains(&kind_str.as_str()) {
                issue_count += 1;
            }

            // Track events without execution_id
            if event.execution_id.is_none() {
                events_without_execution_id += 1;
            }

            // Track unique entities
            if let Some(ref intent_id) = event.intent_id {
                unique_intents.insert(intent_id.to_string());
            }
            if let Some(ref proposal_id) = event.proposal_id {
                unique_proposals.insert(proposal_id.to_string());
            }
            if let Some(ref execution_id) = event.execution_id {
                unique_executions.insert(execution_id.to_string());
            }

            // Flag terminal events missing execution_id
            if terminal_kinds.contains(&kind_str.as_str()) && event.execution_id.is_none() {
                flagged_events.push(FlaggedEvent {
                    event_id: event.event_id,
                    kind: event.kind.clone(),
                    reason: "terminal event missing execution_id".to_string(),
                });
            }
        }

        // Limit flagged events
        if flagged_events.len() > 100 {
            flagged_events.truncate(100);
        }

        Ok(ProvenanceStatsResponse {
            total_events,
            kinds,
            terminal_count,
            issue_count,
            events_without_execution_id,
            unique_intents: unique_intents.len() as u64,
            unique_proposals: unique_proposals.len() as u64,
            unique_executions: unique_executions.len() as u64,
            flagged_events,
        })
    }
}
