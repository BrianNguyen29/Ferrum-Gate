use std::sync::Arc;
use std::time::Duration as StdDuration;

use crate::AuthActor;
use crate::AuthMode;
use crate::OidcConfig;
use crate::OidcJwksCache;
use crate::audit::append_audit;
use crate::server::{hash_token_value, hash_token_with_salt};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use ed25519_dalek::Verifier;
use ferrum_proto::{ApiError, ApiErrorCode, AuditAction, AuditResourceType};

/// Authentication middleware supporting Bearer, Scoped, OIDC, and Agent modes.
pub(crate) async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().as_str().to_string();

    // Keep only shallow health/readiness public. Deep readiness and metrics expose
    // operational detail and require auth whenever auth is enabled.
    if path == "/v1/healthz" || path == "/v1/readyz" {
        return next.run(request).await;
    }

    let config = &state.server_config;

    match config.auth_mode {
        AuthMode::Disabled => next.run(request).await,
        AuthMode::Bearer => {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());
            let Some(header) = auth_header else {
                return auth_error("missing authorization header");
            };
            if !header.starts_with("Bearer ") {
                return auth_error("invalid authorization header format");
            }
            let provided = &header[7..];
            let token = config.bearer_token.as_deref().unwrap_or("");
            if constant_time_eq::constant_time_eq(provided.as_bytes(), token.as_bytes()) {
                next.run(request).await
            } else {
                auth_error("invalid bearer token")
            }
        }
        AuthMode::Oidc => {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());
            let Some(header) = auth_header else {
                return auth_error("missing authorization header");
            };
            if !header.starts_with("Bearer ") {
                return auth_error("invalid authorization header format");
            }
            let provided = &header[7..];
            let oidc = match &config.oidc_config {
                Some(c) => c,
                None => {
                    tracing::error!("oidc config missing");
                    let _ = append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AuthFailed,
                        AuditResourceType::Auth,
                        "oidc",
                        "oidc auth misconfigured",
                        Some(serde_json::json!({"reason": "oidc config missing"})),
                    )
                    .await;
                    return auth_error("oidc auth misconfigured");
                }
            };
            match validate_oidc_token(provided, oidc, state.jwks_cache.as_ref(), &method, &path)
                .await
            {
                Ok((actor_id, scopes)) => {
                    let (mut parts, body) = request.into_parts();
                    parts.extensions.insert(AuthActor {
                        actor_id,
                        source: "oidc",
                        scopes,
                    });
                    let request = axum::http::Request::from_parts(parts, body);
                    next.run(request).await
                }
                Err(OidcAuthError::Unauthorized(msg)) => {
                    let _ = append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AuthFailed,
                        AuditResourceType::Auth,
                        "oidc",
                        "unauthorized",
                        Some(serde_json::json!({"reason": msg})),
                    )
                    .await;
                    auth_error(&msg)
                }
                Err(OidcAuthError::Forbidden(msg)) => {
                    let _ = append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AuthFailed,
                        AuditResourceType::Auth,
                        "oidc",
                        "forbidden",
                        Some(serde_json::json!({"reason": msg})),
                    )
                    .await;
                    (StatusCode::FORBIDDEN, msg).into_response()
                }
            }
        }
        AuthMode::Scoped => {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());
            let Some(header) = auth_header else {
                return auth_error("missing authorization header");
            };
            if !header.starts_with("Bearer ") {
                return auth_error("invalid authorization header format");
            }
            let provided = &header[7..];
            // Step 1: deterministic lookup hash (fast DB lookup)
            let lookup_hash = hash_token_value(provided);
            let token_repo = state.runtime.store.tokens();
            let token = match token_repo.get_by_lookup_hash(&lookup_hash).await {
                Ok(Some(t)) => t,
                Ok(None) => return auth_error("invalid scoped token"),
                Err(e) => {
                    tracing::error!(error = %e, "token lookup failed");
                    return auth_error("token lookup failed");
                }
            };

            // Step 2: verify presented token against secure salted hash
            let expected_hash = hash_token_with_salt(provided, &token.token_salt);
            if !constant_time_eq::constant_time_eq(
                expected_hash.as_bytes(),
                token.token_hash.as_bytes(),
            ) {
                return auth_error("invalid scoped token");
            }

            // Check revocation
            if token.revoked_at.is_some() {
                return auth_error("token revoked");
            }

            // Check expiration
            if token.expires_at < chrono::Utc::now() {
                return auth_error("token expired");
            }

            // Check scope
            let required_scope = required_scope_for_path(&method, &path);
            if let Some(scope) = required_scope {
                if !token_has_scope(&token, scope) {
                    return forbidden_error(&format!("required scope {}", scope));
                }
            }

            // Update last_used_at (best-effort, fire-and-forget)
            let token_id = token.token_id.clone();
            tokio::spawn(async move {
                let _ = token_repo.touch(&token_id).await;
            });

            // Insert authenticated actor identity for downstream handlers
            let (mut parts, body) = request.into_parts();
            parts.extensions.insert(AuthActor {
                actor_id: token.actor_id.clone(),
                source: "scoped",
                scopes: token.scopes.clone(),
            });
            let request = axum::http::Request::from_parts(parts, body);

            next.run(request).await
        }
        AuthMode::Agent => {
            match verify_agent_request(&state, request, next, &method, &path).await {
                Ok(response) => response,
                Err(AgentAuthError::Unauthorized(msg)) => {
                    let _ = append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AgentAuthFailed,
                        AuditResourceType::Auth,
                        "agent",
                        "unauthorized",
                        Some(serde_json::json!({"reason": msg})),
                    )
                    .await;
                    auth_error(&msg)
                }
                Err(AgentAuthError::Forbidden(msg)) => forbidden_error(&msg),
            }
        }
    }
}

