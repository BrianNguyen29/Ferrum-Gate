/// Request-scoped authenticated actor identity inserted by the auth middleware
/// for auth modes that carry inherent identity (Scoped, OIDC, Agent).
/// Handlers fall back to `"unknown"` when the extension is absent.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct AuthActor {
    pub(crate) actor_id: String,
    pub(crate) source: &'static str,
    pub(crate) scopes: Vec<String>,
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
