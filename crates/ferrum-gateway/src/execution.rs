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
//! - `cancel_execution` — terminal-state guard, audit + provenance emission
//!   (rolls the execution back to Canceled without invoking the rollback
//!   service).
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

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_cap::{CapabilityError, CapabilityService};
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, ApprovalBinding, ApprovalMode, ApprovalState, AuditAction,
    AuditResourceType, AuthorizeExecutionRequest, AuthorizeExecutionResponse,
    CancelExecutionResponse, CapabilityId, CapabilityLease, CapabilityStatus,
    CommitExecutionResponse, CompensateExecutionResponse, Decision, EvaluateOutcomeResponse,
    EventId, ExecutionId, ExecutionRecord, ExecutionState, HashChainRef, ObjectRef, ObjectType,
    OutcomeReport, PrepareExecutionResponse, ProposalId, ProvenanceEvent, ProvenanceEventKind,
    ProvenanceQueryRequest, ResourceSelector, RollbackClass, RollbackState, RollbackTarget,
};
use ferrum_rollback::RollbackService;
use ferrum_store::StoreFacade;
use std::sync::Arc;

use crate::audit::append_audit;
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::state::AppState;

pub(crate) fn infer_rollback_class(scope: &[ResourceSelector]) -> RollbackClass {
    if scope
        .iter()
        .any(|selector| matches!(selector, ResourceSelector::EmailDraft { .. }))
    {
        RollbackClass::R2Compensatable
    } else {
        RollbackClass::R0NativeReversible
    }
}

/// Validates that `resource_bindings` is a subset of `resource_scope`.
///
/// Returns `Ok(())` if all capability resource bindings are within the intent's
/// resource scope, `Err(reason)` if any binding exceeds the scope.
///
/// Uses conservative prefix semantics: a binding path/uri is within scope if it
/// starts with any matching scope entry's path/uri prefix. For example:
/// - binding path `/tmp/subdir/file.txt` is within scope path `/tmp` ✓
/// - binding path `/other/file.txt` is NOT within scope path `/tmp` ✗
///
/// An empty `resource_bindings` is always valid (represents no specific resources).
/// An empty `resource_scope` with non-empty `resource_bindings` is always invalid.
pub(crate) fn validate_resource_bindings_subset_of_scope(
    resource_bindings: &[ferrum_proto::ResourceBinding],
    resource_scope: &[ResourceSelector],
) -> Result<(), String> {
    // Empty bindings is always valid (no specific resources requested)
    if resource_bindings.is_empty() {
        return Ok(());
    }

    // Empty scope with non-empty bindings = invalid (cannot expand beyond empty scope)
    if resource_scope.is_empty() {
        return Err("resource scope is empty but capability has resource bindings".to_string());
    }

    for binding in resource_bindings {
        let covered = match binding {
            ferrum_proto::ResourceBinding::File { path, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::FilesystemPath {
                        path: scope_path, ..
                    } = scope
                    {
                        path.starts_with(scope_path)
                    } else {
                        false
                    }
                })
            }
            ferrum_proto::ResourceBinding::Git { repo_path, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::GitRepository {
                        repo_path: scope_repo_path,
                        ..
                    } = scope
                    {
                        repo_path.starts_with(scope_repo_path)
                    } else {
                        false
                    }
                })
            }
            ferrum_proto::ResourceBinding::Sqlite { db_path, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::SqliteDatabase {
                        db_path: scope_db_path,
                        ..
                    } = scope
                    {
                        db_path.starts_with(scope_db_path)
                    } else {
                        false
                    }
                })
            }
            ferrum_proto::ResourceBinding::Http {
                base_url,
                path_prefix,
                ..
            } => resource_scope.iter().any(|scope| {
                if let ResourceSelector::HttpEndpoint {
                    base_url: scope_base_url,
                    path_prefix: scope_path_prefix,
                    ..
                } = scope
                {
                    base_url.starts_with(scope_base_url)
                        && path_prefix.starts_with(scope_path_prefix)
                } else {
                    false
                }
            }),
            ferrum_proto::ResourceBinding::EmailDraft { recipients, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::EmailDraft {
                        recipient_allowlist,
                        ..
                    } = scope
                    {
                        // Email matching: recipient must end with an allowlist entry.
                        // E.g., "user@example.com" ends with "@example.com" ✓
                        recipients
                            .iter()
                            .all(|r| recipient_allowlist.iter().any(|a| r.ends_with(a)))
                    } else {
                        false
                    }
                })
            }
        };

        if !covered {
            return Err(format!(
                "capability resource binding {:?} is not within intent resource scope",
                binding
            ));
        }
    }

    Ok(())
}

