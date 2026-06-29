use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine;
use ferrum_proto::{
    AgentListResponse, ApiError, ApiErrorCode, AuditAction, AuditResourceType,
    RegisterAgentRequest, RegisterAgentResponse, RevokeAgentRequest,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    audit,
    auth_actor::{AuthActor, audit_actor},
    monitoring::GovernanceRoute,
    response::{sanitized_api_error_response, sanitized_response},
    state::AppState,
};

// ── Admin Agent Handlers ──

pub(crate) async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterAgentRequest>,
) -> Response {
    // Validate public key is valid base64 and decodes to 32 bytes
    let pk_bytes = match base64::engine::general_purpose::STANDARD.decode(&req.public_key) {
        Ok(b) => b,
        Err(_) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsCreate);
            let error = ApiError {
                code: ApiErrorCode::ValidationError,
                message: "invalid public_key: must be valid base64".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::BAD_REQUEST,
                &error,
            );
        }
    };
    if pk_bytes.len() != 32 {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::AgentsCreate);
        let error = ApiError {
            code: ApiErrorCode::ValidationError,
            message: "invalid public_key: must decode to 32 bytes".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(
            &state.runtime.firewall,
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    let fingerprint = {
        use sha2::Digest;
        let hash = sha2::Sha256::digest(&pk_bytes);
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, hash)
    };

    // Pre-check duplicates to return tailored errors instead of raw DB constraint violations.
    match state.runtime.store.agents().get(&req.agent_id).await {
        Ok(Some(_)) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsCreate);
            let error = ApiError {
                code: ApiErrorCode::Conflict,
                message: format!("agent_id '{}' already exists", req.agent_id),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::CONFLICT,
                &error,
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "agent duplicate check failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsCreate);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to register agent".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            );
        }
    }

    match state
        .runtime
        .store
        .agents()
        .get_by_fingerprint(&fingerprint)
        .await
    {
        Ok(Some(existing)) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsCreate);
            let error = ApiError {
                code: ApiErrorCode::Conflict,
                message: format!(
                    "public_key already registered for agent '{}'",
                    existing.agent_id
                ),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::CONFLICT,
                &error,
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "agent fingerprint check failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsCreate);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to register agent".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            );
        }
    }

    let allowed_scopes = req.scopes.unwrap_or_else(|| {
        vec![
            "intent:submit".to_string(),
            "proposal:evaluate".to_string(),
            "capability:mint".to_string(),
            "execution:authorize".to_string(),
            "execution:prepare".to_string(),
            "execution:execute".to_string(),
            "execution:verify".to_string(),
            "execution:compensate".to_string(),
        ]
    });

    let agent = ferrum_proto::AgentRecord {
        agent_id: req.agent_id.clone(),
        public_key: req.public_key,
        key_fingerprint: fingerprint.clone(),
        allowed_scopes,
        created_at: chrono::Utc::now(),
        revoked_at: None,
        description: req.description,
    };

    match state.runtime.store.agents().insert(&agent).await {
        Ok(()) => {
            state
                .metrics
                .increment_governance_success(GovernanceRoute::AgentsCreate);
            if let Err(problem) = audit::append_audit_checked(
                &state,
                &req.agent_id,
                AuditAction::AgentRegister,
                AuditResourceType::Agent,
                &req.agent_id,
                "success",
                Some(serde_json::json!({
                    "fingerprint": fingerprint,
                })),
                Some(GovernanceRoute::AgentsCreate),
            )
            .await
            {
                return axum::response::IntoResponse::into_response(problem);
            }
            let response = RegisterAgentResponse { agent };
            sanitized_response(&state.runtime.firewall, StatusCode::CREATED, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "agent insert failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsCreate);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to register agent".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListAgentsQuery {
    active_only: Option<bool>,
    limit: Option<u32>,
    cursor: Option<String>,
}

pub(crate) async fn list_agents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListAgentsQuery>,
) -> Response {
    let active_only = params.active_only.unwrap_or(false);
    let limit = params.limit.unwrap_or(50).min(200);
    let (agents, next_cursor) = match state
        .runtime
        .store
        .agents()
        .list(active_only, limit, params.cursor.as_deref())
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(error = %e, "agent list failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsList);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list agents".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            );
        }
    };

    let total = match state.runtime.store.agents().count(active_only).await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %e, "agent count failed; returning items.len() as total");
            agents.len()
        }
    };

    state
        .metrics
        .increment_governance_success(GovernanceRoute::AgentsList);
    let response = AgentListResponse {
        items: agents,
        next_cursor,
        total,
    };
    sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
}

pub(crate) async fn revoke_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    auth_actor: Option<Extension<AuthActor>>,
    Json(req): Json<RevokeAgentRequest>,
) -> Response {
    match state.runtime.store.agents().revoke(&agent_id).await {
        Ok(true) => {
            state
                .metrics
                .increment_governance_success(GovernanceRoute::AgentsRevoke);
            if let Err(problem) = audit::append_audit_checked(
                &state,
                audit_actor(auth_actor.as_deref()),
                AuditAction::AgentRevoke,
                AuditResourceType::Agent,
                &agent_id,
                "success",
                Some(serde_json::json!({
                    "reason": req.reason,
                })),
                Some(GovernanceRoute::AgentsRevoke),
            )
            .await
            {
                return axum::response::IntoResponse::into_response(problem);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsRevoke);
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "agent not found or already revoked".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(&state.runtime.firewall, StatusCode::NOT_FOUND, &error)
        }
        Err(e) => {
            tracing::error!(error = %e, "agent revoke failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::AgentsRevoke);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to revoke agent".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            )
        }
    }
}
