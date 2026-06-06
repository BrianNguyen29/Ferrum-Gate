use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single append-only audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditLogEntry {
    pub id: i64,
    pub actor_id: String,
    pub action: AuditAction,
    pub resource_type: AuditResourceType,
    pub resource_id: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    /// Deterministic SHA-256 hash of canonical entry content (excludes id, hashes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Hash of the previous audit log entry's `content_hash`, forming a linear chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
}

/// The action performed in an audit log entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum AuditAction {
    TokenCreate,
    TokenRevoke,
    TokenRotate,
    PolicyBundleCreate,
    PolicyBundleActivate,
    PolicyBundleRollback,
    ApprovalResolve,
    ExecutionCancel,
    AuthFailed,
    AgentRegister,
    AgentRevoke,
    AgentAuthFailed,
    LifecycleOutboxRetry,
    LifecycleOutboxResolve,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuditAction::TokenCreate => "token_create",
            AuditAction::TokenRevoke => "token_revoke",
            AuditAction::TokenRotate => "token_rotate",
            AuditAction::PolicyBundleCreate => "policy_bundle_create",
            AuditAction::PolicyBundleActivate => "policy_bundle_activate",
            AuditAction::PolicyBundleRollback => "policy_bundle_rollback",
            AuditAction::ApprovalResolve => "approval_resolve",
            AuditAction::ExecutionCancel => "execution_cancel",
            AuditAction::AuthFailed => "auth_failed",
            AuditAction::AgentRegister => "agent_register",
            AuditAction::AgentRevoke => "agent_revoke",
            AuditAction::AgentAuthFailed => "agent_auth_failed",
            AuditAction::LifecycleOutboxRetry => "lifecycle_outbox_retry",
            AuditAction::LifecycleOutboxResolve => "lifecycle_outbox_resolve",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for AuditAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "token_create" => Ok(AuditAction::TokenCreate),
            "token_revoke" => Ok(AuditAction::TokenRevoke),
            "token_rotate" => Ok(AuditAction::TokenRotate),
            "policy_bundle_create" => Ok(AuditAction::PolicyBundleCreate),
            "policy_bundle_activate" => Ok(AuditAction::PolicyBundleActivate),
            "policy_bundle_rollback" => Ok(AuditAction::PolicyBundleRollback),
            "approval_resolve" => Ok(AuditAction::ApprovalResolve),
            "execution_cancel" => Ok(AuditAction::ExecutionCancel),
            "auth_failed" => Ok(AuditAction::AuthFailed),
            "agent_register" => Ok(AuditAction::AgentRegister),
            "agent_revoke" => Ok(AuditAction::AgentRevoke),
            "agent_auth_failed" => Ok(AuditAction::AgentAuthFailed),
            "lifecycle_outbox_retry" => Ok(AuditAction::LifecycleOutboxRetry),
            "lifecycle_outbox_resolve" => Ok(AuditAction::LifecycleOutboxResolve),
            _ => Err(format!("invalid audit action: {}", s)),
        }
    }
}

/// The type of resource affected by an audited action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum AuditResourceType {
    Token,
    PolicyBundle,
    Approval,
    Execution,
    Auth,
    Agent,
    LifecycleOutbox,
}

impl std::fmt::Display for AuditResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuditResourceType::Token => "token",
            AuditResourceType::PolicyBundle => "policy_bundle",
            AuditResourceType::Approval => "approval",
            AuditResourceType::Execution => "execution",
            AuditResourceType::Auth => "auth",
            AuditResourceType::Agent => "agent",
            AuditResourceType::LifecycleOutbox => "lifecycle_outbox",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for AuditResourceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "token" => Ok(AuditResourceType::Token),
            "policy_bundle" => Ok(AuditResourceType::PolicyBundle),
            "approval" => Ok(AuditResourceType::Approval),
            "execution" => Ok(AuditResourceType::Execution),
            "auth" => Ok(AuditResourceType::Auth),
            "agent" => Ok(AuditResourceType::Agent),
            "lifecycle_outbox" => Ok(AuditResourceType::LifecycleOutbox),
            _ => Err(format!("invalid audit resource type: {}", s)),
        }
    }
}

/// Request to list audit log entries with optional filters.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditLogListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<AuditAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<AuditResourceType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Filter entries created at or after this time (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// Filter entries created at or before this time (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
}