/// Infers the action_type and adapter_key from the tool_name.
/// For FileWrite-related tools (containing "file_write", "write_file", "fs_", etc.),
/// returns ActionType::FileWrite and adapter_key "fs".
/// For sql_mutate, returns ActionType::SqlMutation and adapter_key "sqlite".
/// For maildraft/draft-create/email_draft tools, returns ActionType::MailDraft and adapter_key "maildraft".
/// For git_branch_create, returns ActionType::GitBranchCreate and adapter_key "git".
/// For git_tag_create, returns ActionType::GitTagCreate and adapter_key "git".
/// For git_branch_delete, returns ActionType::GitBranchDelete and adapter_key "git".
/// For git_tag_delete, returns ActionType::GitTagDelete and adapter_key "git".
/// For git_push, returns ActionType::GitPush and adapter_key "git".
/// For git_pull, returns ActionType::GitPull and adapter_key "git".
/// For git_fetch, returns ActionType::GitFetch and adapter_key "git".
/// Otherwise, defaults to ActionType::McpToolMutation and adapter_key "noop".
pub(crate) fn infer_action_type_and_adapter(tool_name: &str) -> (ferrum_proto::ActionType, String) {
    let tool_lower = tool_name.to_lowercase();
    if tool_lower.contains("file_write")
        || tool_lower.contains("write_file")
        || tool_lower.contains("fs_")
        || tool_lower.contains("file-mutation")
    {
        (ferrum_proto::ActionType::FileWrite, "fs".to_string())
    } else if tool_lower.contains("sql_mutate") {
        (ferrum_proto::ActionType::SqlMutation, "sqlite".to_string())
    } else if tool_lower.contains("maildraft")
        || tool_lower.contains("draft_create")
        || tool_lower.contains("email_draft")
    {
        (ferrum_proto::ActionType::MailDraft, "maildraft".to_string())
    } else if tool_lower.contains("git_branch_create") {
        (ferrum_proto::ActionType::GitBranchCreate, "git".to_string())
    } else if tool_lower.contains("git_tag_create") {
        (ferrum_proto::ActionType::GitTagCreate, "git".to_string())
    } else if tool_lower.contains("git_branch_delete") {
        (ferrum_proto::ActionType::GitBranchDelete, "git".to_string())
    } else if tool_lower.contains("git_tag_delete") {
        (ferrum_proto::ActionType::GitTagDelete, "git".to_string())
    } else if tool_lower.contains("git_push") {
        (ferrum_proto::ActionType::GitPush, "git".to_string())
    } else if tool_lower.contains("git_pull") {
        (ferrum_proto::ActionType::GitPull, "git".to_string())
    } else if tool_lower.contains("git_fetch") {
        (ferrum_proto::ActionType::GitFetch, "git".to_string())
    } else if tool_lower.contains("http_post")
        || tool_lower.contains("http_put")
        || tool_lower.contains("http_patch")
        || tool_lower.contains("http_delete")
    {
        (ferrum_proto::ActionType::HttpMutation, "http".to_string())
    } else {
        (
            ferrum_proto::ActionType::McpToolMutation,
            "noop".to_string(),
        )
    }
}

/// Builds a RollbackPrepareRequest with adapter_key inferred from tool_name.
/// This allows the gateway to select the appropriate adapter based on the proposal's tool.
pub(crate) fn build_prepare_request_for_proposal(
    rollback: &RollbackService,
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
    execution_id: ExecutionId,
    rollback_class: &RollbackClass,
    tool_name: &str,
    resource_scope: &[ferrum_proto::ResourceSelector],
    raw_arguments: &serde_json::Value,
) -> ferrum_proto::RollbackPrepareRequest {
    let (action_type, adapter_key) = infer_action_type_and_adapter(tool_name);
    let target = infer_target_from_scope(resource_scope, &action_type);
    let mut request = rollback.build_prepare_request_with_target(
        intent_id,
        proposal_id,
        execution_id,
        rollback_class.clone(),
        action_type,
        adapter_key,
        target,
    );

    // Merge proposal raw_arguments into metadata for git tools so prepare can
    // validate branch_name/remote_name during prepare (fail-closed).
    if let Some(args) = raw_arguments.as_object() {
        match request.action_type {
            ferrum_proto::ActionType::GitBranchCreate => {
                if let Some(branch) = args.get("branch").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("branch_name".to_string(), serde_json::json!(branch));
                }
            }
            ferrum_proto::ActionType::GitPush
            | ferrum_proto::ActionType::GitPull
            | ferrum_proto::ActionType::GitFetch => {
                if let Some(refspec) = args.get("refspec").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("branch_name".to_string(), serde_json::json!(refspec));
                }
                if let Some(remote) = args.get("remote").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("remote_name".to_string(), serde_json::json!(remote));
                }
            }
            _ => {}
        }
    }

    request
}

