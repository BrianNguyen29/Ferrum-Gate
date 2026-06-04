//! Lineage / provenance execution-detail handlers.
//!
//! Routes:
//! - `GET  /v1/provenance/lineage/{execution_id}` -> [`get_execution_lineage`]
//! - `POST /v1/provenance/lineage`                 -> [`query_lineage`]
//! - `GET  /v1/executions/{execution_id}`         -> [`get_execution`]
//!
//! All success paths increment the `GovernanceRoute` counter and apply the
//! output sanitizer to the response payload.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use ferrum_graph::LineageGraph;
use ferrum_proto::{
    ApiErrorCode, EventId, ExecutionDetailResponse, ExecutionId, LineageDirection,
    LineageQueryRequest, LineageQueryResponse, ProvenanceQueryRequest,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::response::sanitize_json;
use crate::state::AppState;

/// Parse a path-segment execution id into a typed `ExecutionId`.
///
/// Mirrors `crate::server::parse_execution_id` so this module stays
/// self-contained while the lifecycle modules (execution.rs) are not yet
/// wired into `lib.rs`.
fn parse_execution_id(value: &str) -> Result<ExecutionId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid execution uuid",
        )
    })?;
    Ok(ExecutionId(parsed))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct LineageResponse {
    execution_id: ExecutionId,
    events: Vec<ferrum_proto::ProvenanceEvent>,
}

pub(crate) async fn get_execution_lineage(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<LineageResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ProvenanceLineageExecutionId, e)
    })?;

    let request = ProvenanceQueryRequest {
        intent_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        since: None,
        until: None,
        edge_types: Vec::new(),
    };

    let events = state
        .runtime
        .store
        .provenance()
        .query(&request)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineageExecutionId,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Build a map of target_event_id -> edges for efficient parent edge lookup
    let mut edges_by_target: HashMap<String, Vec<ferrum_proto::ProvenanceEdge>> = HashMap::new();
    for event in &events {
        let edges = state
            .runtime
            .store
            .provenance()
            .get_edges_to(event.event_id)
            .await
            .map_err(|e| {
                state.metrics.record_governance_error(
                    GovernanceRoute::ProvenanceLineageExecutionId,
                    ApiProblem::internal(anyhow::Error::from(e)),
                )
            })?;
        edges_by_target.insert(event.event_id.to_string(), edges);
    }

    let mut graph = LineageGraph::default();
    for event in &events {
        graph.add_event(event.clone());
    }
    for (child_id, edges) in &edges_by_target {
        for edge in edges {
            let parent_id = edge.from_event_id.to_string();
            let child_id = child_id.to_string();
            graph.add_edge(&parent_id, &child_id);
        }
    }

    let response = LineageResponse {
        execution_id,
        events,
    };
    // I11: sanitize response to strip control characters from string fields
    let json_val = serde_json::to_value(&response).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ProvenanceLineageExecutionId,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: LineageResponse = serde_json::from_value(sanitized).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ProvenanceLineageExecutionId,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    governance_ok!(
        state,
        GovernanceRoute::ProvenanceLineageExecutionId,
        Ok(Json(sanitized_response))
    )
}

