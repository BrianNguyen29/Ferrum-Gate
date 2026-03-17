use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use ferrum_proto::{
    ActorRef, ActorType, ApiError, ApiErrorCode, AuthorizeExecutionRequest,
    AuthorizeExecutionResponse, CapabilityId, CapabilityMintRequest, CapabilityMintResponse,
    CommitRequest, CommitResponse, CompensateRequest, CompensateResponse, Decision,
    EvaluateProposalResponse, ExecuteRequest, ExecuteResponse, ExecutionId, ExecutionRecord,
    ExecutionState, HashChainRef, HealthResponse, IntentCompileRequest, IntentCompileResponse,
    IntentEnvelope, IntentStatus, ObjectRef, ObjectType, OutcomeClause, ProvenanceEvent,
    ProvenanceEventKind, ResourceSelector, RiskTier, RollbackClass, RollbackRequest,
    RollbackResponse, RollbackState, TimeBudget, TrustContextSummary, VerifyRequest,
    VerifyResponse,
};
use ferrum_store::{
    CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo, RollbackRepo,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::{GatewayConfig, GatewayRuntime};

fn create_provenance_event(
    kind: ProvenanceEventKind,
    occurred_at: chrono::DateTime<Utc>,
    intent_id: Option<ferrum_proto::IntentId>,
    proposal_id: Option<ferrum_proto::ProposalId>,
    execution_id: Option<ferrum_proto::ExecutionId>,
    capability_id: Option<CapabilityId>,
    rollback_contract_id: Option<ferrum_proto::RollbackContractId>,
    policy_bundle_id: Option<ferrum_proto::PolicyBundleId>,
) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind,
        occurred_at,
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("Ferrum Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Unknown,
            object_id: String::new(),
            summary: None,
        },
        intent_id,
        proposal_id,
        execution_id,
        capability_id,
        rollback_contract_id,
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
        metadata: ferrum_proto::JsonMap::new(),
    }
}

pub async fn run_http_server(config: GatewayConfig, runtime: GatewayRuntime) -> anyhow::Result<()> {
    let app = build_router(runtime);
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("ferrumd listening on {}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_router(runtime: GatewayRuntime) -> Router {
    Router::new()
        .route("/v1/healthz", get(healthz))
        .route("/v1/readyz", get(readyz))
        .route("/v1/intents/compile", post(compile_intent))
        .route(
            "/v1/proposals/{proposal_id}/evaluate",
            post(evaluate_proposal),
        )
        .route("/v1/capabilities/mint", post(mint_capability))
        .route(
            "/v1/capabilities/{capability_id}/revoke",
            post(revoke_capability),
        )
        .route("/v1/executions/authorize", post(authorize_execution))
        .route(
            "/v1/executions/{execution_id}/prepare",
            post(prepare_execution),
        )
        .route(
            "/v1/executions/{execution_id}/execute",
            post(execute_execution),
        )
        .route(
            "/v1/executions/{execution_id}/verify",
            post(verify_execution),
        )
        .route(
            "/v1/executions/{execution_id}/commit",
            post(commit_execution),
        )
        .route(
            "/v1/executions/{execution_id}/compensate",
            post(compensate_execution),
        )
        .route(
            "/v1/executions/{execution_id}/rollback",
            post(rollback_execution),
        )
        .with_state(Arc::new(runtime))
        .layer(TraceLayer::new_for_http())
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn readyz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ready".to_string(),
    })
}