fn default_limit() -> u32 {
    50
}

/// Response envelope for paginated audit log lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditLogListResponse {
    pub items: Vec<AuditLogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}

/// Response from an audit chain verification request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditLogVerifyResponse {
    /// Whether the chain is intact. If false, `error` should be populated.
    pub valid: bool,
    /// Total number of entries inspected (includes legacy entries without hashes).
    pub total_entries: usize,
    /// Number of hashed (tamper-evident) entries in the chain.
    pub hashed_entries: usize,
    /// Human-readable error if the chain is broken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A cached Merkle root for an audit log time window.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditMerkleRoot {
    /// Start of the UTC-aligned hourly window (RFC 3339).
    pub window_start: DateTime<Utc>,
    /// Hex-encoded Merkle root hash (empty when no entries).
    pub root: String,
    /// Number of audit log entries included in the root.
    pub entry_count: i64,
    /// When the root was computed (RFC 3339).
    pub computed_at: DateTime<Utc>,
}

/// Response from a Merkle root verification request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditMerkleVerifyResponse {
    /// Whether the root is present and valid.
    pub valid: bool,
    /// The requested window start.
    pub window_start: DateTime<Utc>,
    /// The computed Merkle root (empty if no entries).
    pub root: String,
    /// Number of entries in the window.
    pub entry_count: i64,
    /// Human-readable error if validation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response envelope for paginated Merkle root lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditMerkleRootListResponse {
    pub items: Vec<AuditMerkleRoot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}

/// A signed checkpoint over a Merkle root for an audit log time window.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditCheckpoint {
    /// Start of the UTC-aligned hourly window (RFC 3339).
    pub window_start: DateTime<Utc>,
    /// Hex-encoded Merkle root hash.
    pub merkle_root: String,
    /// Number of audit log entries included in the root.
    pub entry_count: i64,
    /// Signer identifier (e.g., operator name, agent_id).
    pub signer_id: String,
    /// Hex-encoded SHA-256 fingerprint of the Ed25519 public key used to verify.
    pub signer_key_fingerprint: String,
    /// When the checkpoint was signed (RFC 3339).
    pub signed_at: DateTime<Utc>,
    /// Base64-encoded Ed25519 signature.
    pub signature: String,
    /// Base64-encoded Ed25519 public key used for verification.
    pub public_key: String,
}

/// Request to create a signed checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateCheckpointRequest {
    pub window_start: DateTime<Utc>,
    pub merkle_root: String,
    pub entry_count: i64,
    pub signer_id: String,
    pub signer_key_fingerprint: String,
    pub signed_at: DateTime<Utc>,
    pub signature: String,
    pub public_key: String,
}

/// Response envelope for paginated checkpoint lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditCheckpointListResponse {
    pub items: Vec<AuditCheckpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}

/// Response from a checkpoint verification request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditCheckpointVerifyResponse {
    /// Whether the checkpoint is present, signature valid, and Merkle root matches.
    pub valid: bool,
    /// The requested window start.
    pub window_start: DateTime<Utc>,
    /// Human-readable error if validation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Stored checkpoint details (if found).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<AuditCheckpoint>,
    /// Current computed Merkle root for the window (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_root: Option<String>,
    /// Current entry count for the window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_entry_count: Option<i64>,
}

/// Compute the deterministic canonical checkpoint payload hash.
///
/// The canonical JSON is alphabetically sorted and compact:
/// `{"entry_count":N,"merkle_root":"...","signed_at":"...","window_start":"..."}`
/// The resulting SHA-256 digest is returned as raw bytes (32).
pub fn canonical_checkpoint_hash(
    window_start: &DateTime<Utc>,
    merkle_root: &str,
    entry_count: i64,
    signed_at: &DateTime<Utc>,
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    // Build canonical JSON with keys in alphabetical order, no extra whitespace.
    let canonical = format!(
        "{{\"entry_count\":{},\"merkle_root\":\"{}\",\"signed_at\":\"{}\",\"window_start\":\"{}\"}}",
        entry_count,
        merkle_root,
        signed_at.to_rfc3339(),
        window_start.to_rfc3339(),
    );
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher.finalize().to_vec()
}
