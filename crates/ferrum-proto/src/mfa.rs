use crate::{MfaFactorId, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaFactor {
    pub id: MfaFactorId,
    pub factor_type: MfaFactorType,
    pub status: MfaFactorStatus,
    pub label: Option<String>,
    pub created_at: Timestamp,
    /// TOTP code or other verification payload (sent by client, never stored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MfaFactorType {
    Totp,
}

impl std::fmt::Display for MfaFactorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MfaFactorType::Totp => "totp",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for MfaFactorType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "totp" => Ok(MfaFactorType::Totp),
            _ => Err(format!("invalid MFA factor type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum MfaFactorStatus {
    Active,
    Inactive,
    Pending,
}

impl std::fmt::Display for MfaFactorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MfaFactorStatus::Active => "Active",
            MfaFactorStatus::Inactive => "Inactive",
            MfaFactorStatus::Pending => "Pending",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for MfaFactorStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Active" => Ok(MfaFactorStatus::Active),
            "Inactive" => Ok(MfaFactorStatus::Inactive),
            "Pending" => Ok(MfaFactorStatus::Pending),
            _ => Err(format!("invalid MFA factor status: {}", s)),
        }
    }
}

/// Store-backed record for an MFA credential.
///
/// Holds the encrypted TOTP secret and metadata required for
/// verification and lifecycle management. The encryption algorithm
/// is not yet implemented; this record provides the storage schema.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaCredentialRecord {
    pub mfa_factor_id: MfaFactorId,
    pub agent_id: String,
    pub factor_type: MfaFactorType,
    pub status: MfaFactorStatus,
    /// Encrypted TOTP secret (ciphertext bytes, base64-encoded).
    pub encrypted_secret: String,
    /// Nonce used during encryption (base64-encoded).
    pub secret_nonce: String,
    /// Identifier of the key used to encrypt the secret.
    pub encryption_key_id: String,
    /// Human-readable label (e.g. "Work Laptop").
    pub label: Option<String>,
    pub created_at: Timestamp,
    /// When the factor was first verified by the user.
    pub verified_at: Option<Timestamp>,
    /// When the factor was last used for a successful verification.
    pub last_used_at: Option<Timestamp>,
    /// The TOTP counter (time step) from the last successful verification.
    /// Used for replay protection: codes from older or equal counters are rejected.
    pub last_used_counter: Option<u64>,
    /// When the factor was revoked (soft-delete).
    pub revoked_at: Option<Timestamp>,
    /// Number of consecutive failed verification attempts since the last success.
    #[serde(default)]
    pub failed_attempts: u32,
    /// If set, the factor is locked until this timestamp.
    pub locked_until: Option<Timestamp>,
    /// Timestamp of the most recent failed verification attempt.
    pub last_failed_at: Option<Timestamp>,
    /// Total number of times this factor has been locked (lifetime counter).
    #[serde(default)]
    pub lockout_count: u32,
    /// Raw JSON for extensibility and forward compatibility.
    pub raw_json: serde_json::Value,
}

impl MfaCredentialRecord {
    pub fn new(
        agent_id: impl Into<String>,
        factor_type: MfaFactorType,
        encrypted_secret: impl Into<String>,
        secret_nonce: impl Into<String>,
        encryption_key_id: impl Into<String>,
    ) -> Self {
        let mfa_factor_id = MfaFactorId(Uuid::new_v4());
        Self {
            mfa_factor_id,
            agent_id: agent_id.into(),
            factor_type,
            status: MfaFactorStatus::Pending,
            encrypted_secret: encrypted_secret.into(),
            secret_nonce: secret_nonce.into(),
            encryption_key_id: encryption_key_id.into(),
            label: None,
            created_at: chrono::Utc::now(),
            verified_at: None,
            last_used_at: None,
            last_used_counter: None,
            revoked_at: None,
            failed_attempts: 0,
            locked_until: None,
            last_failed_at: None,
            lockout_count: 0,
            raw_json: serde_json::Value::Null,
        }
    }
}

// ── Admin route request/response DTOs ──

/// Response to a successful MFA enrollment.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaEnrollResponse {
    pub mfa_factor_id: MfaFactorId,
    pub otpauth_uri: String,
}

/// Request body to verify a pending MFA factor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaVerifyRequest {
    pub code: String,
}

/// Response after verifying a pending MFA factor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaVerifyResponse {
    pub verified: bool,
}

/// Response after disabling (revoking) an active MFA factor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaDisableResponse {
    pub disabled: bool,
}

/// Request body to disable an active MFA factor.
///
/// When an active factor exists, callers must either:
/// - Provide the current `code` to re-verify TOTP, or
/// - Provide a non-empty `reason` and have the `admin:mfa:breakglass` scope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaDisableRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response after rotating an active MFA factor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaRotateResponse {
    pub mfa_factor_id: MfaFactorId,
    pub otpauth_uri: String,
}

/// Request body to rotate an active MFA factor.
///
/// When an active factor exists, callers must either:
/// - Provide the current `code` to re-verify TOTP, or
/// - Provide a non-empty `reason` and have the `admin:mfa:breakglass` scope.
///
/// When no active factor exists, rotation proceeds like enrollment without re-verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaRotateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Sanitized summary of an MFA credential for list/get routes.
/// Does not include the encrypted secret.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaFactorSummary {
    pub mfa_factor_id: MfaFactorId,
    pub factor_type: MfaFactorType,
    pub status: MfaFactorStatus,
    pub label: Option<String>,
    pub created_at: Timestamp,
    pub verified_at: Option<Timestamp>,
    pub last_used_at: Option<Timestamp>,
    pub last_used_counter: Option<u64>,
    pub revoked_at: Option<Timestamp>,
    #[serde(default)]
    pub failed_attempts: u32,
    pub locked_until: Option<Timestamp>,
    pub last_failed_at: Option<Timestamp>,
    #[serde(default)]
    pub lockout_count: u32,
}

/// Response for listing an agent's MFA factors.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MfaFactorListResponse {
    pub items: Vec<MfaFactorSummary>,
    pub total: usize,
}
