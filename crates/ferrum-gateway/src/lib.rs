// Extracted governance handler modules backing the wiring
// (state -> server -> approval + cross-cutting helpers).
mod admin;
mod approval;
mod audit;
mod auth_actor;
mod bridge;
mod capabilities;
mod execution;
mod intents;
mod lineage;
mod macros;
mod mfa;
mod monitoring;
mod policy;
mod policy_eval;
mod problem;
mod proposals;
mod provenance;
mod response;
mod server;
mod state;

pub(crate) use auth_actor::AuthActor;
pub use capabilities::StoreCapabilityService;
pub use mfa::*;
pub use server::*;
pub use state::*;
