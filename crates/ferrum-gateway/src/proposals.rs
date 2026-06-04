//! Action proposal evaluation handler.
//!
//! Routes:
//! - `POST /v1/proposals/{proposal_id}/evaluate` -> `evaluate_proposal`
//!
//! `evaluate_proposal` loads the proposal's parent intent, builds the
//! firewall context, runs the policy bundle evaluation chain (active
//! policy bundles first, static PDP as fallback), persists the proposal
//! to satisfy foreign-key constraints, and emits a `PolicyEvaluated`
//! provenance event before returning the decision.

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, EvaluateProposalResponse, EventId, HashChainRef, ObjectRef, ObjectType,
    ProvenanceEvent, TrustContextSummary,
};
use std::sync::Arc;

use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::policy_eval::{
    build_firewall_context, evaluate_active_policy_bundles, has_tool_output_label,
    has_untrusted_text_label, intent_has_external_label, minimal_intent_for,
    proposal_has_external_metadata,
};
use crate::problem::ApiProblem;
use crate::state::AppState;

pub(crate) async fn evaluate_proposal(
    State(state): State<Arc<AppState>>,
    Path(_proposal_id): Path<String>,
    Json(proposal): Json<ferrum_proto::ActionProposal>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    let intent = match state.runtime.store.intents().get(proposal.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => minimal_intent_for(
            proposal.intent_id,
            proposal.requested_rollback_class.clone(),
        ),
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ProposalsEvaluate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Determine if proposal is external based on intent trust labels and proposal attributes.
    let is_external = intent_has_external_label(&intent)
        || !proposal.taint_inputs.is_empty()
        || proposal_has_external_metadata(&proposal);

    // Build firewall context from proposal and intent.
    let firewall_ctx = build_firewall_context(&intent, &proposal, is_external);

    // Compute taint score via firewall.
    let firewall_taint = state.runtime.firewall.compute_taint_score(&firewall_ctx);

    // Preserve intent's trust labels and sensitivity labels; override taint_score with firewall-derived value.
    let trust = TrustContextSummary {
        input_labels: intent.trust_context.input_labels.clone(),
        sensitivity_labels: intent.trust_context.sensitivity_labels.clone(),
        taint_score: firewall_taint,
        contains_external_metadata: intent.trust_context.contains_external_metadata
            || proposal_has_external_metadata(&proposal),
        contains_tool_output: intent.trust_context.contains_tool_output
            || has_tool_output_label(&intent),
        contains_untrusted_text: intent.trust_context.contains_untrusted_text
            || has_untrusted_text_label(&intent),
    };

    // Check active policy bundles before falling back to PDP.
    // Design: static PDP is the baseline evaluator. Active policy bundles add supplemental
    // constraints on top of PDP. Zero active bundles is an intentional fallback for dev
    // and conditional pilot scenarios. Production operators may later choose a required-bundle
    // policy that rejects when no bundle matches, but the current default is permissive.
    // Use firewall-derived trust context for bundle evaluation to properly assess taint and other trust attributes.
    let out = if let Some(bundle_response) =
        evaluate_active_policy_bundles(&state.runtime.store, &intent, &proposal, &trust).await
    {
        let out = bundle_response;
        // Persist the proposal so foreign-key constraints in executions table are satisfied.
        // Synchronous write: must complete before response to guarantee FK constraints.
        if let Err(e) = state.runtime.store.proposals().insert(&proposal).await {
            tracing::warn!(error = %e, "failed to persist proposal to DB");
            return governance_err!(
                state,
                GovernanceRoute::ProposalsEvaluate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
        out
    } else {
        let out = match state.runtime.pdp.evaluate(&intent, &proposal, &trust).await {
            Ok(out) => out,
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ProposalsEvaluate,
                    ApiProblem::internal(e)
                );
            }
        };

        // Persist the proposal so foreign-key constraints in executions table are satisfied.
        // Synchronous write: must complete before response to guarantee FK constraints.
        if let Err(e) = state.runtime.store.proposals().insert(&proposal).await {
            tracing::warn!(error = %e, "failed to persist proposal to DB");
            return governance_err!(
                state,
                GovernanceRoute::ProposalsEvaluate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
        out
    };

    // Emit PolicyEvaluated provenance event after evaluation succeeds.
    let decision_str = format!("{:?}", out.decision);
    let mut policy_metadata = ferrum_proto::JsonMap::new();
    policy_metadata.insert("decision".to_string(), serde_json::json!(decision_str));
    policy_metadata.insert("reason".to_string(), serde_json::json!("policy_evaluation"));
    let policy_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::PolicyEvaluated,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::PolicyBundle,
            object_id: proposal.proposal_id.to_string(),
            summary: Some("Policy evaluated for proposal".to_string()),
        },
        intent_id: Some(proposal.intent_id),
        proposal_id: Some(proposal.proposal_id),
        execution_id: None,
        capability_id: None,
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
        metadata: policy_metadata,
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&policy_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ProposalsEvaluate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(state, GovernanceRoute::ProposalsEvaluate, Ok(Json(out)))
}
