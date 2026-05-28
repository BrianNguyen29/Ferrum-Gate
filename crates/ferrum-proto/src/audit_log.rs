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
}

impl std::fmt::Display for AuditResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuditResourceType::Token => "token",
            AuditResourceType::PolicyBundle => "policy_bundle",
            AuditResourceType::Approval => "approval",
            AuditResourceType::Execution => "execution",
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
