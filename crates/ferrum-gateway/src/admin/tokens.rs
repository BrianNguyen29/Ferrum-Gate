use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ferrum_proto::{
    ApiError, ApiErrorCode, AuditAction, AuditResourceType, CreateTokenRequest,
    CreateTokenResponse, RevokeTokenRequest, RotateTokenRequest, ScopedToken, ScopedTokenMeta,
    TokenListResponse,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    audit::append_audit,
    generate_token_salt, generate_token_value, hash_token_value, hash_token_with_salt,
    response::{sanitized_api_error_response, sanitized_response},
    state::AppState,
};

// ── Admin Token Handlers ──

pub(crate) async fn create_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTokenRequest>,
) -> Response {
    // Validate TTL <= 90 days
    let max_ttl = chrono::Duration::days(90);
    if req.expires_at > chrono::Utc::now() + max_ttl {
        let error = ApiError {
            code: ApiErrorCode::ValidationError,
            message: "expires_at exceeds maximum TTL of 90 days".to_string(),
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

    let scopes = req.scopes.unwrap_or_else(|| req.role.default_scopes());
    let token_value = generate_token_value();
    let token_salt = generate_token_salt();
    let token_lookup_hash = hash_token_value(&token_value);
    let token_hash = hash_token_with_salt(&token_value, &token_salt);
    let token_id = format!("tok_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    let token = ScopedToken {
        token_id: token_id.clone(),
        actor_id: req.actor_id,
        role: req.role,
        scopes,
        description: req.description,
        expires_at: req.expires_at,
        created_at: chrono::Utc::now(),
        last_used_at: None,
        revoked_at: None,
        revoked_reason: None,
        rotated_from: None,
        token_lookup_hash,
        token_hash,
        token_salt,
    };

    match state.runtime.store.tokens().insert(&token).await {
        Ok(()) => {
            // Audit log: token created
            append_audit(
                &state.runtime.store,
                &token.actor_id,
                AuditAction::TokenCreate,
                AuditResourceType::Token,
                &token_id,
                "success",
                Some(serde_json::json!({
                    "role": format!("{:?}", req.role),
                })),
            )
            .await;
            let meta: ScopedTokenMeta = token.into();
            let response = CreateTokenResponse {
                token: meta,
                token_value,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::CREATED, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "token insert failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to create token".to_string(),
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

pub(crate) async fn list_tokens(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListTokensQuery>,
) -> Response {
    let (tokens, next_cursor) = match state
        .runtime
        .store
        .tokens()
        .list(
            params.actor_id.as_deref(),
            params.role.as_deref(),
            params.active_only.unwrap_or(false),
            params.limit.unwrap_or(50).min(200),
            params.cursor.as_deref(),
        )
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(error = %e, "token list failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list tokens".to_string(),
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

    let items: Vec<ScopedTokenMeta> = tokens.into_iter().map(|t| t.into()).collect();
    let response = TokenListResponse {
        items,
        next_cursor,
        total: 0, // Not computed for performance; clients can infer from items + next_cursor
    };
    sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListTokensQuery {
    actor_id: Option<String>,
    role: Option<String>,
    active_only: Option<bool>,
    limit: Option<u32>,
    cursor: Option<String>,
}

pub(crate) async fn revoke_token(
    State(state): State<Arc<AppState>>,
    Path(token_id): Path<String>,
    Json(req): Json<RevokeTokenRequest>,
) -> Response {
    match state
        .runtime
        .store
        .tokens()
        .revoke(&token_id, req.reason.as_deref())
        .await
    {
        Ok(true) => {
            // Audit log: token revoked
            append_audit(
                &state.runtime.store,
                "unknown",
                AuditAction::TokenRevoke,
                AuditResourceType::Token,
                &token_id,
                "success",
                Some(serde_json::json!({
                    "reason": req.reason,
                })),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => {
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "token not found or already revoked".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(&state.runtime.firewall, StatusCode::NOT_FOUND, &error)
        }
        Err(e) => {
            tracing::error!(error = %e, "token revoke failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to revoke token".to_string(),
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

pub(crate) async fn rotate_token(
    State(state): State<Arc<AppState>>,
    Path(token_id): Path<String>,
    Json(req): Json<RotateTokenRequest>,
) -> Response {
    // Get the old token
    let old_token = match state.runtime.store.tokens().get(&token_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "token not found".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::NOT_FOUND,
                &error,
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "token get failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to rotate token".to_string(),
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

    if old_token.revoked_at.is_some() || old_token.expires_at < chrono::Utc::now() {
        let error = ApiError {
            code: ApiErrorCode::ValidationError,
            message: "token is already revoked or expired".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(&state.runtime.firewall, StatusCode::CONFLICT, &error);
    }

    // Validate TTL <= 90 days when an explicit expiry is requested
    let max_ttl = chrono::Duration::days(90);
    let expires_at = req
        .expires_at
        .unwrap_or_else(|| chrono::Utc::now() + max_ttl);
    if let Some(requested_expires) = req.expires_at {
        if requested_expires > chrono::Utc::now() + max_ttl {
            let error = ApiError {
                code: ApiErrorCode::ValidationError,
                message: "expires_at exceeds maximum TTL of 90 days".to_string(),
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
    }

    // Revoke the old token
    let _ = state
        .runtime
        .store
        .tokens()
        .revoke(&token_id, req.reason.as_deref())
        .await;

    // Create new token with same actor/role/scopes
    let new_token_value = generate_token_value();
    let new_token_salt = generate_token_salt();
    let new_token_lookup_hash = hash_token_value(&new_token_value);
    let new_token_hash = hash_token_with_salt(&new_token_value, &new_token_salt);
    let new_token_id = format!("tok_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    let new_token = ScopedToken {
        token_id: new_token_id.clone(),
        actor_id: old_token.actor_id,
        role: old_token.role,
        scopes: old_token.scopes,
        description: old_token.description,
        expires_at,
        created_at: chrono::Utc::now(),
        last_used_at: None,
        revoked_at: None,
        revoked_reason: None,
        rotated_from: Some(token_id.clone()),
        token_lookup_hash: new_token_lookup_hash,
        token_hash: new_token_hash,
        token_salt: new_token_salt,
    };

    match state.runtime.store.tokens().insert(&new_token).await {
        Ok(()) => {
            // Audit log: token rotated
            append_audit(
                &state.runtime.store,
                &new_token.actor_id,
                AuditAction::TokenRotate,
                AuditResourceType::Token,
                &new_token_id,
                "success",
                Some(serde_json::json!({
                    "old_token_id": token_id,
                    "reason": req.reason,
                })),
            )
            .await;
            let meta: ScopedTokenMeta = new_token.into();
            let response = CreateTokenResponse {
                token: meta,
                token_value: new_token_value,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::CREATED, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "token rotate insert failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to rotate token".to_string(),
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
