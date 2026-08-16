// Extracted governance handler modules backing the wiring
// (state -> server -> approval + cross-cutting helpers).
mod admin;
mod approval;
mod audit;
mod auth;
mod auth_actor;
mod behavioral;
mod bridge;
mod capabilities;
mod execution;
mod ha_reconciler;
mod intents;
mod lineage;
mod macros;
mod metrics;
mod mfa;
mod monitoring;
mod policy;
mod policy_eval;
mod problem;
mod proposals;
mod provenance;
mod quarantine;
mod rate_limit;
mod response;
mod router;
mod server;
mod state;
mod timeout_reconciler;

#[cfg(test)]
mod object_guard_tests;

#[cfg(feature = "worm-sink")]
mod worm_sink;

pub(crate) use auth_actor::AuthActor;
pub use capabilities::StoreCapabilityService;
pub use mfa::*;
pub use server::*;
pub use state::*;