async fn compile_intent(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(req): Json<IntentCompileRequest>,
) -> Result<Json<IntentCompileResponse>, ApiProblem> {
    let now = Utc::now();
    let requested_risk = req.requested_risk_tier.unwrap_or(RiskTier::Medium);
    let default_rollback_class = infer_rollback_class(&req.requested_resource_scope);

    let input_labels = req
        .raw_inputs
        .iter()
        .flat_map(|r| r.trust_labels.clone())
        .collect::<Vec<_>>();
    let sensitivity_labels = req
        .raw_inputs
        .iter()
        .flat_map(|r| r.sensitivity_labels.clone())
        .collect::<Vec<_>>();

    let envelope = IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: req.principal_id,
        session_id: req.session_id,
        channel_id: req.channel_id,
        title: req.title.clone(),
        goal: req.goal.clone(),
        normalized_goal: req.goal.trim().to_lowercase(),
        allowed_outcomes: vec![OutcomeClause {
            id: "primary".to_string(),
            description: req.agent_plan_summary.unwrap_or_else(|| req.goal.clone()),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: req.requested_resource_scope,
        risk_tier: requested_risk,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class,
        time_budget: TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: TrustContextSummary {
            input_labels,
            sensitivity_labels,
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: req.raw_inputs.iter().filter_map(|r| r.event_id).collect(),
        tags: Vec::new(),
        metadata: req.metadata,
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + Duration::minutes(15),
    };

    let intent_id = envelope.intent_id;
    if let Err(e) = runtime.store.intents().insert(&envelope).await {
        tracing::warn!("failed to persist intent: {}", e);
    } else {
        let event = create_provenance_event(
            ProvenanceEventKind::IntentCompiled,
            now,
            Some(intent_id),
            None,
            None,
            None,
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(IntentCompileResponse {
        envelope,
        warnings: Vec::new(),
    }))
}

async fn evaluate_proposal(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(proposal_id_from_path): Path<String>,
    Json(proposal): Json<ferrum_proto::ActionProposal>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    let now = Utc::now();
    let intent_id = proposal.intent_id;

    // Validate path and body proposal_id match
    let proposal_id_from_body = proposal.proposal_id.to_string();
    if proposal_id_from_path != proposal_id_from_body {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            format!(
                "proposal_id mismatch: path has '{}', body has '{}'",
                proposal_id_from_path, proposal_id_from_body
            ),
        ));
    }

    // Load the real intent from store if it exists; fall back to minimal intent only if not found
    let intent = match runtime.store.intents().get(intent_id).await {
        Ok(Some(real_intent)) => real_intent,
        Ok(None) => {
            tracing::warn!("intent {} not found, using minimal intent", intent_id);
            minimal_intent_for(
                proposal.intent_id,
                proposal.requested_rollback_class.clone(),
            )
        }
        Err(e) => {
            tracing::warn!(
                "failed to load intent {}: {}, using minimal intent",
                intent_id,
                e
            );
            minimal_intent_for(
                proposal.intent_id,
                proposal.requested_rollback_class.clone(),
            )
        }
    };

    // Persist the incoming ActionProposal (requires valid intent_id due to FK constraint)
    // Only emit provenance if proposal was successfully persisted
    let proposal_persisted = match runtime.store.proposals().insert(&proposal).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("failed to persist proposal: {}", e);
            false
        }
    };

    // Emit provenance for proposal submission ONLY if proposal was persisted
    let submission_event_id = if proposal_persisted {
        let submission_event = create_provenance_event(
            ProvenanceEventKind::ActionProposalSubmitted,
            now,
            Some(intent_id),
            Some(proposal.proposal_id),
            None,
            None,
            None,
            None,
        );
        let event_id = submission_event.event_id;
        if let Err(e) = runtime
            .store
            .provenance()
            .append_event(&submission_event)
            .await
        {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
        Some(event_id)
    } else {
        None
    };

    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: (proposal.taint_inputs.len() * 10) as u8, // 10 points per taint input
        contains_external_metadata: !proposal.taint_inputs.is_empty(),
        contains_tool_output: false,
        contains_untrusted_text: !proposal.taint_inputs.is_empty(),
    };

    let out = runtime
        .pdp
        .evaluate(&intent, &proposal, &trust)
        .await
        .map_err(ApiProblem::internal)?;

    // Store the decision in the proposal after evaluation
    let mut proposal_with_decision = proposal.clone();
    proposal_with_decision.decision = Some(out.decision.clone());

    // Update the proposal with the decision
    if let Err(e) = runtime
        .store
        .proposals()
        .update(&proposal_with_decision)
        .await
    {
        tracing::warn!("failed to update proposal with decision: {}", e);
    }

    // Emit provenance for policy evaluation with linkage to submission event if proposal was persisted
    let parent_edge = if let Some(submission_event_id) = submission_event_id {
        vec![ferrum_proto::ProvenanceEdge {
            edge_type: ferrum_proto::ProvenanceEdgeType::Caused,
            from_event_id: submission_event_id,
            summary: Some("policy evaluation follows proposal submission".to_string()),
        }]
    } else {
        Vec::new()
    };

    let mut eval_event = create_provenance_event(
        ProvenanceEventKind::PolicyEvaluated,
        now,
        Some(intent_id),
        Some(proposal.proposal_id),
        None,
        None,
        None,
        None,
    );
    eval_event.parent_edges = parent_edge;

    if let Err(e) = runtime.store.provenance().append_event(&eval_event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(out))
}

