use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use ferrum_proto::{
    ApiError, ApiErrorCode, AuditAction, AuditResourceType, MfaCredentialRecord,
    MfaDisableResponse, MfaEnrollResponse, MfaFactorListResponse, MfaFactorSummary, MfaFactorType,
    MfaRotateResponse, MfaVerifyRequest, MfaVerifyResponse,
};
use std::sync::Arc;

use crate::{
    audit, mfa,
    monitoring::GovernanceRoute,
    response::{sanitized_api_error_response, sanitized_response},
    state::AppState,
};

/// Resolve and decode the MFA secret key from server config.
///
/// Returns the 32-byte key on success, or an HTTP `Response` with
/// `Misconfigured` on failure. Used by enroll, verify, and rotate
/// to keep the lookup/decode block identical and fail-closed.
#[allow(clippy::result_large_err)]
fn resolve_mfa_key(state: &AppState) -> Result<Vec<u8>, Response> {
    let key_hex = match &state.server_config.mfa_secret_key {
        Some(k) => k,
        None => {
            let error = ApiError {
                code: ApiErrorCode::Misconfigured,
                message: "mfa_secret_key is not configured".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return Err(sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::FORBIDDEN,
                &error,
            ));
        }
    };

    match mfa::decode_hex_key(key_hex) {
        Ok(b) => Ok(b),
        Err(msg) => {
            let error = ApiError {
                code: ApiErrorCode::Misconfigured,
                message: msg,
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            Err(sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::FORBIDDEN,
                &error,
            ))
        }
    }
}

// ── Admin MFA Handlers ──

