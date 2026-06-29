/// Request-scoped authenticated actor identity inserted by the auth middleware
/// for auth modes that carry inherent identity (Scoped, OIDC, Agent).
/// Handlers fall back to `"unknown"` when the extension is absent.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct AuthActor {
    pub(crate) actor_id: String,
    pub(crate) source: &'static str,
}

/// Extract the actor ID from an optional `AuthActor` extension, falling back to `"unknown"`.
pub(crate) fn audit_actor(auth_actor: Option<&AuthActor>) -> &str {
    auth_actor.map(|a| a.actor_id.as_str()).unwrap_or("unknown")
}