async fn mint_capability(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(request): Json<CapabilityMintRequest>,
) -> Result<Json<CapabilityMintResponse>, ApiProblem> {
    let response = runtime
        .cap
        .mint(request)
        .await
        .map_err(ApiProblem::from_capability)?;

    let now = Utc::now();
    let capability_id = response.lease.capability_id;
    let intent_id = response.lease.intent_id;
    let proposal_id = response.lease.proposal_id;
    let policy_bundle_id = response.lease.policy_bundle_id;

    if let Err(e) = runtime.store.capabilities().insert(&response.lease).await {
        tracing::warn!("failed to persist capability: {}", e);
    } else {
        let event = create_provenance_event(
            ProvenanceEventKind::CapabilityMinted,
            now,
            Some(intent_id),
            Some(proposal_id),
            None,
            Some(capability_id),
            None,
            Some(policy_bundle_id),
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(response))
}

async fn revoke_capability(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(capability_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    let id = parse_capability_id(&capability_id)?;
    let lease = runtime
        .cap
        .revoke(id)
        .await
        .map_err(ApiProblem::from_capability)?;

    let now = Utc::now();
    if let Err(e) = runtime.store.capabilities().update(&lease).await {
        tracing::warn!("failed to update capability: {}", e);
    } else {
        let event = create_provenance_event(
            ProvenanceEventKind::CapabilityRevoked,
            now,
            Some(lease.intent_id),
            Some(lease.proposal_id),
            None,
            Some(lease.capability_id),
            None,
            Some(lease.policy_bundle_id),
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "capability_id": lease.capability_id.to_string()
    })))
}

