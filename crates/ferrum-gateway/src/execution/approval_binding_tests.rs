//! Tests for I6 role-bound approval validation against authenticated resolver
//! evidence (P0 role binding). Metadata fixtures mirror exactly what
//! `approval::resolve_approval` writes after the hardening.

use std::sync::Arc;

use chrono::{Duration, Utc};
use ferrum_proto::{
    ActionProposal, ActorRef, ActorType, ApprovalBinding, ApprovalId, ApprovalMode,
    ApprovalRequest, ApprovalState, CURRENT_RESOLVER_EVIDENCE_VERSION, EventId, HashChainRef,
    IntentEnvelope, IntentId, IntentStatus, JsonMap, ObjectRef, ObjectType, PrincipalId,
    ProposalId, ProvenanceEvent, ProvenanceEventKind, RiskTier, RollbackClass, TimeBudget,
    TrustContextSummary,
};
use ferrum_store::StoreFacade;

use super::validate_approval_binding_digest;

/// The durable marker `ApprovalRepo::resolve` stamps on a hardened-resolver
/// grant. Used to simulate a new-code resolution in direct-insert fixtures.
const RESOLVED_MARKER: Option<u32> = Some(CURRENT_RESOLVER_EVIDENCE_VERSION);

/// Seed intent + proposal + approval (digest-consistent) in `state` carrying
/// `resolver_evidence_version`, and return the ids plus the proposal digest
/// needed to build a valid binding. `resolver_evidence_version=None` models a
/// pre-hardening historical record; `Some(_)` models a new-code resolution.
async fn seed_approval(
    store: &Arc<dyn StoreFacade>,
    requested_by_type: ActorType,
    state: ApprovalState,
    resolver_evidence_version: Option<u32>,
) -> (IntentId, ProposalId, ApprovalId, String) {
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let approval_id = ApprovalId::new();
    let now = Utc::now();

    store
        .intents()
        .insert(&IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "t".to_string(),
            goal: "g".to_string(),
            normalized_goal: "g".to_string(),
            allowed_outcomes: Vec::new(),
            forbidden_outcomes: Vec::new(),
            resource_scope: Vec::new(),
            risk_tier: RiskTier::Low,
            approval_mode: ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: Vec::new(),
                sensitivity_labels: Vec::new(),
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: Vec::new(),
            tags: Vec::new(),
            metadata: JsonMap::new(),
            status: IntentStatus::Active,
            created_at: now,
            expires_at: now + Duration::hours(1),
        })
        .await
        .unwrap();

    let proposal = ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "p".to_string(),
        tool_name: "tool".to_string(),
        server_name: "server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "e".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: JsonMap::new(),
        created_at: now,
    };
    let digest = proposal.canonical_action_digest();
    store.proposals().insert(&proposal).await.unwrap();

    store
        .approvals()
        .insert(&ApprovalRequest {
            approval_id,
            intent_id,
            proposal_id,
            execution_id: None,
            requested_by: ActorRef {
                actor_type: requested_by_type,
                actor_id: "requester".to_string(),
                display_name: None,
            },
            reason: "r".to_string(),
            action_digest: digest.clone(),
            expires_at: now + Duration::hours(1),
            state,
            created_at: now,
            resolver_evidence_version,
        })
        .await
        .unwrap();

    (intent_id, proposal_id, approval_id, digest)
}

/// Seed a Granted approval directly (bypassing resolve). `resolver_evidence_version`
/// controls whether the fixture models a new-code resolution (`Some`) or a
/// pre-hardening historical record (`None`).
async fn seed_granted_approval(
    store: &Arc<dyn StoreFacade>,
    requested_by_type: ActorType,
    resolver_evidence_version: Option<u32>,
) -> (IntentId, ProposalId, ApprovalId, String) {
    seed_approval(
        store,
        requested_by_type,
        ApprovalState::Granted,
        resolver_evidence_version,
    )
    .await
}

fn binding(approval_id: ApprovalId, digest: &str, roles: &[&str]) -> ApprovalBinding {
    ApprovalBinding {
        approval_id,
        approver_roles: roles.iter().map(|r| r.to_string()).collect(),
        approved_action_digest: digest.to_string(),
        expires_at: Utc::now() + Duration::hours(1),
    }
}

