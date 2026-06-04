//! Intent governance handlers.
//!
//! Routes:
//! - `POST /v1/intents/compile` -> `compile_intent`
//! - `GET  /v1/intents`         -> `list_intents`
//!
//! `compile_intent` mints a fresh `IntentEnvelope` from an
//! `IntentCompileRequest`, validates it (I1 envelope validation), and
//! persists it synchronously to satisfy foreign-key constraints on the
//! `proposals`/`capabilities` tables.
//!
//! `list_intents` exposes a paginated, filterable read endpoint over
//! `intents_with_exec_state`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::{Duration, Utc};
use ferrum_proto::{
    ApiErrorCode, IntentCompileRequest, IntentCompileResponse, IntentEnvelope, IntentStatus,
    OutcomeClause, RiskTier, TimeBudget, TrustContextSummary,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::execution::infer_rollback_class;
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::state::AppState;

pub(crate) async fn compile_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IntentCompileRequest>,
) -> Result<Json<IntentCompileResponse>, ApiProblem> {
    let now = Utc::now();
    let requested_risk = req.requested_risk_tier.unwrap_or(RiskTier::Medium);
    let default_rollback_class = infer_rollback_class(&req.requested_resource_scope);

    let input_labels = req
        .raw_inputs
        .iter()
        .flat_map(|r| r.trust_labels.clone())
        .collect::<Vec<_>>();
    let sensitivity_labels = req
        .raw_inputs
        .iter()
        .flat_map(|r| r.sensitivity_labels.clone())
        .collect::<Vec<_>>();

    let envelope = IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: req.principal_id,
        session_id: req.session_id,
        channel_id: req.channel_id,
        title: req.title.clone(),
        goal: req.goal.clone(),
        normalized_goal: req.goal.trim().to_lowercase(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: req.requested_resource_scope,
        risk_tier: requested_risk,
        approval_mode: req
            .approval_mode
            .unwrap_or(ferrum_proto::ApprovalMode::None),
        default_rollback_class,
        time_budget: TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: TrustContextSummary {
            input_labels,
            sensitivity_labels,
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: req.raw_inputs.iter().filter_map(|r| r.event_id).collect(),
        tags: Vec::new(),
        metadata: req.metadata,
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + Duration::minutes(15),
    };

    // I1: Validate envelope before persisting.
    if let Err(msg) = envelope.validate() {
        return governance_err!(
            state,
            GovernanceRoute::IntentsCompile,
            ApiProblem::new(StatusCode::BAD_REQUEST, ApiErrorCode::ValidationError, msg,)
        );
    }

    // Persist the intent envelope so foreign-key constraints in proposals
    // and capabilities tables are satisfied.
    // Synchronous write: must complete before response to guarantee FK constraints.
    if let Err(e) = state.runtime.store.intents().insert(&envelope).await {
        tracing::warn!(error = %e, "failed to persist intent to DB");
        return governance_err!(
            state,
            GovernanceRoute::IntentsCompile,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::IntentsCompile,
        Ok(Json(IntentCompileResponse {
            envelope,
            warnings: Vec::new(),
        }))
    )
}

/// Query parameters for GET /v1/intents
#[derive(Debug, Deserialize)]
pub(crate) struct ListIntentsParams {
    #[serde(default)]
    intent_id: Option<String>,
    #[serde(default)]
    state: Vec<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default = "default_intent_list_limit")]
    limit: u32,
}

fn default_intent_list_limit() -> u32 {
    50
}

pub(crate) const MAX_INTENT_LIST_LIMIT: u32 = 200;

/// Response item for intent list
#[derive(Debug, serde::Serialize)]
pub(crate) struct IntentListItem {
    intent_id: String,
    principal_id: String,
    title: String,
    status: String,
    risk_tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_state: Option<String>,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
}

/// Response envelope for intent list
#[derive(Debug, serde::Serialize)]
pub(crate) struct IntentListEnvelope {
    items: Vec<IntentListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

pub(crate) async fn list_intents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListIntentsParams>,
) -> Result<Json<IntentListEnvelope>, ApiProblem> {
    // Validate and clamp limit
    let limit = if params.limit == 0 || params.limit > MAX_INTENT_LIST_LIMIT {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            format!("limit must be between 1 and {}", MAX_INTENT_LIST_LIMIT),
        ));
    } else {
        params.limit
    };

    // Parse intent_id filter if provided
    let intent_id = if let Some(ref id) = params.intent_id {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "invalid intent_id format",
            )
        })?;
        Some(ferrum_proto::IntentId(uuid))
    } else {
        None
    };

    // Parse status filters - convert string to IntentStatus
    let mut statuses = Vec::new();
    for s in &params.state {
        let status = match s.to_lowercase().as_str() {
            "active" => IntentStatus::Active,
            "closed" => IntentStatus::Closed,
            "expired" => IntentStatus::Expired,
            "quarantined" => IntentStatus::Quarantined,
            "revoked" => IntentStatus::Revoked,
            _ => {
                return Err(ApiProblem::new(
                    StatusCode::BAD_REQUEST,
                    ApiErrorCode::ValidationError,
                    format!("unknown intent status: {}", s),
                ));
            }
        };
        statuses.push(status);
    }

    // Query the store
    let (intents_with_state, next_cursor) = state
        .runtime
        .store
        .intents()
        .list_intents_with_exec_state(intent_id, &statuses, params.cursor.as_deref(), limit)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    let items: Vec<IntentListItem> = intents_with_state
        .into_iter()
        .map(|(intent, exec_state)| IntentListItem {
            intent_id: intent.intent_id.to_string(),
            principal_id: intent.principal_id.to_string(),
            title: intent.title.clone(),
            status: format!("{:?}", intent.status),
            risk_tier: format!("{:?}", intent.risk_tier),
            exec_state,
            created_at: intent.created_at.to_rfc3339(),
            expires_at: Some(intent.expires_at.to_rfc3339()),
        })
        .collect();

    governance_ok!(
        state,
        GovernanceRoute::IntentsList,
        Ok(Json(IntentListEnvelope { items, next_cursor }))
    )
}
