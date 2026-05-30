use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A scoped opaque bearer token.
///
/// Token values are generated server-side and returned exactly once.
/// The server stores only a hash of the token value, not the plaintext.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScopedToken {
    pub token_id: String,
    pub actor_id: String,
    pub role: TokenRole,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_from: Option<String>,
    /// Deterministic lookup hash for fast DB lookup (blake3 of raw token value).
    /// NOT the verification hash — do not use this for authentication.
    #[serde(skip)]
    pub token_lookup_hash: String,
    /// Secure verification hash: blake3(salt || token_value).
    #[serde(skip)]
    pub token_hash: String,
    /// Random 16-byte salt (hex-encoded) for the verification hash.
    #[serde(skip)]
    pub token_salt: String,
}

/// Role assigned to a scoped token.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum TokenRole {
    Admin,
    Operator,
    PolicyAuthor,
    Auditor,
    Agent,
    ReadOnly,
}

impl TokenRole {
    /// Returns the default scope set for this role.
    pub fn default_scopes(&self) -> Vec<String> {
        match self {
            TokenRole::Admin => vec!["*".to_string()],
            TokenRole::Operator => vec![
                "approval:resolve".to_string(),
                "provenance:read".to_string(),
                "policy:read".to_string(),
                "execution:verify".to_string(),
                "backup:run".to_string(),
            ],
            TokenRole::PolicyAuthor => vec![
                "policy:read".to_string(),
                "policy:write".to_string(),
                "provenance:read".to_string(),
            ],
            TokenRole::Auditor => vec!["provenance:read".to_string()],
            TokenRole::Agent => vec![
                "intent:submit".to_string(),
                "proposal:evaluate".to_string(),
                "capability:mint".to_string(),
                "execution:authorize".to_string(),
                "execution:prepare".to_string(),
                "execution:execute".to_string(),
                "execution:verify".to_string(),
                "execution:compensate".to_string(),
            ],
            TokenRole::ReadOnly => vec!["policy:read".to_string(), "provenance:read".to_string()],
        }
    }
}

impl std::fmt::Display for TokenRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TokenRole::Admin => "admin",
            TokenRole::Operator => "operator",
            TokenRole::PolicyAuthor => "policy_author",
            TokenRole::Auditor => "auditor",
            TokenRole::Agent => "agent",
            TokenRole::ReadOnly => "read_only",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for TokenRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(TokenRole::Admin),
            "operator" => Ok(TokenRole::Operator),
            "policy_author" => Ok(TokenRole::PolicyAuthor),
            "auditor" => Ok(TokenRole::Auditor),
            "agent" => Ok(TokenRole::Agent),
            "read_only" => Ok(TokenRole::ReadOnly),
            _ => Err(format!("invalid token role: {}", s)),
        }
    }
}

/// Request to create a new scoped token.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateTokenRequest {
    pub actor_id: String,
    pub role: TokenRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub expires_at: DateTime<Utc>,
}

/// Response when creating a scoped token.
/// The `token_value` is returned exactly once.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateTokenResponse {
    pub token: ScopedTokenMeta,
    pub token_value: String,
}

/// Metadata for a scoped token (no secret material).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScopedTokenMeta {
    pub token_id: String,
    pub actor_id: String,
    pub role: TokenRole,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_from: Option<String>,
}

impl From<ScopedToken> for ScopedTokenMeta {
    fn from(token: ScopedToken) -> Self {
        Self {
            token_id: token.token_id,
            actor_id: token.actor_id,
            role: token.role,
            scopes: token.scopes,
            description: token.description,
            expires_at: token.expires_at,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
            revoked_at: token.revoked_at,
            revoked_reason: token.revoked_reason,
            rotated_from: token.rotated_from,
        }
    }
}

/// Request to revoke a scoped token.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeTokenRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request to rotate a scoped token.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RotateTokenRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Paginated list of scoped tokens.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenListResponse {
    pub items: Vec<ScopedTokenMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}

/// Authentication mode for the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthMode {
    #[default]
    Disabled,
    Bearer,
    Scoped,
    /// OIDC/JWT bearer token validation (Phase 4.3+).
    Oidc,
    /// Ed25519 agent identity signature verification (Phase 4.6+).
    Agent,
}

impl std::fmt::Display for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuthMode::Disabled => "disabled",
            AuthMode::Bearer => "bearer",
            AuthMode::Scoped => "scoped",
            AuthMode::Oidc => "oidc",
            AuthMode::Agent => "agent",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for AuthMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "disabled" => Ok(AuthMode::Disabled),
            "bearer" => Ok(AuthMode::Bearer),
            "scoped" => Ok(AuthMode::Scoped),
            "oidc" => Ok(AuthMode::Oidc),
            "agent" => Ok(AuthMode::Agent),
            _ => Err(format!("invalid auth mode: {}", s)),
        }
    }
}
