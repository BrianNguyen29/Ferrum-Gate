use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A registered agent identity record.
///
/// Stored in the gateway store (SQLite/PostgreSQL). Each record is immutable
/// after creation; revocation is handled via `revoked_at`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentRecord {
    pub agent_id: String,
    /// Base64-encoded Ed25519 raw 32-byte public key.
    pub public_key: String,
    /// Base64url-encoded SHA-256 of `public_key`.
    pub key_fingerprint: String,
    /// Subset of FerrumGate scopes; deny-by-default for unlisted scopes.
    pub allowed_scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to register a new agent identity.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegisterAgentRequest {
    pub agent_id: String,
    /// Base64-encoded Ed25519 raw 32-byte public key.
    pub public_key: String,
    /// Scope list (repeatable). If omitted, uses a minimal default set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Response when registering a new agent identity.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegisterAgentResponse {
    pub agent: AgentRecord,
}

/// Request to revoke an agent identity.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeAgentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Paginated list of agent identities.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentListResponse {
    pub items: Vec<AgentRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}