/// Authenticated resolver evidence, as written by resolve_approval for
/// Scoped/OIDC (role known) auth modes.
fn authenticated_metadata(
    approval_id: ApprovalId,
    actor_id: &str,
    source: &str,
    role: &str,
) -> JsonMap {
    let mut metadata = JsonMap::new();
    metadata.insert(
        "approval_id".to_string(),
        serde_json::json!(approval_id.to_string()),
    );
    metadata.insert("actor_id".to_string(), serde_json::json!(actor_id));
    metadata.insert("actor_source".to_string(), serde_json::json!(source));
    metadata.insert("actor_authenticated".to_string(), serde_json::json!(true));
    metadata.insert("actor_role".to_string(), serde_json::json!(role));
    metadata
}

/// Unauthenticated request-body evidence, as written by resolve_approval for
/// Bearer/Disabled fallback.
fn legacy_body_metadata(approval_id: ApprovalId, actor_id: &str, claimed_type: &str) -> JsonMap {
    let mut metadata = JsonMap::new();
    metadata.insert(
        "approval_id".to_string(),
        serde_json::json!(approval_id.to_string()),
    );
    metadata.insert("actor_id".to_string(), serde_json::json!(actor_id));
    metadata.insert(
        "actor_source".to_string(),
        serde_json::json!("request_body_legacy"),
    );
    metadata.insert("actor_authenticated".to_string(), serde_json::json!(false));
    metadata.insert(
        "claimed_actor_type".to_string(),
        serde_json::json!(claimed_type),
    );
    metadata
}

/// Pre-hardening event shape: actor_id only, no resolver evidence keys.
fn old_format_metadata(actor_id: &str) -> JsonMap {
    let mut metadata = JsonMap::new();
    metadata.insert("actor_id".to_string(), serde_json::json!(actor_id));
    metadata
}

async fn append_grant_event(
    store: &Arc<dyn StoreFacade>,
    intent_id: IntentId,
    proposal_id: ProposalId,
    approval_id: ApprovalId,
    metadata: JsonMap,
) {
    store
        .provenance()
        .append_event(&ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::ApprovalGranted,
            occurred_at: Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::Approval,
                object_id: approval_id.to_string(),
                summary: None,
            },
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
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
            metadata,
            source_runtime_id: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn new_format_authenticated_matching_role_succeeds() {
    let runtime = crate::server::test_runtime().await;
    let (intent_id, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, RESOLVED_MARKER).await;
    append_grant_event(
        &runtime.store,
        intent_id,
        proposal_id,
        approval_id,
        authenticated_metadata(approval_id, "op-1", "scoped", "operator"),
    )
    .await;

    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_ok(),
        "authenticated operator evidence must satisfy: {result:?}"
    );
}

#[tokio::test]
async fn new_format_admin_role_accepted_for_admin_binding() {
    let runtime = crate::server::test_runtime().await;
    let (intent_id, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, RESOLVED_MARKER).await;
    append_grant_event(
        &runtime.store,
        intent_id,
        proposal_id,
        approval_id,
        authenticated_metadata(approval_id, "admin-1", "oidc", "admin"),
    )
    .await;

    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["admin"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_ok(),
        "authenticated admin evidence must satisfy: {result:?}"
    );
}

/// The pre-hardening bug: requested_by.actor_type matched approver_roles even
/// when the resolver's role did not. New-format evidence must govern and deny.
#[tokio::test]
async fn new_format_wrong_role_denied_despite_requested_by_match() {
    let runtime = crate::server::test_runtime().await;
    let (intent_id, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, RESOLVED_MARKER).await;
    append_grant_event(
        &runtime.store,
        intent_id,
        proposal_id,
        approval_id,
        authenticated_metadata(approval_id, "aud-1", "scoped", "auditor"),
    )
    .await;

    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_err(),
        "requested_by fallback must not rescue a role mismatch on new-format events"
    );
}

/// An event explicitly marked actor_authenticated=false (Bearer/Disabled
/// request-body actor) must never satisfy a role-bound approval, even when the
/// claimed/requested_by actor type matches.
#[tokio::test]
async fn new_format_unauthenticated_legacy_event_denied() {
    let runtime = crate::server::test_runtime().await;
    let (intent_id, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, RESOLVED_MARKER).await;
    append_grant_event(
        &runtime.store,
        intent_id,
        proposal_id,
        approval_id,
        legacy_body_metadata(approval_id, "body-actor", "operator"),
    )
    .await;

    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_err(),
        "actor_authenticated=false evidence must never satisfy approver_roles"
    );
}

