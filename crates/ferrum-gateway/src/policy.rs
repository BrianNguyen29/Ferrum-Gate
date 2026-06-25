//! Policy bundle governance handlers.
//!
//! Routes:
//! - `POST   /v1/policy-bundles`                            -> `create_policy_bundle`
//! - `GET    /v1/policy-bundles`                            -> `list_policy_bundles`
//! - `GET    /v1/policy-bundles/{bundle_id}`                -> `get_policy_bundle`
//! - `PUT    /v1/policy-bundles/{bundle_id}`                -> `update_policy_bundle`
//! - `DELETE /v1/policy-bundles/{bundle_id}`                -> `delete_policy_bundle`
//! - `PUT    /v1/policy-bundles/{bundle_id}/active`         -> `set_policy_bundle_active`
//! - `POST   /v1/policy/simulate`                           -> `simulate_policy`
//! - `POST   /v1/policy-bundles/simulate`                   -> `simulate_policy_bundle`
//! - `GET    /v1/policy-bundles/{bundle_id}/versions`       -> `list_policy_bundle_versions`
//! - `GET    /v1/policy-bundles/{bundle_id}/diff`           -> `diff_policy_bundle_versions`
//! - `POST   /v1/policy-bundles/{bundle_id}/rollback`       -> `rollback_policy_bundle`
//!
//! All success paths increment the `GovernanceRoute` counter and apply the
//! output sanitizer to the response payload. Policy evaluation helpers
//! (`evaluate_active_policy_bundles`, `evaluate_bundle_rules`, ...) are
//! imported from `crate::policy_eval` since they are shared with
//! `proposals::evaluate_proposal`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, Decision, DiffPolicyBundleVersionsResponse,
    EvaluateProposalResponse, EventId, HashChainRef, ListPolicyBundleVersionsResponse, ObjectRef,
    ObjectType, PolicyBundleId, PolicyBundleResponse, PolicyBundleSimulateRequest,
    PolicyBundleSimulateResponse, PolicySimulateRequest, ProvenanceEvent, ProvenanceEventKind,
    RollbackPolicyBundleRequest, RollbackPolicyBundleResponse, TrustContextSummary,
    parse_policy_bundle_yaml,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit;
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::policy_eval::{
    build_firewall_context, evaluate_active_policy_bundles, evaluate_bundle_rules,
    has_tool_output_label, has_untrusted_text_label, intent_has_external_label, minimal_intent_for,
    proposal_has_external_metadata,
};
use crate::problem::ApiProblem;
use crate::response::sanitize_json;
use crate::state::AppState;

pub(crate) async fn create_policy_bundle(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ferrum_proto::CreatePolicyBundleRequest>,
) -> Result<Json<ferrum_proto::PolicyBundleResponse>, ApiProblem> {
    // Parse and validate the YAML
    let bundle = parse_policy_bundle_yaml(&request.yaml_content).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::PolicyBundlesCreate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("invalid policy bundle YAML: {}", e),
            ),
        )
    })?;

    let content_hash = bundle.content_hash.clone().unwrap_or_default();

    // Check for idempotency: if a bundle with the same content hash exists, return it
    if let Ok(Some(existing)) = state
        .runtime
        .store
        .policy_bundles()
        .get_by_content_hash(&content_hash)
        .await
    {
        return governance_ok!(
            state,
            GovernanceRoute::PolicyBundlesCreate,
            Ok(Json(ferrum_proto::PolicyBundleResponse {
                bundle: existing,
                content_hash,
            }))
        );
    }

    // Insert the new bundle
    state
        .runtime
        .store
        .policy_bundles()
        .insert(&bundle)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesCreate,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Audit log: policy bundle created
    if let Err(problem) = audit::append_audit_checked(
        &state,
        "gateway",
        ferrum_proto::AuditAction::PolicyBundleCreate,
        ferrum_proto::AuditResourceType::PolicyBundle,
        &bundle.bundle_id,
        "success",
        Some(json!({
            "version": bundle.version,
            "content_hash": content_hash,
        })),
        Some(GovernanceRoute::PolicyBundlesCreate),
    )
    .await
    {
        return governance_err!(state, GovernanceRoute::PolicyBundlesCreate, problem);
    }

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesCreate,
        Ok(Json(ferrum_proto::PolicyBundleResponse {
            bundle,
            content_hash,
        }))
    )
}