/// If the contract has an HTTP placeholder compensation plan (only url present),
/// enrich it with method, payload, and expected_statuses from contract target
/// and metadata so that http.replay_v1 validation succeeds.
/// Fails closed by leaving the contract unchanged when required data is missing.
pub(crate) fn enrich_http_compensation_if_needed(
    mut contract: ferrum_proto::RollbackContract,
) -> ferrum_proto::RollbackContract {
    if contract.adapter_key != "http" || contract.compensation_plan.len() != 1 {
        return contract;
    }
    let step = &contract.compensation_plan[0];
    if step.operation != "http.replay_v1" || step.args.contains_key("method") {
        return contract;
    }

    let method = match &contract.target {
        ferrum_proto::RollbackTarget::HttpRequest { method, .. } => format!("{:?}", method),
        _ => return contract,
    };

    let payload = contract
        .metadata
        .get("execute_payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let expected_statuses: Vec<u16> = contract
        .metadata
        .get("response_status")
        .and_then(|v| v.as_u64())
        .map(|s| vec![s as u16])
        .unwrap_or_else(|| vec![200]);

    let enriched_step = ferrum_proto::CompensationStep {
        order: step.order,
        adapter_key: step.adapter_key.clone(),
        operation: step.operation.clone(),
        idempotency_key: step.idempotency_key.clone(),
        args: {
            let mut args = step.args.clone();
            args.insert("method".to_string(), serde_json::json!(method));
            args.insert("payload".to_string(), payload);
            args.insert(
                "expected_statuses".to_string(),
                serde_json::json!(expected_statuses),
            );
            args
        },
    };

    contract.compensation_plan = vec![enriched_step];
    contract
}

/// Infers the RollbackTarget from resource_scope.
/// For FilesystemPath selectors, returns RollbackTarget::FilePath with the path.
/// For SqliteDatabase selectors with SqlMutation action, returns RollbackTarget::SqliteTxn.
/// For other selectors, returns Generic fallback.
pub(crate) fn infer_target_from_scope(
    scope: &[ferrum_proto::ResourceSelector],
    action_type: &ferrum_proto::ActionType,
) -> RollbackTarget {
    // Only use FilePath target for file-related action types
    let is_file_action = matches!(
        action_type,
        ferrum_proto::ActionType::FileWrite
            | ferrum_proto::ActionType::FileDelete
            | ferrum_proto::ActionType::FileMove
            | ferrum_proto::ActionType::FileCopy
            | ferrum_proto::ActionType::FileAppend
            | ferrum_proto::ActionType::FileChmod
    );

    if is_file_action {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::FilesystemPath {
                path,
                mode: _,
                content_hash: _,
            } = selector
            {
                return RollbackTarget::FilePath {
                    path: path.clone(),
                    before_hash: None,
                    after_hash: None,
                };
            }
        }
    }

    // SqliteDatabase selector for SqlMutation action type
    if matches!(action_type, ferrum_proto::ActionType::SqlMutation) {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::SqliteDatabase {
                db_path,
                tables: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::SqliteTxn {
                    db_path: db_path.clone(),
                    tx_id: format!("tx-{}", uuid::Uuid::new_v4()),
                };
            }
        }
    }

    // EmailDraft selector for MailDraft action type
    if matches!(action_type, ferrum_proto::ActionType::MailDraft) {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::EmailDraft {
                recipient_allowlist,
                subject_prefix_allowlist: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::EmailDraft {
                    draft_id: None, // draft_id is set at runtime by execute
                    recipients: recipient_allowlist.clone(),
                };
            }
        }
    }

    // GitRepository selector for git action types (GitBranchCreate, GitTagCreate, etc.)
    let is_git_action = matches!(
        action_type,
        ferrum_proto::ActionType::GitBranchCreate
            | ferrum_proto::ActionType::GitTagCreate
            | ferrum_proto::ActionType::GitBranchDelete
            | ferrum_proto::ActionType::GitTagDelete
            | ferrum_proto::ActionType::GitPush
            | ferrum_proto::ActionType::GitPull
            | ferrum_proto::ActionType::GitFetch
            | ferrum_proto::ActionType::GitCommit
    );

    if is_git_action {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::GitRepository {
                repo_path,
                allowed_refs: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::GitRef {
                    repo_path: repo_path.clone(),
                    before_ref: None,
                    after_ref: None,
                };
            }
        }
    }

    // HttpEndpoint selector for HttpMutation action type
    if matches!(action_type, ferrum_proto::ActionType::HttpMutation) {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::HttpEndpoint {
                method,
                base_url,
                path_prefix,
                mode: _,
            } = selector
            {
                let url = if path_prefix.starts_with('/') {
                    format!("{}{}", base_url, path_prefix)
                } else {
                    format!("{}/{}", base_url, path_prefix)
                };
                return RollbackTarget::HttpRequest {
                    method: method.clone(),
                    url,
                    request_digest: String::new(),
                };
            }
        }
    }

    // Default fallback
    RollbackTarget::Generic {
        namespace: "mcp".to_string(),
        identifier: "tool-call".to_string(),
    }
}

pub(crate) fn parse_execution_id(value: &str) -> Result<ExecutionId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid execution uuid",
        )
    })?;
    Ok(ExecutionId(parsed))
}

// ---------------------------------------------------------------------------
// Durable capability helpers (Stage 3)
// ---------------------------------------------------------------------------

