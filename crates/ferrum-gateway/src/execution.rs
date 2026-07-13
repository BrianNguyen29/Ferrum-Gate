//! Pure execution helpers for the gateway.
//!
//! Stages 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10 of the server.rs refactor: move the helper functions used
//! by execution handlers out of `server.rs` so that the handler modules can
//! stay focused on transport concerns.
//!
//! Scope (Stage 2 — pure helpers):
//! - Argument-constraint validation
//! - Resource-binding subset-of-scope validation
//! - Rollback prepare request construction
//! - Action / adapter / target inference from tool name and resource scope
//! - Rollback class inference
//! - HTTP compensation enrichment
//! - Path-UUID parsing (`parse_execution_id`)
//!
//! Scope (Stage 3 — async store/capability helpers):
//! - `get_capability_for_authorize` — load a capability from the in-memory
//!   service with a persisted-store fallback for `authorize_execution`.
//! - `mark_capability_used_durable` — mark a capability consumed in memory and
//!   persist the updated status (with atomic store-only fallback).
//! - `validate_approval_binding_digest` — enforce I6 binding-digest invariants
//!   before authorizing an execution.
//!
//! Scope (Stage 4 — low-risk HTTP handlers):
//! - `cancel_execution` — pre-side-effect guard, audit + provenance emission
//!   (moves the execution to Canceled only before adapter side effects start).
//! - `evaluate_outcome` — PDP outcome evaluation that returns the alignment
//!   verdict (allowed/forbidden vs. actual effect).
//!
//! Scope (Stage 5 — explicit manual commit handler):
//! - `commit_execution` — terminal-state guard, rollback contract `Verified`
//!   guard, `auto_commit=false` guard, `SideEffectVerified` provenance
//!   prerequisite, transition to `Committed`, emit `SideEffectCommitted`
//!   provenance event. R3/manual commit semantics preserved verbatim.
//!
//! Scope (Stage 6 — compensate HTTP handler):
//! - `compensate_execution` — state guard (ExecutedAwaitingVerify contract +
//!   Running/AwaitingVerification execution), HTTP compensation enrichment
//!   before rollback, rollback `compensate` invocation, transition to
//!   `Compensated`, and emit `SideEffectCompensated` provenance event.
//!
//! Scope (Stage 7 — verify HTTP handler):
//! - `verify_execution` — state guard (ExecutedAwaitingVerify contract +
//!   Running/AwaitingVerification execution), rollback `verify` invocation,
//!   conditional `auto_commit` branch (Verified → Committed vs.
//!   Running/AwaitingVerification), `FileHashMatches` expected_hash
//!   injection from `result_digest`, transition to `Verified`/`Failed`,
//!   emit `SideEffectVerified` provenance, and conditional
//!   `SideEffectCommitted` provenance only when verified && auto_commit.
//!
//! Scope (Stage 8 — execute HTTP handler):
//! - `execute_execution` — argument-constraint validation, DraftOnly
//!   defense-in-depth guard, lineage prerequisite gate (Prepared contract
//!   and Prepared/Authorized/Proposed execution), rollback `execute`
//!   invocation, transition to `ExecutedAwaitingVerify` (contract) /
//!   `Running` (execution), `result_digest` propagation, and emit
//!   `ToolCallExecuted` provenance event.
//!
//! Scope (Stage 9 — prepare HTTP handler):
//! - `prepare_execution` — execution/proposal/intent lookup, D1.5
//!   state guard (only `Authorized`/`Prepared` execution states accepted),
//!   DraftOnly intent guard, rollback `prepare` call, rollback contract
//!   insert, execution state update, and emit two provenance events
//!   (`SideEffectPrepared` and `ToolCallPrepared`).
//!
//! Scope (Stage 10 — authorize HTTP handler):
//! - `authorize_execution` — capability load/fallback via
//!   `get_capability_for_authorize`, I5 resource binding subset validation,
//!   I6 approval binding digest validation, durable single-use mark via
//!   `mark_capability_used_durable`, execution insert, and
//!   `ActionProposalSubmitted` provenance emission. Mechanical move; all
//!   invariants preserved verbatim.
//!
//! Out of scope (kept in `server.rs` until later stages):
//! - Non-execution handlers (policy, approval, lineage, admin, monitoring).
//!
//! Out of scope (kept in `server.rs` until later stages):
//! - HTTP handlers for the authorize / prepare lifecycle.
//!   `authorize_execution` is intentionally last due to single-use capability
//!   risk.

mod approval_binding;
mod authorize;
mod cancel;
mod commit;
mod compensate;
mod durable_capability;
mod evaluate_outcome;
mod execute;
mod inference;
mod lifecycle_outbox;
mod prepare;
mod resource_scope;
mod validation;
mod verify;

pub(crate) use crate::provenance::validate_minimum_lineage_chain;
pub(crate) use approval_binding::validate_approval_binding_digest;
pub(crate) use authorize::authorize_execution;
pub(crate) use cancel::cancel_execution;
pub(crate) use commit::commit_execution;
pub(crate) use compensate::compensate_execution;
pub(crate) use durable_capability::classify_authorization_cas_failure;
pub(crate) use durable_capability::get_capability_for_authorize;
#[allow(unused_imports)]
pub(crate) use durable_capability::mark_capability_used_durable;
pub(crate) use evaluate_outcome::evaluate_outcome;
pub(crate) use execute::execute_execution;
pub(crate) use inference::{
    build_prepare_request_for_proposal, enrich_http_compensation_if_needed, infer_rollback_class,
    parse_execution_id,
};
pub(crate) use lifecycle_outbox::{
    execution_is_cancelable_pre_side_effect, execution_is_terminal_for_commit,
    lifecycle_event_metadata, mark_lifecycle_obligation_written,
    mark_lifecycle_transition_reconciled, record_lifecycle_transition_outbox,
    record_lifecycle_transition_outbox_with_obligations,
};
pub(crate) use prepare::prepare_execution;
pub(crate) use resource_scope::validate_resource_bindings_subset_of_scope;
pub(crate) use validation::{validate_argument_constraints, validate_capability_proposal_binding};
pub(crate) use verify::verify_execution;

#[cfg(test)]
pub(crate) use inference::infer_action_type_and_adapter;

#[cfg(test)]
pub(crate) use validation::effective_arguments;

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