/// Historical carve-out: approvals granted before resolver-evidence metadata
/// existed retain the old requested_by fallback.
#[tokio::test]
async fn historical_old_format_event_uses_requested_by_fallback() {
    let runtime = crate::server::test_runtime().await;
    let (intent_id, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, None).await;
    append_grant_event(
        &runtime.store,
        intent_id,
        proposal_id,
        approval_id,
        old_format_metadata("legacy-approver"),
    )
    .await;

    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_ok(),
        "old-format events keep the requested_by fallback: {result:?}"
    );
}

/// Historical carve-out: approvals granted out-of-band (no provenance event)
/// retain the old requested_by fallback, in both directions.
#[tokio::test]
async fn historical_no_event_uses_requested_by_fallback() {
    let runtime = crate::server::test_runtime().await;
    let (_, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, None).await;

    let accepted = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        accepted.is_ok(),
        "no-event approval keeps fallback: {accepted:?}"
    );

    let rejected = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["admin"]),
        proposal_id,
    )
    .await;
    assert!(
        rejected.is_err(),
        "fallback must still reject non-matching roles"
    );
}

/// P0.5 regression: a marker-bearing grant with NO provenance event must fail
/// closed. The durable marker (not the absence of an event) decides strictness,
/// so the requested_by fallback is never available to a new-code resolution.
#[tokio::test]
async fn marker_bearing_no_event_fails_closed() {
    let runtime = crate::server::test_runtime().await;
    let (_, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, RESOLVED_MARKER).await;

    // requested_by.actor_type == operator matches the binding, but the durable
    // marker requires authenticated resolver evidence and none exists.
    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_err(),
        "marker-bearing grant with no event must not use the requested_by fallback: {result:?}"
    );
}

/// P0.5 end-to-end: resolve a Pending approval through the real store CAS
/// (which stamps the marker atomically with the grant), then simulate the
/// provenance-append failure by appending no event. The grant must fail closed.
#[tokio::test]
async fn resolved_approval_without_event_fails_closed() {
    let runtime = crate::server::test_runtime().await;
    let (_, proposal_id, approval_id, digest) = seed_approval(
        &runtime.store,
        ActorType::Operator,
        ApprovalState::Pending,
        None,
    )
    .await;

    // Resolve via the real store path: this stamps resolver_evidence_version in
    // the same atomic write that transitions the approval to Granted.
    let won = runtime
        .store
        .approvals()
        .resolve(approval_id, ApprovalState::Granted, Utc::now())
        .await
        .unwrap();
    assert!(won, "seed pending approval must resolve to Granted");

    let resolved = runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resolved.resolver_evidence_version, RESOLVED_MARKER,
        "resolve() must stamp the resolver-evidence marker"
    );

    // No provenance event appended: the resolve-before-provenance failure
    // scenario. The marker-bearing grant must fail closed on role-bound I6.
    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_err(),
        "resolved-but-no-event grant must fail closed: {result:?}"
    );
}

/// Mixed-evidence precedence: for a marker-bearing approval, an old-format
/// event (no actor_authenticated) and a matching requested_by are both ignored;
/// only authenticated new-format resolver evidence can satisfy the binding.
#[tokio::test]
async fn marker_bearing_old_format_event_denied_despite_requested_by_match() {
    let runtime = crate::server::test_runtime().await;
    let (intent_id, proposal_id, approval_id, digest) =
        seed_granted_approval(&runtime.store, ActorType::Operator, RESOLVED_MARKER).await;
    append_grant_event(
        &runtime.store,
        intent_id,
        proposal_id,
        approval_id,
        old_format_metadata("legacy-approver"),
    )
    .await;

    let result = validate_approval_binding_digest(
        &runtime.store,
        &binding(approval_id, &digest, &["operator"]),
        proposal_id,
    )
    .await;
    assert!(
        result.is_err(),
        "marker-bearing grant must ignore old-format evidence and the requested_by fallback: {result:?}"
    );
}