/// Load capability from in-memory service, falling back to persisted store.
/// Returns NotFound if not found in either.
pub(crate) async fn get_capability_for_authorize(
    cap: &Arc<dyn CapabilityService>,
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> Result<CapabilityLease, CapabilityError> {
    // Try in-memory first
    match cap.get(capability_id).await {
        Ok(lease) => return Ok(lease),
        Err(CapabilityError::NotFound) => {}
        Err(e) => return Err(e),
    }

    // Fall back to persisted store
    let Some(lease) = store
        .capabilities()
        .get(capability_id)
        .await
        .map_err(|_e| CapabilityError::NotFound)?
    // Treat store errors as NotFound for authorize
    else {
        return Err(CapabilityError::NotFound);
    };

    // Validate persisted capability status
    if matches!(lease.status, CapabilityStatus::Used) {
        return Err(CapabilityError::AlreadyUsed);
    }
    if matches!(lease.status, CapabilityStatus::Revoked) {
        return Err(CapabilityError::Revoked);
    }
    if lease.expires_at < Utc::now() {
        return Err(CapabilityError::Expired);
    }

    Ok(lease)
}

/// Mark capability as used in memory and persist the updated status.
/// If the capability is not found in memory, falls back to store and persists there.
pub(crate) async fn mark_capability_used_durable(
    cap: &Arc<dyn CapabilityService>,
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> Result<CapabilityLease, CapabilityError> {
    // Try in-memory mark_used first
    match cap.mark_used(capability_id).await {
        Ok(lease) => {
            // In-memory succeeded; persist the updated lease synchronously
            store.capabilities().update(&lease).await.map_err(|e| {
                tracing::error!(error = %e, "failed to persist used capability status");
                CapabilityError::NotFound // Map to NotFound for API error handling
            })?;
            Ok(lease)
        }
        Err(CapabilityError::NotFound) => {
            // In-memory miss: load from store, validate, then atomically update
            let Some(lease) = store.capabilities().get(capability_id).await.map_err(|e| {
                tracing::error!(error = %e, "failed to load capability from store for mark_used");
                CapabilityError::NotFound
            })?
            else {
                return Err(CapabilityError::NotFound);
            };

            // Validate status before attempting atomic update
            if matches!(lease.status, CapabilityStatus::Used) {
                return Err(CapabilityError::AlreadyUsed);
            }
            if matches!(lease.status, CapabilityStatus::Revoked) {
                return Err(CapabilityError::Revoked);
            }
            if lease.expires_at < Utc::now() {
                return Err(CapabilityError::Expired);
            }

            // Atomically update only if still Active; if another writer won, fail
            let updated = store
                .capabilities()
                .update_status_if_active(capability_id, CapabilityStatus::Used)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "failed to atomically update capability status");
                    CapabilityError::NotFound
                })?;

            if !updated {
                return Err(CapabilityError::AlreadyUsed);
            }

            // Reconstruct the used lease for the caller
            let mut used_lease = lease;
            used_lease.status = CapabilityStatus::Used;
            Ok(used_lease)
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// I6 Approval Binding Digest Validation (Stage 3)
// ---------------------------------------------------------------------------

/// Validates the approval binding digest per I6 invariant.
///
/// Checks when `approval_binding=Some`:
/// 1. Approval exists (404 -> 403 IntegrityMismatch)
/// 2. Approval state is Granted (403 PolicyDenied)
/// 3. Binding not expired (403 PolicyDenied)
/// 4. Approval not expired (403 PolicyDenied)
/// 5. Binding digest matches approval digest (403 IntegrityMismatch)
/// 6. Computed proposal digest matches binding digest (403 IntegrityMismatch)
///
/// Skips all checks when `approval_binding=None` (backward compatible).
pub(crate) async fn validate_approval_binding_digest(
    store: &Arc<dyn StoreFacade>,
    binding: &ApprovalBinding,
    proposal_id: ProposalId,
) -> Result<(), ApiProblem> {
    // Step 1: Fetch the approval by ID
    let approval = store
        .approvals()
        .get(binding.approval_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "approval not found for binding",
            )
        })?;

    // Step 2: Check approval state is Granted
    if !matches!(approval.state, ApprovalState::Granted) {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            format!("approval state is {:?}, expected Granted", approval.state),
        ));
    }

    // Step 3: Check binding not expired
    if binding.expires_at < Utc::now() {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "approval binding has expired",
        ));
    }

    // Step 4: Check approval not expired
    if approval.expires_at < Utc::now() {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "approval has expired",
        ));
    }

    // Step 5: Check binding digest matches approval digest
    if binding.approved_action_digest != approval.action_digest {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::IntegrityMismatch,
            "binding digest does not match approval digest",
        ));
    }

    // Step 6: Fetch proposal and verify computed digest matches binding digest
    let proposal = store
        .proposals()
        .get(proposal_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "proposal not found",
            )
        })?;

    let computed_digest = proposal.canonical_action_digest();
    if computed_digest != binding.approved_action_digest {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::IntegrityMismatch,
            "computed proposal digest does not match binding digest",
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Stage 4 — Low-risk HTTP handlers
// ---------------------------------------------------------------------------

/// `POST /v1/executions/{execution_id}/cancel`
///
/// Cancels a non-terminal execution by transitioning it to `Canceled`,
/// recording an audit entry, and emitting a `SideEffectRolledBack`
/// provenance event so the lineage reflects the cancel as a rollback-like
/// terminal effect.
pub(crate) async fn cancel_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<CancelExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsCancel, e)
    })?;

    // Look up the execution record
    let execution = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                ),
            )
        })?;

    let previous_state = execution.state.clone();

    // ------------------------------------------------------------------
    // Cancel guard: only non-terminal states can be canceled.
    // Terminal states: Verified, Committed, Compensated, RolledBack, Failed,
    //   Expired, Denied, Quarantined
    // Non-terminal states that can be canceled: Proposed, Authorized, Prepared,
    //   Running, AwaitingApproval, AwaitingVerification
    // ------------------------------------------------------------------
    let is_cancelable = matches!(
        previous_state,
        ExecutionState::Proposed
            | ExecutionState::Authorized
            | ExecutionState::Prepared
            | ExecutionState::Running
            | ExecutionState::AwaitingApproval
            | ExecutionState::AwaitingVerification
    );

    if !is_cancelable {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCancel,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "cancel not allowed: execution is in terminal state",
            )
        );
    }

    // Update execution state to Canceled
    let mut updated_execution = execution;
    updated_execution.state = ExecutionState::Canceled;
    updated_execution.finished_at = Some(Utc::now());
    state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Audit log: execution canceled
    append_audit(
        &state.runtime.store,
        "gateway",
        AuditAction::ExecutionCancel,
        AuditResourceType::Execution,
        &execution_id.to_string(),
        "success",
        Some(serde_json::json!({
            "previous_state": format!("{:?}", previous_state),
        })),
    )
    .await;

    // Emit SideEffectRolledBack provenance event for cancel operation.
    // Cancel triggers a rollback-like effect even if no contract exists.
    let cancel_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::SideEffectRolledBack,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Execution canceled".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: updated_execution.rollback_contract_id,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert(
                "previous_state".to_string(),
                serde_json::json!(format!("{:?}", previous_state)),
            );
            m
        },
        source_runtime_id: None,
    };
    state
        .runtime
        .store
        .provenance()
        .append_event(&cancel_event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCancel,
        Ok(Json(CancelExecutionResponse {
            execution_id,
            previous_state,
            current_state: ExecutionState::Canceled,
            canceled_at: Utc::now(),
        }))
    )
}

