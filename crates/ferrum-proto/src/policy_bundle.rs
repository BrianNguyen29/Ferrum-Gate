use crate::{PolicyBundleId, Timestamp, intent::OutcomeClause};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A named, versioned policy bundle persisted for auditability and reuse.
///
/// H1.1a: Policy bundle lifecycle tooling — persistence, inspection, and management
/// of authored intent outcome contracts (allowed_outcomes / forbidden_outcomes).
///
/// Policy bundles are derived deterministically from their content fingerprint
/// (see [`PolicyBundleId::derive`]), enabling same-input same-id behavior for
/// policy bundle identity propagation across intent compilations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundle {
    /// Stable identifier derived deterministically from bundle content.
    pub bundle_id: PolicyBundleId,
    /// Human-readable name for operator reference.
    pub name: String,
    /// Free-form description of what this bundle governs.
    pub description: String,
    /// Semantic version tag for the bundle content.
    pub version: String,
    /// When this bundle was first persisted.
    pub created_at: Timestamp,
    /// When this bundle was last updated (Same as created_at if never updated).
    pub updated_at: Timestamp,
}

/// Request to register a new policy bundle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleRegisterRequest {
    /// Human-readable name (unique per bundle_id).
    pub name: String,
    /// Free-form description.
    pub description: String,
    /// Semantic version tag (e.g. "1.0.0").
    pub version: String,
    /// Optional policy bundle fingerprint (derived from outcome contract content).
    /// When provided, must match the deterministic fingerprint computed from
    /// allowed_outcomes and forbidden_outcomes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Optional outcome clauses to persist with this bundle.
    /// When provided alongside fingerprint, the server validates that the
    /// fingerprint matches the canonical serialization of these clauses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_outcomes: Option<Vec<OutcomeClause>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_outcomes: Option<Vec<OutcomeClause>>,
}

/// Response when a policy bundle is registered or fetched.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleResponse {
    pub bundle: PolicyBundle,
}

/// Response for a list of bundles (paginated).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PolicyBundleListResponse {
    pub items: Vec<PolicyBundle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