async fn authorize_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(request): Json<AuthorizeExecutionRequest>,
) -> Result<Json<AuthorizeExecutionResponse>, ApiProblem> {
    let lease = runtime
        .cap
        .get(request.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

    // Load the proposal to check its decision
    let proposal = runtime
        .store
        .proposals()
        .get(request.proposal_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "proposal not found",
            )
        })?;

    // Check proposal decision - fail-safe: block execution for non-Allow decisions
    let decision = proposal.decision.as_ref();
    let is_quarantined = decision
        .map(|d| *d == Decision::Quarantine)
        .unwrap_or(false);
    let is_require_approval = decision
        .map(|d| *d == Decision::RequireApproval)
        .unwrap_or(false);
    let is_deny = decision.map(|d| *d == Decision::Deny).unwrap_or(false);
    let is_draft_only = decision
        .map(|d| *d == Decision::AllowDraftOnly)
        .unwrap_or(false);

    // Non-allow decisions should not progress to executable states
    let is_blocked = is_quarantined || is_require_approval || is_deny || is_draft_only;

    let now = Utc::now();

    // Determine execution state and decision based on proposal decision
    let (execution_state, execution_decision) = if is_blocked {
        // Blocked decisions get terminal error states:
        // - Quarantine -> Quarantined (already terminal)
        // - RequireApproval -> AwaitingApproval (requires external approval before execution)
        // - Deny -> Denied (terminal, rejected)
        // - AllowDraftOnly -> Denied (draft-only cannot execute)
        if is_quarantined {
            (ExecutionState::Quarantined, Decision::Quarantine)
        } else if is_require_approval {
            (ExecutionState::AwaitingApproval, Decision::RequireApproval)
        } else if is_deny {
            (ExecutionState::Denied, Decision::Deny)
        } else {
            // AllowDraftOnly - treat as denied since we can't execute drafts
            (ExecutionState::Denied, Decision::Deny)
        }
    } else if request.dry_run {
        (ExecutionState::Authorized, Decision::Allow)
    } else {
        (ExecutionState::Prepared, Decision::Allow)
    };

    let record = ExecutionRecord {
        execution_id: ExecutionId::new(),
        proposal_id: request.proposal_id,
        intent_id: lease.intent_id,
        capability_id: lease.capability_id,
        rollback_contract_id: None,
        decision: execution_decision,
        state: execution_state,
        started_at: now,
        finished_at: if is_blocked { Some(now) } else { None },
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let execution_id = record.execution_id;
    let intent_id = record.intent_id;
    let capability_id = record.capability_id;
    let proposal_id = record.proposal_id;

    if let Err(e) = runtime.store.executions().insert(&record).await {
        tracing::warn!("failed to persist execution: {}", e);
    } else {
        // Emit appropriate provenance event based on decision
        let event_kind = if is_blocked {
            if is_quarantined {
                ProvenanceEventKind::Quarantined
            } else if is_require_approval {
                ProvenanceEventKind::ToolCallPrepared // Could add ApprovalRequired variant
            } else {
                ProvenanceEventKind::ToolCallPrepared
            }
        } else {
            ProvenanceEventKind::ToolCallPrepared
        };
        let event = create_provenance_event(
            event_kind,
            now,
            Some(intent_id),
            Some(proposal_id),
            Some(execution_id),
            Some(capability_id),
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(AuthorizeExecutionResponse {
        execution: record,
        warnings: Vec::new(),
    }))
}

async fn prepare_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::PrepareExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id)?;

    let Some(existing) = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
    else {
        return Err(ApiProblem::new(
            StatusCode::NOT_FOUND,
            ApiErrorCode::NotFound,
            "execution record not found",
        ));
    };

    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Load proposal to get the correct rollback class - FAIL CLOSED if not found
    let requested_rollback_class = match runtime.store.proposals().get(proposal_id).await {
        Ok(Some(proposal)) => proposal.requested_rollback_class,
        Ok(None) => {
            return Err(ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                format!(
                    "proposal {} not found for execution preparation",
                    proposal_id
                ),
            ));
        }
        Err(e) => {
            return Err(ApiProblem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Internal,
                format!("failed to load proposal {}: {}", proposal_id, e),
            ));
        }
    };

    let request = runtime.rollback.default_prepare_request(
        intent_id,
        proposal_id,
        execution_id,
        requested_rollback_class,
    );

    let response = runtime
        .rollback
        .prepare(request)
        .await
        .map_err(ApiProblem::internal)?;

    let contract = response.contract.clone();
    let now = Utc::now();

    if let Err(e) = runtime.store.rollback_contracts().insert(&contract).await {
        tracing::warn!("failed to persist rollback contract: {}", e);
    } else {
        let mut updated_execution = existing;
        updated_execution.rollback_contract_id = Some(contract.contract_id);
        updated_execution.state = ExecutionState::Prepared;

        if let Err(e) = runtime.store.executions().update(&updated_execution).await {
            tracing::warn!("failed to update execution with rollback contract: {}", e);
        }

        let event = create_provenance_event(
            ProvenanceEventKind::SideEffectPrepared,
            now,
            Some(intent_id),
            Some(proposal_id),
            Some(execution_id),
            None,
            Some(contract.contract_id),
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(ferrum_proto::PrepareExecutionResponse {
        execution_id,
        prepared: response.accepted,
        rollback_contract: Some(contract),
        warnings: response.warnings,
    }))
}

