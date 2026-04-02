use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Namespace UUID for deterministic policy bundle ID derivation.
/// This is a well-known namespace UUID that we use to derive
/// deterministic UUIDs from policy bundle content.
const POLICY_BUNDLE_NAMESPACE: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_abcd_ef1234567890);

macro_rules! strong_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generate a new random UUID (random variant).
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

strong_id!(IntentId);
strong_id!(ProposalId);
strong_id!(CapabilityId);
strong_id!(ExecutionId);
strong_id!(EventId);
strong_id!(RollbackContractId);
strong_id!(ApprovalId);
strong_id!(PrincipalId);
strong_id!(SessionId);
strong_id!(ChannelId);

/// PolicyBundleId uses deterministic derivation from policy bundle content.
/// This enables same-input same-id behavior for policy bundle identity propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct PolicyBundleId(pub Uuid);

impl PolicyBundleId {
    /// Generate a new random UUID (random variant).
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Derive a deterministic UUID from policy bundle content using UUID v5
    /// (name-based with SHA-1). Same content always produces the same UUID.
    pub fn derive(content: &str) -> Self {
        Self(Uuid::new_v5(&POLICY_BUNDLE_NAMESPACE, content.as_bytes()))
    }
}

impl Default for PolicyBundleId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PolicyBundleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PolicyBundleId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(Self)
    }
}