pub(crate) async fn list_policy_bundles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ferrum_proto::PolicyBundleListResponse>, ApiProblem> {
    let bundles = state
        .runtime
        .store
        .policy_bundles()
        .list()
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesList,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    let total = bundles.len();
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesList,
        Ok(Json(ferrum_proto::PolicyBundleListResponse {
            bundles,
            total,
        }))
    )
}

pub(crate) async fn get_policy_bundle(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
) -> Result<Json<PolicyBundleResponse>, ApiProblem> {
    let bundle = state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesGet,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesGet,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    format!("policy bundle '{}' not found", bundle_id),
                ),
            )
        })?;

    let content_hash = bundle.content_hash.clone().unwrap_or_default();
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesGet,
        Ok(Json(PolicyBundleResponse {
            bundle,
            content_hash,
        }))
    )
}

pub(crate) async fn update_policy_bundle(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
    Json(request): Json<ferrum_proto::UpdatePolicyBundleRequest>,
) -> Result<Json<PolicyBundleResponse>, ApiProblem> {
    // Parse and validate the YAML
    let mut bundle = parse_policy_bundle_yaml(&request.yaml_content).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::PolicyBundlesUpdate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("invalid policy bundle YAML: {}", e),
            ),
        )
    })?;

    // Ensure the bundle_id matches the path
    if bundle.bundle_id != bundle_id {
        return governance_err!(
            state,
            GovernanceRoute::PolicyBundlesUpdate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!(
                    "bundle_id in YAML ('{}') does not match path ('{}')",
                    bundle.bundle_id, bundle_id
                ),
            )
        );
    }

    // Get existing bundle to preserve created_at and check existence
    let existing = state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesUpdate,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesUpdate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    format!("policy bundle '{}' not found", bundle_id),
                ),
            )
        })?;

    // Preserve created_at and update updated_at
    bundle.created_at = existing.created_at;
    bundle.updated_at = chrono::Utc::now();

    // Recompute content hash
    let content_hash = bundle.compute_content_hash();
    bundle.content_hash = Some(content_hash.clone());

    state
        .runtime
        .store
        .policy_bundles()
        .update(&bundle)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesUpdate,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesUpdate,
        Ok(Json(PolicyBundleResponse {
            bundle,
            content_hash,
        }))
    )
}

pub(crate) async fn delete_policy_bundle(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    // Check if bundle exists
    let not_found = {
        let msg = format!("policy bundle '{}' not found", bundle_id);
        state.runtime.firewall.sanitize(&msg)
    };
    state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesDelete,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesDelete,
                ApiProblem::new(StatusCode::NOT_FOUND, ApiErrorCode::NotFound, not_found),
            )
        })?;

    state
        .runtime
        .store
        .policy_bundles()
        .delete(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesDelete,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    let response = json!({ "ok": true, "bundle_id": bundle_id });
    let sanitized = sanitize_json(&state.runtime.firewall, response);
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesDelete,
        Ok(Json(sanitized))
    )
}