/// `POST /v1/executions/{execution_id}/evaluate-outcome`
///
/// Validates that the path `execution_id` matches the report, loads the
/// execution and its intent, and delegates the alignment check to the PDP
/// `evaluate_outcome` engine.
pub(crate) async fn evaluate_outcome(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    Json(report): Json<OutcomeReport>,
) -> Result<Json<EvaluateOutcomeResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsEvaluateOutcome, e)
    })?;

    // Validate execution_id matches report
    if report.execution_id != execution_id {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsEvaluateOutcome,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "execution_id in path does not match report",
            )
        );
    }

    // Look up execution to get intent_id
    let execution = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                ),
            )
        })?;

    // Look up intent
    let intent = state
        .runtime
        .store
        .intents()
        .get(execution.intent_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for execution",
                ),
            )
        })?;

    let response = state
        .runtime
        .pdp
        .evaluate_outcome(&intent, &report)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(e),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsEvaluateOutcome,
        Ok(Json(response))
    )
}

/// `POST /v1/executions/{execution_id}/commit`
///
/// Explicit manual commit handler. Transitions a verified execution into the
/// `Committed` terminal state for rollback contracts that were prepared with
/// `auto_commit=false`. This is the R3 manual commit boundary.
///
/// Guards (preserved verbatim from the original `server.rs` implementation):
/// 1. Execution must not already be in a terminal state (`Committed`,
///    `Compensated`, `RolledBack`, `Failed`) → `409 Conflict`.
/// 2. Execution must have a `rollback_contract_id` → `404 Not Found`.
/// 3. The rollback contract must exist → `404 Not Found`.
/// 4. The rollback contract state must be `Verified` → `409 Conflict`.
/// 5. The rollback contract must not have been prepared with `auto_commit=true`
///    (those are auto-committed by verify) → `409 Conflict`.
/// 6. A `SideEffectVerified` provenance event must exist for the execution
///    → `409 Conflict`.
///
/// On success: transitions both the rollback contract and the execution to
/// `Committed`, emits a `SideEffectCommitted` provenance event, and returns
/// the `CommitExecutionResponse`.
pub(crate) async fn commit_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<CommitExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsCommit, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Reject if execution is already in a terminal state.
    if matches!(
        execution.state,
        ExecutionState::Committed
            | ExecutionState::Compensated
            | ExecutionState::RolledBack
            | ExecutionState::Failed
    ) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "commit not allowed: execution is already in terminal state {:?}",
                    execution.state
                ),
            )
        );
    }

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Reject if contract is not Verified.
    if !matches!(contract.state, RollbackState::Verified) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "commit not allowed: rollback contract is in state {:?}, expected Verified",
                    contract.state
                ),
            )
        );
    }

    // Reject if contract was prepared with auto_commit=true (verify already committed).
    if contract.auto_commit {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "commit not allowed: contract was prepared with auto_commit=true \
                 (execution was already auto-committed by verify)",
            )
        );
    }

    // Verify that a SideEffectVerified provenance event exists for this execution.
    let verified_events = match state
        .runtime
        .store
        .provenance()
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectVerified),
            since: None,
            until: None,
            edge_types: vec![],
        })
        .await
    {
        Ok(events) => events,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    if verified_events.is_empty() {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "commit not allowed: no SideEffectVerified provenance event found",
            )
        );
    }

    // Transition both contract and execution to Committed.
    let mut updated_contract = contract;
    updated_contract.state = RollbackState::Committed;
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    let mut updated_execution = execution;
    updated_execution.state = ExecutionState::Committed;
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit SideEffectCommitted provenance event.
    let committed_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::SideEffectCommitted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Side effect committed".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(updated_contract.contract_id),
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&committed_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCommit,
        Ok(Json(CommitExecutionResponse {
            execution_id,
            committed: true,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

/// Compensate a running or awaiting-verification execution by invoking the
/// rollback service's `compensate` action and transitioning the contract and
/// execution to `Compensated`. Emits a `SideEffectCompensated` provenance
/// event for lineage.
///
/// State guard (WS-Compensate):
/// - Contract must be in `ExecutedAwaitingVerify`.
/// - Execution must be in `Running` or `AwaitingVerification`.
///
/// Other states yield a 409 Conflict.
pub(crate) async fn compensate_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<CompensateExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsCompensate, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Compensate state guard
    match (&contract.state, &execution.state) {
        (RollbackState::ExecutedAwaitingVerify, ExecutionState::Running)
        | (RollbackState::ExecutedAwaitingVerify, ExecutionState::AwaitingVerification) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "compensate not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Enrich HTTP placeholder compensation plans before compensate so that
    // parse_replay_contract can validate method/payload/expected_statuses.
    let contract = enrich_http_compensation_if_needed(contract);

    // Call compensate on the contract
    if let Err(e) = state.runtime.rollback.compensate(&contract).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(e)
        );
    }

    // Update contract state to Compensated
    let mut updated_contract = contract.clone();
    updated_contract.state = RollbackState::Compensated;
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Update execution state to Compensated
    let mut updated_execution = execution;
    updated_execution.state = ExecutionState::Compensated;
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit provenance event
    let terminal_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::SideEffectCompensated,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Execution compensated".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(updated_contract.contract_id),
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&terminal_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCompensate,
        Ok(Json(CompensateExecutionResponse {
            execution_id,
            compensated: true,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

/// Verify a previously executed side effect by invoking the rollback
/// service's `verify` action and transitioning the contract and execution
/// based on the outcome. Emits a `SideEffectVerified` provenance event, and
/// conditionally a `SideEffectCommitted` event when verification succeeds
/// and the contract was prepared with `auto_commit=true`.
///
/// State guard (WS-Verify):
/// - Contract must be in `ExecutedAwaitingVerify`.
/// - Execution must be in `Running` or `AwaitingVerification`.
///
/// Other states yield a 409 Conflict.
///
/// D1.6 / R3 branching: when `verified=true && auto_commit=true`, the
/// execution transitions to `Committed` and a `SideEffectCommitted`
/// provenance event is emitted. When `auto_commit=false` (e.g. R3
/// irreversible-high-consequence), the execution remains in its current
/// state awaiting an explicit `/commit` call.
pub(crate) async fn verify_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::VerifyExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsVerify, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Verify guard: contract must be ExecutedAwaitingVerify and execution must be
    // Running or AwaitingVerification. Return 409 Conflict for invalid state transitions.
    match (&contract.state, &execution.state) {
        (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::Running,
        )
        | (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::AwaitingVerification,
        ) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "verify not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Call verify on the adapter via the rollback service.
    // Before calling verify, update FileHashMatches checks with the result_digest
    // so that they can verify post-execute content hash.
    let mut verify_contract = contract.clone();
    if let Some(ref result_digest) = execution.result_digest {
        for check in &mut verify_contract.verify_checks {
            if matches!(check.check_type, ferrum_proto::CheckType::FileHashMatches) {
                check.config.insert(
                    "expected_hash".to_string(),
                    serde_json::json!(result_digest),
                );
            }
        }
        // Also update after_hash on the persisted contract for future reference
        if let ferrum_proto::RollbackTarget::FilePath {
            ref mut after_hash, ..
        } = verify_contract.target
        {
            *after_hash = Some(result_digest.clone());
        }
    }

    let verified = match state.runtime.rollback.verify(&verify_contract).await {
        Ok(verified) => verified,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(e)
            );
        }
    };

    // Update contract state based on verification result.
    // Persist verify_contract (not the original contract) so that verify-time
    // mutations (expected_hash on FileHashMatches checks, after_hash on target)
    // are stored for future inspection.
    let mut updated_contract = verify_contract;
    updated_contract.state = if verified {
        ferrum_proto::RollbackState::Verified
    } else {
        ferrum_proto::RollbackState::Failed
    };
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // D1.6 / R3 enforcement: Only set execution to Committed (and emit SideEffectCommitted)
    // when verified=true AND contract.auto_commit=true. When auto_commit=false, the execution
    // remains in Running/AwaitingVerification state to await explicit commit.
    // This preserves the verified result in contract state while respecting rollback semantics.
    // R3 (irreversible-high-consequence) always sets auto_commit=false at prepare time;
    // verify honors that by suppressing automatic commit. Explicit commit is required for R3.
    let mut updated_execution = execution;
    if verified {
        if updated_contract.auto_commit {
            // auto_commit=true: normal path - execution becomes Committed
            updated_execution.state = ferrum_proto::ExecutionState::Committed;
        } else {
            // auto_commit=false: verified but not committed - keep execution in current state
            // Contract is Verified but execution stays Running/AwaitingVerification
            // This allows explicit commit via separate flow when auto_commit=false
        }
    } else {
        updated_execution.state = ferrum_proto::ExecutionState::Failed;
    }
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit SideEffectVerified provenance event (regardless of verification result).
    let verified_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectVerified,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Side effect verified".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(updated_contract.contract_id),
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("verified".to_string(), serde_json::json!(verified));
            m
        },
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&verified_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit SideEffectCommitted provenance event only when verification succeeded AND auto_commit=true.
    // When auto_commit=false, SideEffectCommitted is suppressed to preserve rollback semantics.
    if verified && updated_contract.auto_commit {
        let committed_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::SideEffectCommitted,
            occurred_at: Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("FerrumGate Gateway".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::RollbackContract,
                object_id: updated_contract.contract_id.to_string(),
                summary: Some("Side effect committed".to_string()),
            },
            intent_id: Some(updated_execution.intent_id),
            proposal_id: Some(updated_execution.proposal_id),
            execution_id: Some(execution_id),
            capability_id: None,
            rollback_contract_id: Some(updated_contract.contract_id),
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
            source_runtime_id: None,
        };
        if let Err(e) = state
            .runtime
            .store
            .provenance()
            .append_event(&committed_event)
            .await
        {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsVerify,
        Ok(Json(ferrum_proto::VerifyExecutionResponse {
            execution_id,
            verified,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

pub(crate) async fn execute_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    Json(request): Json<ferrum_proto::ExecuteExecutionRequest>,
) -> Result<Json<ferrum_proto::ExecuteExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsExecute, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS3: Defense-in-depth — enforce draft-only guard at execute checkpoint.
    // Look up the intent and reject execution if the intent enforces draft-only mode.
    // This is defense-in-depth; prepare already blocks DraftOnly, but execute also
    // guards against any path that might bypass prepare.
    let intent = match state.runtime.store.intents().get(execution.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if matches!(intent.approval_mode, ApprovalMode::DraftOnly) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "draft-only intent cannot proceed to execute",
            )
        );
    }

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Execute guard: contract must be Prepared and execution must be Prepared or Authorized.
    // Return 409 Conflict for invalid state transitions.
    match (&contract.state, &execution.state) {
        (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Prepared)
        | (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Authorized)
        | (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Proposed) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "execute not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Call execute on the adapter via the rollback service
    let receipt = match state
        .runtime
        .rollback
        .execute(&contract, &request.payload)
        .await
    {
        Ok(receipt) => receipt,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(e)
            );
        }
    };

    // Update contract state to ExecutedAwaitingVerify and capture after_hash from
    // the execute receipt so after_hash is available for inspection immediately
    // after execute (before verify has run).
    let mut updated_contract = contract.clone();
    updated_contract.state = ferrum_proto::RollbackState::ExecutedAwaitingVerify;
    if let ferrum_proto::RollbackTarget::FilePath {
        ref mut after_hash, ..
    } = updated_contract.target
    {
        *after_hash = receipt.result_digest.clone();
    }
    // For HTTP targets, propagate request_digest from execute receipt into target
    // so that compensation replay can validate digest matching.
    if let ferrum_proto::RollbackTarget::HttpRequest {
        ref mut request_digest,
        ..
    } = updated_contract.target
    {
        if let Some(digest) = receipt
            .adapter_metadata
            .get("request_digest")
            .and_then(|v| v.as_str())
        {
            *request_digest = digest.to_string();
        }
    }
    // Propagate adapter_metadata from execute receipt into contract metadata so that
    // rollback/compensate can access critical fields (e.g., branch_name for GitBranchCreate).
    for (key, value) in &receipt.adapter_metadata {
        updated_contract.metadata.insert(key.clone(), value.clone());
    }
    // Store execute payload for later compensation enrichment (HTTP replay).
    updated_contract
        .metadata
        .insert("execute_payload".to_string(), request.payload.clone());
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Update execution state to Running
    let mut updated_execution = execution;
    updated_execution.state = ferrum_proto::ExecutionState::Running;
    updated_execution.result_digest = receipt.result_digest.clone();
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit ToolCallExecuted provenance event.
    let tool_executed_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ToolCallExecuted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Tool call executed".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: updated_execution.rollback_contract_id,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&tool_executed_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsExecute,
        Ok(Json(ferrum_proto::ExecuteExecutionResponse {
            execution_id,
            executed: true,
            result_digest: receipt.result_digest,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

// ---------------------------------------------------------------------------
// Stage 9 — Prepare HTTP handler
// ---------------------------------------------------------------------------

/// `POST /v1/executions/{execution_id}/prepare`
///
/// Prepares an execution by invoking the rollback service's `prepare` action
/// to mint a rollback contract. The handler enforces:
///
/// 1. D1.5 state guard — only `Authorized` or `Prepared` execution states
///    may transition to `Prepared`; all other states return 409 Conflict.
/// 2. DraftOnly intent guard — if the intent enforces `ApprovalMode::DraftOnly`,
///    prepare is rejected with 403 PolicyDenied (defense-in-depth in addition
///    to `evaluate` short-circuiting at this mode).
/// 3. Rollback contract insert — the contract from `rollback.prepare` is
///    persisted and the execution's `rollback_contract_id` is updated.
/// 4. Two provenance events — `SideEffectPrepared` and `ToolCallPrepared` —
///    are emitted (parent edges empty for both, mirroring the Q1-P5
///    conservative prepare chain).
pub(crate) async fn prepare_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<PrepareExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsPrepare, e);
        }
    };

    // Look up the existing execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // D1.5 mandatory: Reject prepare for non-preparable execution states.
    // Only Authorized or Prepared executions can transition to Prepared.
    // All other states (Proposed, Running, Committed, Compensated, etc.) return 409 Conflict.
    match execution.state {
        ExecutionState::Authorized | ExecutionState::Prepared => {
            // Valid state - proceed with prepare
        }
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "execution in state '{:?}' cannot be prepared; only '{:?}' or '{:?}' are preparable",
                        execution.state,
                        ExecutionState::Authorized,
                        ExecutionState::Prepared
                    ),
                )
            );
        }
    }

    // Look up the proposal to retrieve the real rollback_class.
    // The proposal is the most reliable existing linked record for this execution.
    let proposal = match state
        .runtime
        .store
        .proposals()
        .get(execution.proposal_id)
        .await
    {
        Ok(Some(proposal)) => proposal,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "proposal not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    let rollback_class = proposal.requested_rollback_class.clone();

    // WS3: Enforce draft-only guard at prepare checkpoint.
    // Look up the intent and reject preparation if the intent enforces draft-only mode.
    // This prevents a draft-only intent from bypassing evaluate and reaching prepare.
    let intent = match state.runtime.store.intents().get(execution.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if matches!(intent.approval_mode, ApprovalMode::DraftOnly) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "draft-only intent cannot proceed to prepare",
            )
        );
    }

    let request = build_prepare_request_for_proposal(
        &state.runtime.rollback,
        execution.intent_id,
        execution.proposal_id,
        execution_id,
        &rollback_class,
        &proposal.tool_name,
        &intent.resource_scope,
        &proposal.raw_arguments,
    );

    let response = match state.runtime.rollback.prepare(request).await {
        Ok(response) => response,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(e)
            );
        }
    };

    // Store the contract in the database
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .insert(&response.contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Capture execution IDs for provenance before moving into updated_execution
    let execution_intent_id = execution.intent_id;
    let execution_proposal_id = execution.proposal_id;

    // Link the contract to the execution by updating rollback_contract_id
    let mut updated_execution = execution;
    updated_execution.rollback_contract_id = Some(response.contract.contract_id);
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit provenance event for preparation (Q1-P5 conservative chain: prepare).
    let prepare_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectPrepared,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: response.contract.contract_id.to_string(),
            summary: Some("Execution prepared with rollback contract".to_string()),
        },
        intent_id: Some(execution_intent_id),
        proposal_id: Some(execution_proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(response.contract.contract_id),
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&prepare_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit ToolCallPrepared provenance event.
    let tool_prepared_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ToolCallPrepared,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Tool call prepared for execution".to_string()),
        },
        intent_id: Some(execution_intent_id),
        proposal_id: Some(execution_proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(response.contract.contract_id),
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&tool_prepared_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsPrepare,
        Ok(Json(PrepareExecutionResponse {
            execution_id,
            prepared: response.accepted,
            rollback_contract: Some(response.contract),
            warnings: response.warnings,
        }))
    )
}

