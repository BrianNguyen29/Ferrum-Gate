//! Strongly-typed UUID wrappers for domain identifiers.
//!
//! # Examples
//!
//! ```
//! use ferrum_proto::{IntentId, CapabilityId, ProposalId};
//!
//! let intent = IntentId::new();
//! let cap = CapabilityId::new();
//! let proposal = ProposalId::new();
//! assert!(!intent.to_string().is_empty());
//! assert!(!cap.to_string().is_empty());
//! assert!(!proposal.to_string().is_empty());
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! strong_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
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
strong_id!(LifecycleOutboxId);
strong_id!(ApprovalId);
strong_id!(PolicyBundleId);
strong_id!(PrincipalId);
strong_id!(SessionId);
strong_id!(ChannelId);