/// Error type for Agent auth failures.
enum AgentAuthError {
    Unauthorized(String),
    Forbidden(String),
}

/// Verify an Ed25519-signed agent request.
///
/// Flow:
/// 1. Extract required headers.
/// 2. Verify timestamp skew.
/// 3. Check nonce replay cache.
/// 4. Recompute body hash and compare.
/// 5. Look up agent and check revocation.
/// 6. Verify Ed25519 signature over canonical payload.
/// 7. Enforce route scope.
async fn verify_agent_request(
    state: &AppState,
    request: Request,
    next: Next,
    method: &str,
    path: &str,
) -> Result<Response, AgentAuthError> {
    let headers = request.headers().clone();
    let agent_id = headers
        .get("X-Ferrum-Agent-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Agent-Id".to_string()))?;
    let timestamp = headers
        .get("X-Ferrum-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Timestamp".to_string()))?;
    let nonce = headers
        .get("X-Ferrum-Nonce")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Nonce".to_string()))?;
    let body_hash_header = headers
        .get("X-Ferrum-Body-Hash")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Body-Hash".to_string()))?;
    let signature_b64 = headers
        .get("X-Ferrum-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Signature".to_string()))?;

    // Verify timestamp
    let ts = chrono::DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| AgentAuthError::Unauthorized("invalid timestamp format".to_string()))?
        .with_timezone(&chrono::Utc);
    let now = chrono::Utc::now();
    let skew = chrono::Duration::seconds(state.server_config.agent_clock_skew_secs);
    if ts < now - skew || ts > now + skew {
        return Err(AgentAuthError::Unauthorized(
            "timestamp out of skew window".to_string(),
        ));
    }

    // Verify nonce (replay protection)
    if nonce.chars().count() > 256 {
        return Err(AgentAuthError::Unauthorized(
            "nonce exceeds maximum length".to_string(),
        ));
    }
    let nonce_ttl = if state.server_config.nonce_cache_ttl_secs > 0 {
        StdDuration::from_secs(state.server_config.nonce_cache_ttl_secs)
    } else {
        StdDuration::from_secs((state.server_config.agent_clock_skew_secs * 2).max(60) as u64)
    };
    match state.nonce_cache.check_and_insert(nonce, nonce_ttl).await {
        Ok(true) => {}
        Ok(false) => {
            return Err(AgentAuthError::Unauthorized("replayed nonce".to_string()));
        }
        Err(error) => {
            tracing::error!(%error, "nonce cache check failed");
            return Err(AgentAuthError::Unauthorized(
                "nonce cache unavailable".to_string(),
            ));
        }
    }

    // Read body and verify body hash
    let (mut parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024)
        .await
        .map_err(|_| AgentAuthError::Unauthorized("failed to read body".to_string()))?;
    let computed_body_hash = if bytes.is_empty() {
        "null".to_string()
    } else {
        blake3::hash(&bytes).to_hex().to_string()
    };
    if computed_body_hash != body_hash_header {
        return Err(AgentAuthError::Unauthorized(
            "body hash mismatch".to_string(),
        ));
    }

    // Look up agent
    let agent = match state.runtime.store.agents().get(agent_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return Err(AgentAuthError::Unauthorized("agent not found".to_string()));
        }
        Err(e) => {
            tracing::error!(error = %e, "agent lookup failed");
            return Err(AgentAuthError::Unauthorized(
                "agent lookup failed".to_string(),
            ));
        }
    };

    if agent.revoked_at.is_some() {
        return Err(AgentAuthError::Unauthorized("agent revoked".to_string()));
    }

    // Canonical payload
    let payload = format!(
        "{}:{}:{}:{}:{}:{}",
        agent_id, timestamp, nonce, body_hash_header, method, path
    );

    // Decode and verify signature
    let sig_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature_b64)
            .map_err(|_| AgentAuthError::Unauthorized("invalid signature encoding".to_string()))?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| AgentAuthError::Unauthorized("invalid signature length".to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    let pk_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &agent.public_key,
    )
    .map_err(|_| AgentAuthError::Unauthorized("invalid public key encoding".to_string()))?;
    let pk_array: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| AgentAuthError::Unauthorized("invalid public key length".to_string()))?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pk_array)
        .map_err(|_| AgentAuthError::Unauthorized("invalid public key".to_string()))?;

    verifying_key
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| AgentAuthError::Unauthorized("signature verification failed".to_string()))?;

    // Scope enforcement
    if let Some(required) = required_scope_for_path(method, path) {
        let has_scope = agent
            .allowed_scopes
            .iter()
            .any(|s| s == "*" || s == required);
        if !has_scope {
            return Err(AgentAuthError::Forbidden(format!(
                "required scope {}",
                required
            )));
        }
    }

    // Reconstruct request and proceed
    parts.extensions.insert(AuthActor {
        actor_id: agent_id.to_string(),
        source: "agent",
        scopes: agent.allowed_scopes.clone(),
    });
    let request = axum::http::Request::from_parts(parts, axum::body::Body::from(bytes));
    Ok(next.run(request).await)
}

