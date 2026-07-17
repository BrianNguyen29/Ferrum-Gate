use crate::auth_actor::AuthActor;
use ferrum_proto::{ApiError, ApiErrorCode};

/// Enforce scope attenuation: an issuer may only grant a subset of the scopes it
/// already holds, and wildcard (`*`) issuance requires the issuer itself to
/// hold wildcard authority.
///
/// Returns a bounded `ApiError` with `ApiErrorCode::Forbidden` when the request
/// exceeds the issuer's authority. Callers must map this to a `FORBIDDEN`
/// response before any persistence side effect.
pub(crate) fn check_scope_attenuation(
    issuer: &AuthActor,
    requested_scopes: &[String],
) -> Result<(), ApiError> {
    let issuer_has_wildcard = issuer.scopes.iter().any(|s| s == "*");
    if issuer_has_wildcard {
        return Ok(());
    }

    if requested_scopes.iter().any(|s| s == "*") {
        return Err(ApiError {
            code: ApiErrorCode::Forbidden,
            message: "wildcard scope requires issuer wildcard authority".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        });
    }

    for scope in requested_scopes {
        if !issuer.has_scope(scope) {
            return Err(ApiError {
                code: ApiErrorCode::Forbidden,
                message: format!("requested scope '{}' exceeds issuer scopes", scope),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            });
        }
    }

    Ok(())
}