pub(crate) async fn enroll_mfa(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Response {
    let key_bytes = match resolve_mfa_key(&state) {
        Ok(b) => b,
        Err(response) => return response,
    };

    let secret = mfa::generate_totp_secret();
    let (encrypted_secret, nonce) = match mfa::encrypt_secret(&key_bytes, &secret) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "mfa encrypt_secret failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaEnroll);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to encrypt MFA secret".to_string(),
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

    let record = MfaCredentialRecord::new(
        &agent_id,
        MfaFactorType::Totp,
        encrypted_secret,
        nonce,
        "default",
    );

    let mfa_factor_id = record.mfa_factor_id;
    let issuer = state.server_config.mfa_totp_issuer.clone();
    let otpauth_uri = mfa::build_otpauth_uri(&secret, &issuer, &agent_id);

    match state.runtime.store.mfa_credentials().insert(&record).await {
        Ok(()) => {
            state
                .metrics
                .increment_governance_success(GovernanceRoute::MfaEnroll);
            if let Err(problem) = audit::append_audit_checked(
                &state,
                "unknown",
                AuditAction::MfaEnroll,
                AuditResourceType::MfaCredential,
                &mfa_factor_id.to_string(),
                "success",
                Some(serde_json::json!({
                    "agent_id": agent_id,
                })),
                Some(GovernanceRoute::MfaEnroll),
            )
            .await
            {
                return axum::response::IntoResponse::into_response(problem);
            }
            let response = MfaEnrollResponse {
                mfa_factor_id,
                otpauth_uri,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::CREATED, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "mfa credential insert failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaEnroll);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to enroll MFA factor".to_string(),
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

pub(crate) async fn verify_mfa(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(req): Json<MfaVerifyRequest>,
) -> Response {
    let key_bytes = match resolve_mfa_key(&state) {
        Ok(b) => b,
        Err(response) => return response,
    };

    // Find the most recent pending factor for the agent
    let records = match state
        .runtime
        .store
        .mfa_credentials()
        .list_by_agent(&agent_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "mfa list_by_agent failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list MFA factors".to_string(),
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

    let pending = records
        .into_iter()
        .find(|r| r.status == ferrum_proto::MfaFactorStatus::Pending && r.revoked_at.is_none());

    let record = match pending {
        Some(r) => r,
        None => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "no pending MFA factor found for agent".to_string(),
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
    };

    let secret =
        match mfa::decrypt_secret(&key_bytes, &record.encrypted_secret, &record.secret_nonce) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "mfa decrypt_secret failed");
                state
                    .metrics
                    .increment_governance_error(GovernanceRoute::MfaVerify);
                let error = ApiError {
                    code: ApiErrorCode::Internal,
                    message: "failed to decrypt MFA secret".to_string(),
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

    let now = chrono::Utc::now().timestamp() as u64;
    let matched_counter = match mfa::verify_totp_code_with_counter(&secret, &req.code, now) {
        Ok(c) => c,
        Err(_) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::MfaInvalid,
                message: "invalid TOTP code".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::FORBIDDEN,
                &error,
            );
        }
    };

    // Record successful use with counter for replay protection (CAS in DB)
    match state
        .runtime
        .store
        .mfa_credentials()
        .record_use(record.mfa_factor_id, matched_counter)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::MfaInvalid,
                message: "invalid TOTP code".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::FORBIDDEN,
                &error,
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "mfa record_use failed during verify");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "MFA state update failed".to_string(),
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
        .mfa_credentials()
        .activate(record.mfa_factor_id)
        .await
    {
        Ok(true) => {
            state
                .metrics
                .increment_governance_success(GovernanceRoute::MfaVerify);
            if let Err(problem) = audit::append_audit_checked(
                &state,
                "unknown",
                AuditAction::MfaVerify,
                AuditResourceType::MfaCredential,
                &record.mfa_factor_id.to_string(),
                "success",
                Some(serde_json::json!({
                    "agent_id": agent_id,
                })),
                Some(GovernanceRoute::MfaVerify),
            )
            .await
            {
                return axum::response::IntoResponse::into_response(problem);
            }
            let response = MfaVerifyResponse { verified: true };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Ok(false) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::Conflict,
                message: "factor was not pending or already activated".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(&state.runtime.firewall, StatusCode::CONFLICT, &error)
        }
        Err(e) => {
            tracing::error!(error = %e, "mfa activate failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaVerify);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to activate MFA factor".to_string(),
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

pub(crate) async fn disable_mfa(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Response {
    let active = match state
        .runtime
        .store
        .mfa_credentials()
        .get_active_for_agent(&agent_id)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaDisable);
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "no active MFA factor found for agent".to_string(),
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
            tracing::error!(error = %e, "mfa get_active_for_agent failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaDisable);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to retrieve active MFA factor".to_string(),
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

    match state
        .runtime
        .store
        .mfa_credentials()
        .revoke(active.mfa_factor_id)
        .await
    {
        Ok(true) => {
            state
                .metrics
                .increment_governance_success(GovernanceRoute::MfaDisable);
            if let Err(problem) = audit::append_audit_checked(
                &state,
                "unknown",
                AuditAction::MfaDisable,
                AuditResourceType::MfaCredential,
                &active.mfa_factor_id.to_string(),
                "success",
                Some(serde_json::json!({
                    "agent_id": agent_id,
                })),
                Some(GovernanceRoute::MfaDisable),
            )
            .await
            {
                return axum::response::IntoResponse::into_response(problem);
            }
            let response = MfaDisableResponse { disabled: true };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Ok(false) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaDisable);
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "factor not found or already revoked".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(&state.runtime.firewall, StatusCode::NOT_FOUND, &error)
        }
        Err(e) => {
            tracing::error!(error = %e, "mfa revoke failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaDisable);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to disable MFA factor".to_string(),
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

pub(crate) async fn rotate_mfa(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Response {
    let key_bytes = match resolve_mfa_key(&state) {
        Ok(b) => b,
        Err(response) => return response,
    };

    // Revoke existing active factor
    let active = match state
        .runtime
        .store
        .mfa_credentials()
        .get_active_for_agent(&agent_id)
        .await
    {
        Ok(Some(r)) => Some(r),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(error = %e, "mfa get_active_for_agent failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaRotate);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to retrieve active MFA factor".to_string(),
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

    if let Some(ref active) = active {
        if let Err(e) = state
            .runtime
            .store
            .mfa_credentials()
            .revoke(active.mfa_factor_id)
            .await
        {
            tracing::error!(error = %e, "mfa revoke during rotate failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaRotate);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to revoke existing MFA factor".to_string(),
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

    // Create new pending factor
    let secret = mfa::generate_totp_secret();
    let (encrypted_secret, nonce) = match mfa::encrypt_secret(&key_bytes, &secret) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "mfa encrypt_secret during rotate failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaRotate);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to encrypt MFA secret".to_string(),
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

    let record = MfaCredentialRecord::new(
        &agent_id,
        MfaFactorType::Totp,
        encrypted_secret,
        nonce,
        "default",
    );
    let mfa_factor_id = record.mfa_factor_id;
    let issuer = state.server_config.mfa_totp_issuer.clone();
    let otpauth_uri = mfa::build_otpauth_uri(&secret, &issuer, &agent_id);

    if let Err(e) = state.runtime.store.mfa_credentials().insert(&record).await {
        tracing::error!(error = %e, "mfa credential insert during rotate failed");
        state
            .metrics
            .increment_governance_error(GovernanceRoute::MfaRotate);
        let error = ApiError {
            code: ApiErrorCode::Internal,
            message: "failed to create new MFA factor".to_string(),
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

    state
        .metrics
        .increment_governance_success(GovernanceRoute::MfaRotate);
    if let Err(problem) = audit::append_audit_checked(
        &state,
        "unknown",
        AuditAction::MfaRotate,
        AuditResourceType::MfaCredential,
        &mfa_factor_id.to_string(),
        "success",
        Some(serde_json::json!({
            "agent_id": agent_id,
            "previous_factor_id": active.as_ref().map(|a| a.mfa_factor_id.to_string()),
        })),
        Some(GovernanceRoute::MfaRotate),
    )
    .await
    {
        return axum::response::IntoResponse::into_response(problem);
    }

    let response = MfaRotateResponse {
        mfa_factor_id,
        otpauth_uri,
    };
    sanitized_response(&state.runtime.firewall, StatusCode::CREATED, &response)
}

pub(crate) async fn list_mfa_factors(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Response {
    let records = match state
        .runtime
        .store
        .mfa_credentials()
        .list_by_agent(&agent_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "mfa list_by_agent failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaList);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list MFA factors".to_string(),
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

    let items: Vec<MfaFactorSummary> = records
        .into_iter()
        .map(|r| MfaFactorSummary {
            mfa_factor_id: r.mfa_factor_id,
            factor_type: r.factor_type,
            status: r.status,
            label: r.label,
            created_at: r.created_at,
            verified_at: r.verified_at,
            last_used_at: r.last_used_at,
            last_used_counter: r.last_used_counter,
            revoked_at: r.revoked_at,
        })
        .collect();

    let total = items.len();
    state
        .metrics
        .increment_governance_success(GovernanceRoute::MfaList);
    let response = MfaFactorListResponse { items, total };
    sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
}

pub(crate) async fn get_mfa_factor(
    State(state): State<Arc<AppState>>,
    Path((agent_id, mfa_factor_id)): Path<(String, ferrum_proto::MfaFactorId)>,
) -> Response {
    let record = match state
        .runtime
        .store
        .mfa_credentials()
        .get(mfa_factor_id)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaGet);
            let error = ApiError {
                code: ApiErrorCode::NotFound,
                message: "MFA factor not found".to_string(),
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
            tracing::error!(error = %e, "mfa get failed");
            state
                .metrics
                .increment_governance_error(GovernanceRoute::MfaGet);
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to retrieve MFA factor".to_string(),
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

    if record.agent_id != agent_id {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::MfaGet);
        let error = ApiError {
            code: ApiErrorCode::NotFound,
            message: "MFA factor not found for agent".to_string(),
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

    state
        .metrics
        .increment_governance_success(GovernanceRoute::MfaGet);
    let summary = MfaFactorSummary {
        mfa_factor_id: record.mfa_factor_id,
        factor_type: record.factor_type,
        status: record.status,
        label: record.label,
        created_at: record.created_at,
        verified_at: record.verified_at,
        last_used_at: record.last_used_at,
        last_used_counter: record.last_used_counter,
        revoked_at: record.revoked_at,
    };
    sanitized_response(&state.runtime.firewall, StatusCode::OK, &summary)
}