fn auth_error(message: &str) -> Response {
    let error = ApiError {
        code: ApiErrorCode::Unauthorized,
        message: message.to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        retriable: false,
        details: serde_json::json!({}),
    };
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

fn forbidden_error(message: &str) -> Response {
    let error = ApiError {
        code: ApiErrorCode::Forbidden,
        message: message.to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        retriable: false,
        details: serde_json::json!({}),
    };
    (StatusCode::FORBIDDEN, Json(error)).into_response()
}

/// Check if a token has a given scope (or wildcard).
fn token_has_scope(token: &ferrum_proto::ScopedToken, scope: &str) -> bool {
    token.scopes.iter().any(|s| s == "*" || s == scope)
}

/// Map HTTP method + path to required scope.
pub(crate) fn required_scope_for_path(method: &str, path: &str) -> Option<&'static str> {
    // Public endpoints (no scope required) are handled before this is called
    match (method, path) {
        // Intent and proposal
        ("POST", "/v1/intents/compile") => Some("intent:submit"),
        ("GET", "/v1/intents") => Some("intent:submit"),
        ("POST", p) if p.starts_with("/v1/proposals/") && p.ends_with("/evaluate") => {
            Some("proposal:evaluate")
        }
        // Capability
        ("POST", "/v1/capabilities/mint") => Some("capability:mint"),
        ("POST", p) if p.starts_with("/v1/capabilities/") && p.ends_with("/revoke") => {
            Some("capability:mint")
        }
        // Execution
        ("POST", "/v1/executions/authorize") => Some("execution:authorize"),
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/prepare") => {
            Some("execution:prepare")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/execute") => {
            Some("execution:execute")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/verify") => {
            Some("execution:verify")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/commit") => {
            Some("execution:commit")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/compensate") => {
            Some("execution:compensate")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/cancel") => {
            Some("execution:execute")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/evaluate-outcome") => {
            Some("execution:verify")
        }
        ("GET", p) if p.starts_with("/v1/executions/") => Some("provenance:read"),
        // Approvals
        ("GET", "/v1/approvals") => Some("approval:read"),
        ("GET", p) if p.starts_with("/v1/approvals/") && !p.ends_with("/resolve") => {
            Some("approval:read")
        }
        ("POST", p) if p.starts_with("/v1/approvals/") && p.ends_with("/resolve") => {
            Some("approval:resolve")
        }
        // Quarantine holds (Phase 1: mirror approval scopes)
        ("GET", "/v1/quarantines") => Some("approval:read"),
        ("GET", p) if p.starts_with("/v1/quarantines/") && !p.ends_with("/resolve") => {
            Some("approval:read")
        }
        ("POST", p) if p.starts_with("/v1/quarantines/") && p.ends_with("/resolve") => {
            Some("approval:resolve")
        }
        // Policy bundles
        ("POST", "/v1/policy-bundles") => Some("policy:write"),
        ("GET", "/v1/policy-bundles") => Some("policy:read"),
        ("GET", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/versions") => {
            Some("policy:read")
        }
        ("GET", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/diff") => {
            Some("policy:read")
        }
        ("POST", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/rollback") => {
            Some("policy:write")
        }
        ("POST", "/v1/policy/simulate") => Some("policy:read"),
        ("POST", "/v1/policy-bundles/simulate") => Some("policy:read"),
        ("GET", p) if p.starts_with("/v1/policy-bundles/") => Some("policy:read"),
        ("PUT", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/active") => {
            Some("policy:write")
        }
        ("PUT", p) if p.starts_with("/v1/policy-bundles/") => Some("policy:write"),
        ("DELETE", p) if p.starts_with("/v1/policy-bundles/") => Some("policy:write"),
        // Provenance
        ("POST", "/v1/provenance/query") => Some("provenance:read"),
        ("POST", "/v1/provenance/lineage") => Some("provenance:read"),
        ("GET", p) if p.starts_with("/v1/provenance/lineage/") => Some("provenance:read"),
        ("POST", "/v1/provenance/ingest") => Some("provenance:write"),
        // Bridge
        ("GET", "/v1/bridges") => Some("provenance:read"),
        ("GET", p) if p.starts_with("/v1/bridges/") && p.ends_with("/tools") => {
            Some("provenance:read")
        }
        // Admin tokens
        ("POST", "/v1/admin/tokens") => Some("admin:tokens"),
        ("GET", "/v1/admin/tokens") => Some("admin:tokens"),
        ("DELETE", p) if p.starts_with("/v1/admin/tokens/") => Some("admin:tokens"),
        ("POST", p) if p.starts_with("/v1/admin/tokens/") && p.ends_with("/rotate") => {
            Some("admin:tokens")
        }
        // Admin agents
        ("POST", "/v1/admin/agents") => Some("admin:agents"),
        ("GET", "/v1/admin/agents") => Some("admin:agents"),
        ("DELETE", p) if p.starts_with("/v1/admin/agents/") => Some("admin:agents"),
        // Admin MFA routes
        ("POST", p) if p.starts_with("/v1/admin/agents/") && p.ends_with("/mfa/enroll") => {
            Some("admin:mfa")
        }
        ("POST", p) if p.starts_with("/v1/admin/agents/") && p.ends_with("/mfa/verify") => {
            Some("admin:mfa")
        }
        ("POST", p) if p.starts_with("/v1/admin/agents/") && p.ends_with("/mfa/disable") => {
            Some("admin:mfa")
        }
        ("POST", p) if p.starts_with("/v1/admin/agents/") && p.ends_with("/mfa/rotate") => {
            Some("admin:mfa")
        }
        ("GET", p) if p.starts_with("/v1/admin/agents/") && p.ends_with("/mfa") => {
            Some("admin:mfa")
        }
        ("GET", p)
            if p.starts_with("/v1/admin/agents/")
                && p.contains("/mfa/")
                && !p.ends_with("/mfa") =>
        {
            Some("admin:mfa")
        }
        // Lifecycle outbox operator workflow
        ("GET", "/v1/admin/lifecycle-outbox") => Some("admin:lifecycle-outbox:read"),
        ("GET", p) if p.starts_with("/v1/admin/lifecycle-outbox/") => {
            Some("admin:lifecycle-outbox:read")
        }
        ("POST", p)
            if p.starts_with("/v1/admin/lifecycle-outbox/")
                && (p.ends_with("/retry") || p.ends_with("/resolve")) =>
        {
            Some("admin:lifecycle-outbox:write")
        }
        // Audit logs
        ("GET", "/v1/admin/audit-logs") => Some("admin:audit"),
        ("GET", "/v1/admin/audit-logs/export") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/verify") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/merkle-verify") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/merkle-roots") => Some("admin:audit"),
        ("POST", "/v1/admin/audit/checkpoints") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/checkpoints") => Some("admin:audit"),
        ("GET", p) if p.starts_with("/v1/admin/audit/checkpoints/") && p.ends_with("/verify") => {
            Some("admin:audit")
        }
        _ => Some("admin:tokens"), // Deny-by-default for unknown paths
    }
}