// ---------------------------------------------------------------------------
// Stage 10 — Authorize HTTP handler (single-use capability gate)
// ---------------------------------------------------------------------------

/// `POST /v1/executions/authorize`
///
/// Authorize an execution by consuming its single-use capability. The handler
/// enforces the capability gate in this exact order:
///
/// 1. Load capability from in-memory service, falling back to persisted store
///    via `get_capability_for_authorize`.
/// 2. Binding invariant — reject (403 IntegrityMismatch) if
///    `request.proposal_id != lease.proposal_id` before any durable
///    capability mutation.
/// 3. I5 invariant — validate that capability `resource_bindings` is a subset
///    of the intent's `resource_scope`.
/// 4. I6 invariant — if the capability has an `approval_binding`, validate the
///    approval binding digest (and the proposal's canonical action digest)
///    against the binding. Skipped when `approval_binding=None`.
/// 5. Mark the capability as used in memory and persist the updated status
///    via `mark_capability_used_durable`. Returns `AlreadyUsed` if the
///    capability has already been consumed (single-use enforcement).
/// 6. Insert an `ExecutionRecord` (state `Authorized` for dry-run, `Prepared`
///    otherwise) and emit an `ActionProposalSubmitted` provenance event.
///
/// All guards, ordering, status codes, and the response schema are preserved
/// verbatim from the original `server.rs` implementation.
pub(crate) async fn authorize_execution(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AuthorizeExecutionRequest>,
) -> Result<Json<AuthorizeExecutionResponse>, ApiProblem> {
    // Load capability from in-memory service, falling back to persisted store.
    // This ensures capability survives in-memory state loss.
    let lease = match get_capability_for_authorize(
        &state.runtime.cap,
        &state.runtime.store,
        request.capability_id,
    )
    .await
    {
        Ok(lease) => lease,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(e)
            );
        }
    };

    // Binding invariant: the request's proposal_id MUST match the lease's
    // proposal_id. This prevents a holder of one capability from using it to
    // authorize an unrelated proposal, and ensures the durable single-use
    // mark below only fires for a matched (capability, proposal) pair.
    if request.proposal_id != lease.proposal_id {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "request proposal_id does not match capability lease proposal_id",
            )
        );
    }

    // I5: Validate that capability resource_bindings is a subset of intent resource_scope.
    // This prevents a capability from expanding beyond the intent's authorized scope.
    let intent = match state.runtime.store.intents().get(lease.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for capability",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if let Err(scope_violation) =
        validate_resource_bindings_subset_of_scope(&lease.resource_bindings, &intent.resource_scope)
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                scope_violation,
            )
        );
    }

    // I6: Validate approval binding digest if present.
    // This ensures the proposal digest matches the approved action digest.
    // Skipped when approval_binding=None (backward compatible).
    if let Some(ref binding) = lease.approval_binding {
        validate_approval_binding_digest(&state.runtime.store, binding, request.proposal_id)
            .await
            .map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::ExecutionsAuthorize, e)
            })?;
    }

    // Mark the capability as used - returns AlreadyUsed if already consumed.
    // This enforces single-use: first authorize succeeds, subsequent ones fail.
    // Persists the updated status to store for durability.
    match mark_capability_used_durable(
        &state.runtime.cap,
        &state.runtime.store,
        request.capability_id,
    )
    .await
    {
        Ok(lease) => lease,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(e)
            );
        }
    };

    let record = ExecutionRecord {
        execution_id: ExecutionId::new(),
        proposal_id: request.proposal_id,
        intent_id: lease.intent_id,
        capability_id: lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: if request.dry_run {
            ExecutionState::Authorized
        } else {
            ExecutionState::Prepared
        },
        started_at: Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    // Persist the execution record so subsequent prepare/execute can find it.
    // Write-queue ensures serialized writes - no more SQLite lock contention.
    if let Err(e) = state.runtime.store.executions().insert(&record).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit provenance event for authorization (Q1-P5 conservative chain: authorize).
    let auth_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ActionProposalSubmitted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: record.execution_id.to_string(),
            summary: Some("Execution authorized".to_string()),
        },
        intent_id: Some(record.intent_id),
        proposal_id: Some(record.proposal_id),
        execution_id: Some(record.execution_id),
        capability_id: Some(record.capability_id),
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&auth_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsAuthorize,
        Ok(Json(AuthorizeExecutionResponse {
            execution: record,
            warnings: Vec::new(),
        }))
    )
}