async fn execute_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: execute requires Prepared
    if !matches!(existing.state, ExecutionState::Prepared) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::ValidationError,
            format!(
                "execution must be in Prepared state to execute, current state: {:?}",
                existing.state
            ),
        ));
    }

    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract, must prepare first",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Execute via adapter
    let receipt = runtime
        .rollback
        .execute(&contract, &req.payload)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to Running (executing)
    let mut updated_execution = existing;
    updated_execution.state = ExecutionState::Running;
    updated_execution.result_digest = receipt.result_digest.clone();

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to ExecutedAwaitingVerify
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::ExecutedAwaitingVerify)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit ToolCallExecuted provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::ToolCallExecuted,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(ExecuteResponse {
        execution_id,
        executed: true,
        result_digest: receipt.result_digest,
        external_id: receipt.external_id,
    }))
}

async fn verify_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: verify requires Running
    if !matches!(existing.state, ExecutionState::Running) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::ValidationError,
            format!(
                "execution must be in Running state to verify, current state: {:?}",
                existing.state
            ),
        ));
    }

    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Verify via adapter
    let verified = runtime
        .rollback
        .verify(&contract)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to AwaitingVerification
    let mut updated_execution = existing.clone();
    updated_execution.state = if verified {
        ExecutionState::AwaitingVerification
    } else {
        ExecutionState::Failed
    };

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to Verified (or Failed)
    if verified {
        if let Err(e) = runtime
            .store
            .rollback_contracts()
            .update_state(contract_id, RollbackState::Verified)
            .await
        {
            tracing::warn!("failed to update rollback contract state: {}", e);
        }
    }

    // Emit SideEffectVerified provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectVerified,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    // Auto-commit for non-R3 contracts if verified
    if verified && contract.auto_commit {
        let commit_response = perform_commit(&runtime, &existing, &contract, now).await?;
        return Ok(Json(VerifyResponse {
            execution_id,
            verified: true,
            verified_at: Some(commit_response.committed_at.unwrap_or(now)),
        }));
    }

    Ok(Json(VerifyResponse {
        execution_id,
        verified,
        verified_at: if verified { Some(now) } else { None },
    }))
}

async fn commit_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<CommitRequest>,
) -> Result<Json<CommitResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: commit requires AwaitingVerification
    if !matches!(existing.state, ExecutionState::AwaitingVerification) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::ValidationError,
            format!(
                "execution must be in AwaitingVerification state to commit, current state: {:?}",
                existing.state
            ),
        ));
    }

    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    let now = Utc::now();
    perform_commit(&runtime, &existing, &contract, now).await
}

async fn perform_commit(
    runtime: &Arc<GatewayRuntime>,
    existing: &ExecutionRecord,
    contract: &ferrum_proto::RollbackContract,
    now: chrono::DateTime<Utc>,
) -> Result<Json<CommitResponse>, ApiProblem> {
    let execution_id = existing.execution_id;
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;
    let contract_id = contract.contract_id;

    // Update execution state to Committed
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::Committed;
    updated_execution.finished_at = Some(now);

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to Committed
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::Committed)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit SideEffectCommitted provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectCommitted,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(CommitResponse {
        execution_id,
        committed: true,
        committed_at: Some(now),
    }))
}