// ---------------------------------------------------------------------------
// Phase 4.3: OIDC/JWT offline validation helpers
// ---------------------------------------------------------------------------

/// Error type for OIDC auth failures.
enum OidcAuthError {
    Unauthorized(String),
    Forbidden(String),
}

/// Validate a Bearer JWT against OIDC config.
///
/// Flow:
/// 1. Decode header, select static key by `kid`.
/// 2. If static key missing and jwks_url configured, fetch from JWKS cache.
/// 3. Validate signature, algorithm allowlist, issuer, audience, exp, nbf.
/// 4. Map actor_id from configured claim.
/// 5. Map role from configured role/group claims via explicit mapping table.
/// 6. Derive scopes via `TokenRole::default_scopes()`.
/// 7. Enforce `required_scope_for_path()`.
///
/// Fail closed: any validation failure returns `OidcAuthError::Unauthorized`.
/// Unmapped role or missing required scope returns `OidcAuthError::Forbidden`.
async fn validate_oidc_token(
    token: &str,
    oidc: &OidcConfig,
    jwks_cache: Option<&Arc<OidcJwksCache>>,
    method: &str,
    path: &str,
) -> Result<(String, Vec<String>), OidcAuthError> {
    // Step 1: decode header to get kid and alg
    let header = match jsonwebtoken::decode_header(token) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(error = %e, "failed to decode jwt header");
            return Err(OidcAuthError::Unauthorized("invalid jwt".to_string()));
        }
    };

    // Reject "none" algorithm unconditionally
    if header.alg == jsonwebtoken::Algorithm::HS256
        && !oidc
            .allowed_algorithms
            .contains(&jsonwebtoken::Algorithm::HS256)
    {
        // HS256 is only allowed if explicitly listed (tests)
    }
    if !oidc.allowed_algorithms.contains(&header.alg) {
        tracing::warn!(alg = ?header.alg, "jwt algorithm not in allowlist");
        return Err(OidcAuthError::Unauthorized(
            "unsupported jwt algorithm".to_string(),
        ));
    }

    // Step 2: select key by kid (empty string fallback for JWTs without kid)
    let kid = header.kid.as_deref().unwrap_or("");
    let key_material = if let Some(km) = oidc.static_keys.get(kid) {
        km.clone()
    } else if let Some(cache) = jwks_cache {
        match cache.get_key(kid).await {
            Ok(Some(km)) => km,
            Ok(None) => {
                tracing::warn!(kid = %kid, "jwt key not found in static keys or jwks");
                return Err(OidcAuthError::Unauthorized("jwt key not found".to_string()));
            }
            Err(e) => {
                tracing::warn!(error = %e, kid = %kid, "jwks fetch failed");
                return Err(OidcAuthError::Unauthorized(
                    "jwt key unavailable".to_string(),
                ));
            }
        }
    } else {
        tracing::warn!(kid = %kid, "jwt key not found in static keys");
        return Err(OidcAuthError::Unauthorized("jwt key not found".to_string()));
    };

    let decoding_key = match key_material.to_decoding_key() {
        Ok(k) => k,
        Err(e) => {
            tracing::error!(error = %e, "failed to build decoding key");
            return Err(OidcAuthError::Unauthorized("jwt key invalid".to_string()));
        }
    };

    // Step 3: build validation
    let mut validation = jsonwebtoken::Validation::new(header.alg);
    validation.leeway = oidc.clock_skew_secs.max(0) as u64;
    validation.validate_nbf = true;
    validation.set_issuer(&[&oidc.issuer]);
    validation.set_audience(&oidc.audiences);
    validation.algorithms = oidc.allowed_algorithms.clone();

    // Step 4: decode and validate signature + claims
    let token_data: jsonwebtoken::TokenData<serde_json::Map<String, serde_json::Value>> =
        match jsonwebtoken::decode(token, &decoding_key, &validation) {
            Ok(td) => td,
            Err(e) => {
                tracing::warn!(error = %e, "jwt validation failed");
                return Err(OidcAuthError::Unauthorized("invalid jwt".to_string()));
            }
        };

    let claims = token_data.claims;

    // Step 4b: explicit future-iat rejection (fail closed).
    // If `iat` is present and beyond now + clock_skew, reject.
    // Missing `iat` is tolerated to avoid breaking IdPs that omit it.
    if let Some(iat_val) = claims.get("iat").and_then(|v| v.as_i64()) {
        let now = chrono::Utc::now().timestamp();
        let skew = oidc.clock_skew_secs.max(0);
        if iat_val > now + skew {
            tracing::warn!(iat = %iat_val, now = %now, skew = %skew, "jwt iat is in the future");
            return Err(OidcAuthError::Unauthorized("invalid jwt".to_string()));
        }
    }

    // Step 5: email verification check
    if oidc.require_email_verified {
        let verified = claims
            .get("email_verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !verified {
            tracing::warn!("jwt email_verified is false or missing");
            return Err(OidcAuthError::Unauthorized(
                "email not verified".to_string(),
            ));
        }
    }

    // Step 6: extract actor_id
    let actor_id = match claims.get(&oidc.actor_id_claim).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            tracing::warn!(claim = %oidc.actor_id_claim, "jwt missing actor_id claim");
            return Err(OidcAuthError::Unauthorized(
                "missing actor_id claim".to_string(),
            ));
        }
    };

    // Step 7: extract role_source claim and map to TokenRole
    let role_source_values = match claims.get(&oidc.role_source_claim) {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => {
            tracing::warn!(
                claim = %oidc.role_source_claim,
                "jwt missing role_source claim"
            );
            return Err(OidcAuthError::Forbidden("unmapped role".to_string()));
        }
    };

    let mapped_role = role_source_values
        .iter()
        .filter_map(|name| oidc.role_mappings.get(name))
        .next()
        .copied();

    let role = match mapped_role {
        Some(r) => r,
        None => {
            tracing::warn!(
                values = ?role_source_values,
                "jwt role not mapped"
            );
            return Err(OidcAuthError::Forbidden("unmapped role".to_string()));
        }
    };

    // Step 8: derive scopes from role
    let scopes = role.default_scopes();

    // Step 9: enforce required scope for path
    if let Some(required) = required_scope_for_path(method, path) {
        let has_scope = scopes.iter().any(|s| s == "*" || s == required);
        if !has_scope {
            tracing::warn!(
                actor_id = %actor_id,
                role = ?role,
                required = %required,
                "jwt insufficient scope"
            );
            return Err(OidcAuthError::Forbidden(format!(
                "required scope {}",
                required
            )));
        }
    }

    tracing::debug!(actor_id = %actor_id, role = ?role, "oidc auth succeeded");
    Ok((actor_id, scopes))
}
