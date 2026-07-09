use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use ferrum_proto::{ApiErrorCode, EvaluateOutcomeResponse, OutcomeReport};

use crate::execution::parse_execution_id;
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::state::AppState;

/// `POST /v1/executions/{execution_id}/evaluate-outcome`
///
/// Validates that the path `execution_id` matches the report, loads the
/// execution and its intent, and delegates the alignment check to the PDP
/// `evaluate_outcome` engine.
pub(crate) async fn evaluate_outcome(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    Json(report): Json<OutcomeReport>,
) -> Result<Json<EvaluateOutcomeResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsEvaluateOutcome, e)
    })?;

    // Validate execution_id matches report
    if report.execution_id != execution_id {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsEvaluateOutcome,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "execution_id in path does not match report",
            )
        );
    }

    // Look up execution to get intent_id
    let execution = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                ),
            )
        })?;

    // Look up intent
    let intent = state
        .runtime
        .store
        .intents()
        .get(execution.intent_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for execution",
                ),
            )
        })?;

    let response = state
        .runtime
        .pdp
        .evaluate_outcome(&intent, &report)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(e),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsEvaluateOutcome,
        Ok(Json(response))
    )
}