pub(crate) async fn set_policy_bundle_active(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
    Json(request): Json<ferrum_proto::SetPolicyBundleActiveRequest>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    // Check if bundle exists
    let not_found = {
        let msg = format!("policy bundle '{}' not found", bundle_id);
        state.runtime.firewall.sanitize(&msg)
    };
    state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesSetActive,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesSetActive,
                ApiProblem::new(StatusCode::NOT_FOUND, ApiErrorCode::NotFound, not_found),
            )
        })?;

    state
        .runtime
        .store
        .policy_bundles()
        .set_active(&bundle_id, request.active)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesSetActive,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Audit log: policy bundle activated/deactivated
    if let Err(problem) = audit::append_audit_checked(
        &state,
        "gateway",
        ferrum_proto::AuditAction::PolicyBundleActivate,
        ferrum_proto::AuditResourceType::PolicyBundle,
        &bundle_id,
        "success",
        Some(json!({
            "active": request.active,
        })),
        Some(GovernanceRoute::PolicyBundlesSetActive),
    )
    .await
    {
        return governance_err!(state, GovernanceRoute::PolicyBundlesSetActive, problem);
    }

    // Emit provenance event for policy bundle activation/deactivation (POL-4)
    let policy_bundle_id = uuid::Uuid::parse_str(&bundle_id).ok().map(PolicyBundleId);
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert("active".to_string(), json!(request.active));
    let provenance_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: if request.active {
            ProvenanceEventKind::PolicyBundleActivated
        } else {
            ProvenanceEventKind::PolicyBundleDeactivated
        },
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "gateway".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::PolicyBundle,
            object_id: bundle_id.clone(),
            summary: None,
        },
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata,
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&provenance_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::PolicyBundlesSetActive,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    let response = json!({
        "ok": true,
        "bundle_id": bundle_id,
        "active": request.active
    });
    let sanitized = sanitize_json(&state.runtime.firewall, response);
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesSetActive,
        Ok(Json(sanitized))
    )
}

/// Simulate evaluation against the active runtime policy without side effects.
/// No proposal, intent, bundle, or provenance is persisted.
pub(crate) async fn simulate_policy(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PolicySimulateRequest>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    // Build or use provided intent
    let intent = request.intent.unwrap_or_else(|| {
        minimal_intent_for(
            request.proposal.intent_id,
            request.proposal.requested_rollback_class.clone(),
        )
    });

    // Determine if proposal is external based on intent trust labels and proposal attributes.
    let is_external = intent_has_external_label(&intent)
        || !request.proposal.taint_inputs.is_empty()
        || proposal_has_external_metadata(&request.proposal);

    // Build firewall context from proposal and intent.
    let firewall_ctx = build_firewall_context(&intent, &request.proposal, is_external);

    // Compute taint score via firewall.
    let firewall_taint = state.runtime.firewall.compute_taint_score(&firewall_ctx);

    // Preserve intent's trust labels and sensitivity labels; override taint_score with firewall-derived value.
    let trust = TrustContextSummary {
        input_labels: intent.trust_context.input_labels.clone(),
        sensitivity_labels: intent.trust_context.sensitivity_labels.clone(),
        taint_score: firewall_taint,
        contains_external_metadata: intent.trust_context.contains_external_metadata
            || proposal_has_external_metadata(&request.proposal),
        contains_tool_output: intent.trust_context.contains_tool_output
            || has_tool_output_label(&intent),
        contains_untrusted_text: intent.trust_context.contains_untrusted_text
            || has_untrusted_text_label(&intent),
    };

    // Evaluate against active policy bundles, then fall back to PDP.
    // No persistence, no provenance emission, no capability minting.
    let out = if let Some(bundle_response) =
        evaluate_active_policy_bundles(&state.runtime.store, &intent, &request.proposal, &trust)
            .await
    {
        bundle_response
    } else {
        match state
            .runtime
            .pdp
            .evaluate(&intent, &request.proposal, &trust)
            .await
        {
            Ok(out) => out,
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::PolicySimulate,
                    ApiProblem::internal(e)
                );
            }
        }
    };

    governance_ok!(state, GovernanceRoute::PolicySimulate, Ok(Json(out)))
}

