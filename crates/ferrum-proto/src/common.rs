use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type Timestamp = DateTime<Utc>;
pub type JsonMap = IndexMap<String, serde_json::Value>;
pub type Sha256Hex = String;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum RiskTier {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ApprovalMode {
    None,
    Required,
    DraftOnly,
    TwoPhaseCommit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Quarantine,
    RequireApproval,
    AllowDraftOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum TrustLabel {
    Trusted,
    UserProvided,
    InternalPolicy,
    InternalSystem,
    ExternalWeb,
    ExternalEmail,
    ExternalRepoText,
    ExternalToolMetadata,
    ExternalToolOutput,
    OCRExtracted,
    Untrusted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum SensitivityLabel {
    Public,
    Internal,
    Confidential,
    Secret,
    Credential,
    Pii,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ResourceMode {
    Read,
    Write,
    ReadWrite,
    Draft,
    Execute,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum RollbackClass {
    R0NativeReversible,
    R1SnapshotRecoverable,
    R2Compensatable,
    R3IrreversibleHighConsequence,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimeBudget {
    pub max_duration_ms: u64,
    pub max_steps: u32,
    pub max_retries_per_step: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrustContextSummary {
    pub input_labels: Vec<TrustLabel>,
    pub sensitivity_labels: Vec<SensitivityLabel>,
    pub taint_score: u8,
    pub contains_external_metadata: bool,
    pub contains_tool_output: bool,
    pub contains_untrusted_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActorRef {
    pub actor_type: ActorType,
    pub actor_id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ActorType {
    User,
    Agent,
    PolicyEngine,
    Gateway,
    Adapter,
    Operator,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObjectRef {
    pub object_type: ObjectType,
    pub object_id: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ObjectType {
    Intent,
    Proposal,
    Capability,
    ToolManifest,
    ToolCall,
    ToolOutput,
    SideEffect,
    RollbackContract,
    Approval,
    PolicyBundle,
    Message,
    File,
    GitRef,
    SqlQuery,
    HttpRequest,
    EmailDraft,
    ProvenanceEvent,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HashChainRef {
    pub content_hash: Option<Sha256Hex>,
    pub manifest_hash: Option<Sha256Hex>,
    pub policy_bundle_hash: Option<Sha256Hex>,
    pub previous_ledger_hash: Option<Sha256Hex>,
}
