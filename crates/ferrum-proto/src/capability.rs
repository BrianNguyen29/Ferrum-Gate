use crate::{
    ApprovalId, CapabilityId, JsonMap, PolicyBundleId, ProposalId, ResourceMode, Timestamp,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityLease {
    pub capability_id: CapabilityId,
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub tool_binding: ToolBinding,
    pub resource_bindings: Vec<ResourceBinding>,
    pub argument_constraints: Vec<ArgumentConstraint>,
    pub taint_budget: TaintBudget,
    pub approval_binding: Option<ApprovalBinding>,
    pub issued_by: String,
    pub policy_bundle_id: PolicyBundleId,
    pub tool_manifest_id: Option<String>,
    pub manifest_hash: Option<String>,
    pub status: CapabilityStatus,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum CapabilityStatus {
    Active,
    Used,
    Expired,
    Revoked,
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolBinding {
    pub server_name: String,
    pub tool_name: String,
    pub tool_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ResourceBinding {
    File {
        path: String,
        mode: ResourceMode,
        required_hash: Option<String>,
    },
    Git {
        repo_path: String,
        allowed_refs: Vec<String>,
        mode: ResourceMode,
    },
    Sqlite {
        db_path: String,
        tables: Vec<String>,
        mode: ResourceMode,
    },
    Http {
        method: crate::HttpMethod,
        base_url: String,
        path_prefix: String,
        header_allowlist: Vec<String>,
        mode: ResourceMode,
    },
    EmailDraft {
        recipients: Vec<String>,
        allow_send: bool,
        mode: ResourceMode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum ArgumentConstraint {
    ExactString { key: String, value: String },
    StringOneOf { key: String, values: Vec<String> },
    StringRegex { key: String, pattern: String },
    IntRange { key: String, min: i64, max: i64 },
    BoolExact { key: String, value: bool },
    JsonPointerMustExist { pointer: String },
    JsonPointerMustNotExist { pointer: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaintBudget {
    pub max_taint_score: u8,
    pub allow_external_tool_output: bool,
    pub allow_external_metadata: bool,
    pub allow_untrusted_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalBinding {
    pub approval_id: ApprovalId,
    pub approver_roles: Vec<String>,
    pub approved_action_digest: String,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityMintRequest {
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub tool_binding: ToolBinding,
    pub resource_bindings: Vec<ResourceBinding>,
    pub argument_constraints: Vec<ArgumentConstraint>,
    pub taint_budget: TaintBudget,
    pub approval_binding: Option<ApprovalBinding>,
    pub requested_ttl_secs: u64,
    /// U1-S9a: Optional pre-computed policy bundle ID.
    /// When provided, the capability service uses this instead of generating a random one.
    /// Derived deterministically from the intent's outcome contracts.
    pub policy_bundle_id: Option<PolicyBundleId>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityMintResponse {
    pub lease: CapabilityLease,
    pub warnings: Vec<String>,
}