/// Simulate a policy bundle against a sample proposal without side effects.
/// No proposal, bundle, or provenance is persisted.
pub(crate) async fn simulate_policy_bundle(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PolicyBundleSimulateRequest>,
) -> Result<Json<PolicyBundleSimulateResponse>, ApiProblem> {
    // Parse the bundle YAML
    let bundle = parse_policy_bundle_yaml(&request.bundle_yaml).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::PolicyBundlesSimulate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("invalid policy bundle YAML: {}", e),
            ),
        )
    })?;

    // Build or use provided intent
    let intent = request.intent.unwrap_or_else(|| {
        minimal_intent_for(
            request.proposal.intent_id,
            request.proposal.requested_rollback_class.clone(),
        )
    });

    // Determine if proposal is external based on intent trust labels and proposal attributes.
    let is_external = intent_has_external_label(&intent)
        || !request.proposal.taint_inputs.is_empty()
        || proposal_has_external_metadata(&request.proposal);

    // Build firewall context from proposal and intent.
    let firewall_ctx = build_firewall_context(&intent, &request.proposal, is_external);

    // Compute taint score via firewall.
    let firewall_taint = state.runtime.firewall.compute_taint_score(&firewall_ctx);

    // Preserve intent's trust labels and sensitivity labels; override taint_score with firewall-derived value.
    let trust = TrustContextSummary {
        input_labels: intent.trust_context.input_labels.clone(),
        sensitivity_labels: intent.trust_context.sensitivity_labels.clone(),
        taint_score: firewall_taint,
        contains_external_metadata: intent.trust_context.contains_external_metadata
            || proposal_has_external_metadata(&request.proposal),
        contains_tool_output: intent.trust_context.contains_tool_output
            || has_tool_output_label(&intent),
        contains_untrusted_text: intent.trust_context.contains_untrusted_text
            || has_untrusted_text_label(&intent),
    };

    // Evaluate the provided bundle rules against the sample context.
    let response = evaluate_bundle_rules(&bundle, &intent, &request.proposal, &trust)
        .map(|eval| PolicyBundleSimulateResponse {
            decision: eval.decision,
            reason: eval.reason,
            matched_rule_ids: eval.matched_rule_ids,
            warnings: eval.warnings,
        })
        .unwrap_or_else(|| PolicyBundleSimulateResponse {
            decision: Decision::Allow,
            reason: "no rules matched in the provided bundle".to_string(),
            matched_rule_ids: Vec::new(),
            warnings: Vec::new(),
        });

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesSimulate,
        Ok(Json(response))
    )
}

pub(crate) async fn list_policy_bundle_versions(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(bundle_id): axum::extract::Path<String>,
) -> Result<Json<ListPolicyBundleVersionsResponse>, ApiProblem> {
    let versions = state
        .runtime
        .store
        .policy_bundles()
        .list_versions(&bundle_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    let total = versions.len();
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesVersions,
        Ok(Json(ListPolicyBundleVersionsResponse { versions, total }))
    )
}

/// Compute a structural JSON diff between two serde_json::Value trees.
/// Returns a JSON object with keys "added", "removed", "changed".
fn json_diff(left: &serde_json::Value, right: &serde_json::Value) -> serde_json::Value {
    let mut added = serde_json::Map::new();
    let mut removed = serde_json::Map::new();
    let mut changed = serde_json::Map::new();

    match (left, right) {
        (serde_json::Value::Object(lm), serde_json::Value::Object(rm)) => {
            for (k, lv) in lm {
                match rm.get(k) {
                    Some(rv) if lv != rv => {
                        let child = json_diff(lv, rv);
                        if !child.as_object().map(|o| o.is_empty()).unwrap_or(false) {
                            changed.insert(k.clone(), child);
                        }
                    }
                    None => {
                        removed.insert(k.clone(), lv.clone());
                    }
                    _ => {}
                }
            }
            for (k, rv) in rm {
                if !lm.contains_key(k) {
                    added.insert(k.clone(), rv.clone());
                }
            }
        }
        (serde_json::Value::Array(la), serde_json::Value::Array(ra)) => {
            let max_len = la.len().max(ra.len());
            for i in 0..max_len {
                match (la.get(i), ra.get(i)) {
                    (Some(lv), Some(rv)) if lv != rv => {
                        let child = json_diff(lv, rv);
                        changed.insert(i.to_string(), child);
                    }
                    (Some(lv), None) => {
                        removed.insert(i.to_string(), lv.clone());
                    }
                    (None, Some(rv)) => {
                        added.insert(i.to_string(), rv.clone());
                    }
                    _ => {}
                }
            }
        }
        _ if left != right => {
            changed.insert("_old".to_string(), left.clone());
            changed.insert("_new".to_string(), right.clone());
        }
        _ => {}
    }

    let mut result = serde_json::Map::new();
    if !added.is_empty() {
        result.insert("added".to_string(), serde_json::Value::Object(added));
    }
    if !removed.is_empty() {
        result.insert("removed".to_string(), serde_json::Value::Object(removed));
    }
    if !changed.is_empty() {
        result.insert("changed".to_string(), serde_json::Value::Object(changed));
    }
    serde_json::Value::Object(result)
}

