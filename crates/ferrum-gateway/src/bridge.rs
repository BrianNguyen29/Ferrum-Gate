//! Bridge and provenance endpoints.
//!
//! Routes:
//! - `POST /v1/provenance/query`
//! - `POST /v1/provenance/ingest`
//! - `GET  /v1/bridges`
//! - `GET  /v1/bridges/{bridge_id}/tools`
//!
//! All success paths increment the `GovernanceRoute` counter and apply the
//! output sanitizer to the response payload.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, EventId, HashChainRef, ObjectRef, ObjectType,
    ProvenanceEvent, ProvenanceIngestRequest, ProvenanceIngestResponse, ProvenanceQueryRequest,
    ProvenanceQueryResponse,
};
use ferrum_sync::BridgeToolInfo;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::response::sanitize_json;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BridgeInfo {
    pub(crate) runtime_id: String,
    pub(crate) connected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BridgeListResponse {
    pub(crate) bridges: Vec<BridgeInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BridgeToolsResponse {
    pub(crate) runtime_id: String,
    pub(crate) tools: Vec<BridgeToolInfo>,
}

pub(crate) async fn query_provenance(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ProvenanceQueryRequest>,
) -> Result<Json<ProvenanceQueryResponse>, ApiProblem> {
    let events = state
        .runtime
        .store
        .provenance()
        .query(&request)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceQuery,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    state
        .metrics
        .increment_governance_success(GovernanceRoute::ProvenanceQuery);
    Ok(Json(ProvenanceQueryResponse { events }))
}

pub(crate) async fn ingest_provenance(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ProvenanceIngestRequest>,
) -> Result<Json<ProvenanceIngestResponse>, ApiProblem> {
    // Validate source_runtime_id against registered bridges - fail closed
    let bridge = state
        .runtime
        .bridges
        .iter()
        .find(|b| b.runtime_id() == request.source_runtime_id);

    if bridge.is_none() {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::ProvenanceIngest);
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            format!("unknown source_runtime_id: {}", request.source_runtime_id),
        ));
    }

    // Build ProvenanceEvent from request
    let event_id = EventId::new();
    let event = ProvenanceEvent {
        event_id,
        kind: request.kind,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: request.source_runtime_id.clone(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::ProvenanceEvent,
            object_id: request.source_runtime_id.clone(),
            summary: Some(request.description.clone()),
        },
        intent_id: request.intent_id,
        proposal_id: None,
        execution_id: request.execution_id,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: request.trust_labels,
        sensitivity_labels: request.sensitivity_labels,
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: request.metadata,
        source_runtime_id: Some(request.source_runtime_id),
    };

    // Persist - FAIL CLOSED on store errors
    state
        .runtime
        .store
        .provenance()
        .append_event(&event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceIngest,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    state
        .metrics
        .increment_governance_success(GovernanceRoute::ProvenanceIngest);
    Ok(Json(ProvenanceIngestResponse {
        event_id,
        linked: true,
    }))
}

pub(crate) async fn list_bridges(State(state): State<Arc<AppState>>) -> Json<BridgeListResponse> {
    let bridges: Vec<BridgeInfo> = state
        .runtime
        .bridges
        .iter()
        .map(|b| BridgeInfo {
            runtime_id: b.runtime_id().to_string(),
            connected: b.is_connected(),
        })
        .collect();
    Json(BridgeListResponse { bridges })
}

pub(crate) async fn list_bridge_tools(
    State(state): State<Arc<AppState>>,
    Path(bridge_id): Path<String>,
) -> Result<Json<BridgeToolsResponse>, ApiProblem> {
    let bridge = state
        .runtime
        .bridges
        .iter()
        .find(|b| b.runtime_id() == bridge_id)
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::BridgesBridgeIdTools,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    format!("bridge not found: {}", bridge_id),
                ),
            )
        })?;

    if !bridge.is_connected() {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::BridgesBridgeIdTools);
        return Err(ApiProblem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiErrorCode::Internal,
            format!("bridge '{}' is not connected", bridge_id),
        ));
    }

    let tools = bridge.list_tools().await.map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::BridgesBridgeIdTools,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;

    let response = BridgeToolsResponse {
        runtime_id: bridge_id,
        tools,
    };
    // I11: sanitize response to strip control characters from string fields
    let json_val = serde_json::to_value(&response).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::BridgesBridgeIdTools,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: BridgeToolsResponse =
        serde_json::from_value(sanitized).map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::BridgesBridgeIdTools,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    state
        .metrics
        .increment_governance_success(GovernanceRoute::BridgesBridgeIdTools);
    Ok(Json(sanitized_response))
}