/// Multi-hop lineage query from a seed event_id.
/// Traverses ancestor and/or descendant edges up to max_hops depth.
pub(crate) async fn query_lineage(
    State(state): State<Arc<AppState>>,
    Json(request): Json<LineageQueryRequest>,
) -> Result<Json<LineageQueryResponse>, ApiProblem> {
    let max_hops = request.max_hops.clamp(1, 10);

    // Fetch the seed event
    let seed_event = state
        .runtime
        .store
        .provenance()
        .get_event(request.event_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineage,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineage,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "seed event not found",
                ),
            )
        })?;

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    visited.insert(request.event_id.to_string());

    let mut events: Vec<ferrum_proto::ProvenanceEvent> = vec![seed_event];
    let mut edges: Vec<ferrum_proto::ProvenanceEdge> = Vec::new();

    // BFS traversal
    let mut frontier: Vec<String> = vec![request.event_id.to_string()];
    let mut next_frontier: Vec<String> = Vec::new();

    for _hop in 0..max_hops {
        if frontier.is_empty() {
            break;
        }

        for event_id_str in &frontier {
            let event_id: EventId =
                event_id_str
                    .parse::<uuid::Uuid>()
                    .map(EventId)
                    .map_err(|_| {
                        state.metrics.record_governance_error(
                            GovernanceRoute::ProvenanceLineage,
                            ApiProblem::new(
                                StatusCode::BAD_REQUEST,
                                ApiErrorCode::ValidationError,
                                "invalid event_id format: must be a valid UUID",
                            ),
                        )
                    })?;

            // Ancestor traversal: get_edges_to returns parent edges (from_event_id is parent)
            if matches!(
                request.direction,
                LineageDirection::Ancestors | LineageDirection::Both
            ) {
                let parent_edges = state
                    .runtime
                    .store
                    .provenance()
                    .get_edges_to(event_id)
                    .await
                    .map_err(|e| {
                        state.metrics.record_governance_error(
                            GovernanceRoute::ProvenanceLineage,
                            ApiProblem::internal(anyhow::Error::from(e)),
                        )
                    })?;

                for edge in &parent_edges {
                    if visited.insert(edge.from_event_id.to_string()) {
                        // Fetch the parent event
                        if let Some(parent_event) = state
                            .runtime
                            .store
                            .provenance()
                            .get_event(edge.from_event_id)
                            .await
                            .map_err(|e| {
                                state.metrics.record_governance_error(
                                    GovernanceRoute::ProvenanceLineage,
                                    ApiProblem::internal(anyhow::Error::from(e)),
                                )
                            })?
                        {
                            events.push(parent_event);
                            next_frontier.push(edge.from_event_id.to_string());
                        }
                    }
                    edges.push(edge.clone());
                }
            }

            // Descendant traversal: get_edges_from returns child edges (to_event_id is child)
            if matches!(
                request.direction,
                LineageDirection::Descendants | LineageDirection::Both
            ) {
                let child_edges = state
                    .runtime
                    .store
                    .provenance()
                    .get_edges_from(&[event_id])
                    .await
                    .map_err(|e| {
                        state.metrics.record_governance_error(
                            GovernanceRoute::ProvenanceLineage,
                            ApiProblem::internal(anyhow::Error::from(e)),
                        )
                    })?;

                for edge in &child_edges {
                    if let Some(to_id) = &edge.to_event_id {
                        if visited.insert(to_id.to_string()) {
                            // Fetch the child event
                            if let Some(child_event) = state
                                .runtime
                                .store
                                .provenance()
                                .get_event(*to_id)
                                .await
                                .map_err(|e| {
                                    state.metrics.record_governance_error(
                                        GovernanceRoute::ProvenanceLineage,
                                        ApiProblem::internal(anyhow::Error::from(e)),
                                    )
                                })?
                            {
                                events.push(child_event);
                                next_frontier.push(to_id.to_string());
                            }
                        }
                    }
                    edges.push(edge.clone());
                }
            }
        }

        frontier = next_frontier;
        next_frontier = Vec::new();
    }

    let response = LineageQueryResponse { events, edges };
    // I11: sanitize response to strip control characters from string fields
    let json_val = serde_json::to_value(&response).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ProvenanceLineage,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: LineageQueryResponse =
        serde_json::from_value(sanitized).map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineage,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    governance_ok!(
        state,
        GovernanceRoute::ProvenanceLineage,
        Ok(Json(sanitized_response))
    )
}

pub(crate) async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ExecutionDetailResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsExecutionId, e)
    })?;
    let record = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Look up the rollback contract if present, for fs-first rollback inspection.
    // This enables operators to inspect contract state, target path, before_hash,
    // after_hash, compensation_plan, and verify_checks via GET /v1/executions/{id}.
    let rollback_contract = if let Some(contract_id) = record.rollback_contract_id {
        match state
            .runtime
            .store
            .rollback_contracts()
            .get(contract_id)
            .await
        {
            Ok(contract) => contract,
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ExecutionsExecutionId,
                    ApiProblem::internal(anyhow::Error::from(e))
                );
            }
        }
    } else {
        None
    };

    let response = ExecutionDetailResponse {
        execution: record,
        rollback_contract,
    };
    // I11: sanitize response to strip control characters from string fields
    let json_val = match serde_json::to_value(&response) {
        Ok(val) => val,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: ExecutionDetailResponse = match serde_json::from_value(sanitized) {
        Ok(resp) => resp,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    governance_ok!(
        state,
        GovernanceRoute::ExecutionsExecutionId,
        Ok(Json(sanitized_response))
    )
}
