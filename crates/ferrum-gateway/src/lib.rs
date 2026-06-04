// Extracted governance handler modules backing the wiring
// (state -> server -> approval + cross-cutting helpers).
mod admin;
mod approval;
mod audit;
mod bridge;
mod capabilities;
mod execution;
mod intents;
mod lineage;
mod macros;
mod monitoring;
mod policy;
mod policy_eval;
mod problem;
mod proposals;
mod response;
mod server;
mod state;

pub use server::*;
pub use state::*;