async fn compensate_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<CompensateRequest>,
) -> Result<Json<CompensateResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: reject if already in a terminal state that cannot be compensated
    // Note: Committed is allowed (can undo after commit), but Compensated/RolledBack/Denied/Failed/Quarantined are terminal
    use ferrum_proto::ExecutionState::*;
    match existing.state {
        Compensated | RolledBack | Denied | Failed | Quarantined => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!("execution already in terminal state: {:?}", existing.state),
            ));
        }
        _ => {}
    }

    // State guard: compensate requires execution to have a rollback contract
    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Call rollback service to compensate
    runtime
        .rollback
        .compensate(&contract)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to Compensated
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::Compensated;
    updated_execution.finished_at = Some(now);

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to Compensated
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::Compensated)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit SideEffectCompensated provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectCompensated,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(CompensateResponse {
        execution_id,
        compensated: true,
        compensated_at: Some(now),
    }))
}

async fn rollback_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: reject if already in a terminal state that cannot be rolled back
    // Note: Committed is allowed (can undo after commit), but Compensated/RolledBack/Denied/Failed/Quarantined are terminal
    use ferrum_proto::ExecutionState::*;
    match existing.state {
        Compensated | RolledBack | Denied | Failed | Quarantined => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!("execution already in terminal state: {:?}", existing.state),
            ));
        }
        _ => {}
    }

    // State guard: rollback requires execution to have a rollback contract
    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Call rollback service to rollback
    runtime
        .rollback
        .rollback(&contract)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to RolledBack
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::RolledBack;
    updated_execution.finished_at = Some(now);

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to RolledBack
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::RolledBack)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit SideEffectRolledBack provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectRolledBack,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(RollbackResponse {
        execution_id,
        rolled_back: true,
        rolled_back_at: Some(now),
    }))
}

fn infer_rollback_class(scope: &[ResourceSelector]) -> RollbackClass {
    if scope
        .iter()
        .any(|selector| matches!(selector, ResourceSelector::EmailDraft { .. }))
    {
        RollbackClass::R2Compensatable
    } else {
        RollbackClass::R0NativeReversible
    }
}

fn minimal_intent_for(
    intent_id: ferrum_proto::IntentId,
    rollback: RollbackClass,
) -> IntentEnvelope {
    let now = Utc::now();
    IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "scaffold-intent".to_string(),
        goal: "scaffold evaluation".to_string(),
        normalized_goal: "scaffold evaluation".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "default".to_string(),
            description: "scaffold outcome".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: rollback,
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
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + Duration::minutes(15),
    }
}

fn parse_capability_id(value: &str) -> Result<CapabilityId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid capability uuid",
        )
    })?;
    Ok(CapabilityId(parsed))
}

fn parse_execution_id(value: &str) -> Result<ExecutionId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid execution uuid",
        )
    })?;
    Ok(ExecutionId(parsed))
}

#[derive(Debug)]
struct ApiProblem(ApiError, StatusCode);

impl ApiProblem {
    fn new(status: StatusCode, code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self(
            ApiError {
                code,
                message: message.into(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            },
            status,
        )
    }

    fn internal(err: anyhow::Error) -> Self {
        Self(
            ApiError {
                code: ApiErrorCode::Internal,
                message: err.to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            },
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    }

    fn from_capability(err: ferrum_cap::CapabilityError) -> Self {
        let code = match err {
            ferrum_cap::CapabilityError::NotFound => ApiErrorCode::NotFound,
            ferrum_cap::CapabilityError::AlreadyUsed => ApiErrorCode::Conflict,
            ferrum_cap::CapabilityError::Revoked => ApiErrorCode::CapabilityRevoked,
            ferrum_cap::CapabilityError::Expired => ApiErrorCode::CapabilityExpired,
            ferrum_cap::CapabilityError::TtlTooLong => ApiErrorCode::ValidationError,
        };
        Self::new(StatusCode::BAD_REQUEST, code, err.to_string())
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        (self.1, Json(self.0)).into_response()
    }
}