pub(crate) async fn diff_policy_bundle_versions(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(bundle_id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<DiffPolicyBundleVersionsResponse>, ApiProblem> {
    let from_version: i64 = params
        .get("from")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "missing or invalid 'from' query parameter",
            )
        })?;
    let to_version: i64 = params
        .get("to")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "missing or invalid 'to' query parameter",
            )
        })?;

    let repo = state.runtime.store.policy_bundles();
    let from_v = repo
        .get_version(&bundle_id, from_version)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                format!(
                    "version {} not found for bundle {}",
                    from_version, bundle_id
                ),
            )
        })?;
    let to_v = repo
        .get_version(&bundle_id, to_version)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                format!("version {} not found for bundle {}", to_version, bundle_id),
            )
        })?;

    let left = serde_json::to_value(&from_v.content).unwrap_or_default();
    let right = serde_json::to_value(&to_v.content).unwrap_or_default();
    let diff = json_diff(&left, &right);

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesDiff,
        Ok(Json(DiffPolicyBundleVersionsResponse {
            bundle_id,
            from_version,
            to_version,
            diff,
        }))
    )
}

pub(crate) async fn rollback_policy_bundle(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(bundle_id): axum::extract::Path<String>,
    Json(request): Json<RollbackPolicyBundleRequest>,
) -> Result<Json<RollbackPolicyBundleResponse>, ApiProblem> {
    let repo = state.runtime.store.policy_bundles();

    // Get current version number before rollback
    let versions = repo
        .list_versions(&bundle_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;
    let previous_version = versions.iter().map(|v| v.version).max().unwrap_or(0);

    let new_version = repo
        .rollback(&bundle_id, request.target_version, request.actor.as_deref())
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    // Audit log: policy bundle rollback
    if let Err(problem) = audit::append_audit_checked(
        &state,
        request.actor.as_deref().unwrap_or("unknown"),
        ferrum_proto::AuditAction::PolicyBundleRollback,
        ferrum_proto::AuditResourceType::PolicyBundle,
        &bundle_id,
        "success",
        Some(json!({
            "previous_version": previous_version,
            "new_version": new_version,
            "rolled_back_to_version": request.target_version,
        })),
        Some(GovernanceRoute::PolicyBundlesRollback),
    )
    .await
    {
        return governance_err!(state, GovernanceRoute::PolicyBundlesRollback, problem);
    }

    // Emit provenance event
    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::PolicyBundleRolledBack,
        occurred_at: chrono::Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: request
                .actor
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            display_name: request.actor.clone(),
        },
        object: ObjectRef {
            object_type: ObjectType::PolicyBundle,
            object_id: bundle_id.clone(),
            summary: Some(format!(
                "Rollback from v{} to v{}",
                previous_version, new_version
            )),
        },
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: uuid::Uuid::parse_str(&bundle_id).ok().map(PolicyBundleId),
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
            m.insert("from_version".to_string(), json!(previous_version));
            m.insert("to_version".to_string(), json!(new_version));
            m.insert(
                "rolled_back_to_version".to_string(),
                json!(request.target_version),
            );
            m
        },
        source_runtime_id: None,
    };

    state
        .runtime
        .store
        .provenance()
        .append_event(&event)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesRollback,
        Ok(Json(RollbackPolicyBundleResponse {
            bundle_id,
            new_version,
            previous_version,
            rolled_back_to_version: request.target_version,
        }))
    )
}
