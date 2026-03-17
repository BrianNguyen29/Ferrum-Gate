use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use ferrum_proto::{
    ApiError, ApiErrorCode, AuthorizeExecutionRequest, AuthorizeExecutionResponse, CapabilityId,
    CapabilityMintRequest, CapabilityMintResponse, Decision, EvaluateProposalResponse,
    ExecutionId, ExecutionRecord, ExecutionState, HealthResponse, IntentCompileRequest,
    IntentCompileResponse, IntentEnvelope, IntentStatus, OutcomeClause, ProvenanceEvent,
    ProvenanceEventKind, ResourceSelector, RiskTier, RollbackClass, TimeBudget, TrustContextSummary,
    ActorRef, ActorType, ObjectRef, ObjectType, HashChainRef,
};
use ferrum_store::{CapabilityRepo, ExecutionRepo, IntentRepo, ProvenanceRepo, RollbackRepo};
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
        .route("/v1/proposals/{proposal_id}/evaluate", post(evaluate_proposal))
        .route("/v1/capabilities/mint", post(mint_capability))
        .route("/v1/capabilities/{capability_id}/revoke", post(revoke_capability))
        .route("/v1/executions/authorize", post(authorize_execution))
        .route("/v1/executions/{execution_id}/prepare", post(prepare_execution))
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
    Path(_proposal_id): Path<String>,
    Json(proposal): Json<ferrum_proto::ActionProposal>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    let intent = minimal_intent_for(proposal.intent_id, proposal.requested_rollback_class.clone());
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 0,
        contains_external_metadata: false,
        contains_tool_output: false,
        contains_untrusted_text: false,
    };

    let out = runtime
        .pdp
        .evaluate(&intent, &proposal, &trust)
        .await
        .map_err(ApiProblem::internal)?;
    Ok(Json(out))
}

async fn mint_capability(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(request): Json<CapabilityMintRequest>,
) -> Result<Json<CapabilityMintResponse>, ApiProblem> {
    let response = runtime.cap.mint(request).await.map_err(ApiProblem::from_capability)?;
    
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
    let lease = runtime.cap.revoke(id).await.map_err(ApiProblem::from_capability)?;
    
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

    let now = Utc::now();
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
        started_at: now,
        finished_at: None,
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
        let event = create_provenance_event(
            ProvenanceEventKind::ToolCallPrepared,
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

    let request = runtime.rollback.default_prepare_request(
        intent_id,
        proposal_id,
        execution_id,
        RollbackClass::R0NativeReversible,
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

fn infer_rollback_class(scope: &[ResourceSelector]) -> RollbackClass {
    if scope.iter().any(|selector| matches!(selector, ResourceSelector::EmailDraft { .. })) {
        RollbackClass::R2Compensatable
    } else {
        RollbackClass::R0NativeReversible
    }
}

fn minimal_intent_for(intent_id: ferrum_proto::IntentId, rollback: RollbackClass) -> IntentEnvelope {
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
