use crate::problem::ApiProblem;
use crate::state::AuthMode;

/// Request-scoped authenticated actor identity inserted by the auth middleware
/// for auth modes that carry inherent identity (Scoped, OIDC, Agent).
/// Handlers fall back to `"unknown"` when the extension is absent.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct AuthActor {
    pub(crate) actor_id: String,
    pub(crate) source: &'static str,
    pub(crate) scopes: Vec<String>,
    /// Authenticated token role when the auth source carries one (Scoped, OIDC).
    /// `None` for Agent auth, which authenticates by Ed25519 key, not by role.
    pub(crate) role: Option<ferrum_proto::TokenRole>,
}

impl AuthActor {
    /// Check whether the actor has the given scope (or the wildcard `*`).
    pub(crate) fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == "*" || s == scope)
    }
}

/// Extract the actor ID from an optional `AuthActor` extension, falling back to `"unknown"`.
pub(crate) fn audit_actor(auth_actor: Option<&AuthActor>) -> &str {
    auth_actor.map(|a| a.actor_id.as_str()).unwrap_or("unknown")
}

/// Generic non-enumerating 404 used when a workflow object access is denied.
fn guard_not_found() -> ApiProblem {
    ApiProblem::object_not_found()
}

/// Enforce exact-owner access control for a workflow object (capability or execution).
///
/// - `Bearer`/`Disabled` auth modes retain compatibility and are unaffected.
/// - Authenticated identity modes (`Scoped`, `OIDC`, `Agent`) require a present
///   `AuthActor`.
/// - `owner_actor_id: Some` requires an exact match with `AuthActor.actor_id`.
/// - `owner_actor_id: None` denies by default. A finite, explicit
///   `legacy_object_compat_allow_until` permits access only until the deadline.
///
/// Emits bounded security telemetry with object kind and operation only (no
/// object IDs, actor IDs, or tokens).
pub(crate) fn enforce_object_owner_guard(
    auth_actor: Option<&AuthActor>,
    owner_actor_id: Option<&String>,
    auth_mode: AuthMode,
    legacy_object_compat_allow_until: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
    object_kind: &'static str,
    operation: &'static str,
) -> Result<(), ApiProblem> {
    // Bearer/Disabled retain current compatibility.
    if matches!(auth_mode, AuthMode::Bearer | AuthMode::Disabled) {
        return Ok(());
    }

    let Some(actor) = auth_actor else {
        tracing::warn!(
            object_kind,
            operation,
            decision = "deny",
            reason = "missing_actor",
            "object access denied"
        );
        return Err(guard_not_found());
    };

    if let Some(owner) = owner_actor_id {
        if actor.actor_id == *owner {
            return Ok(());
        }
        tracing::warn!(
            object_kind,
            operation,
            decision = "deny",
            reason = "owner_mismatch",
            "object access denied"
        );
        return Err(guard_not_found());
    }

    // Legacy unbound object.
    if let Some(allow_until) = legacy_object_compat_allow_until {
        if now < allow_until {
            tracing::warn!(
                object_kind,
                operation,
                decision = "allow",
                reason = "legacy_compat",
                allow_until = %allow_until,
                "legacy unbound object access allowed by temporary policy"
            );
            return Ok(());
        }
    }

    tracing::warn!(
        object_kind,
        operation,
        decision = "deny",
        reason = "unbound_no_compat",
        "legacy unbound object access denied"
    );
    Err(guard_not_found())
}
