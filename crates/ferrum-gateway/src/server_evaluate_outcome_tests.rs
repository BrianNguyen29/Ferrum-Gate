use super::*;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn test_evaluate_outcome_endpoint_aligned() {
    let runtime = test_runtime().await;
    let router = build_router(runtime.clone());

    // Create an intent with allowed outcome
    let intent_id = ferrum_proto::IntentId::new();
    let intent = IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
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
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };
    runtime.store.intents().insert(&intent).await.unwrap();

    // Create a proposal to satisfy foreign key constraints
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test_tool".to_string(),
        server_name: "test_server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read only analysis".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Mint a capability to satisfy foreign key constraints
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test_server".to_string(),
            tool_name: "test_tool".to_string(),
            tool_version: None,
        },
        resource_bindings: Vec::new(),
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let capability_response = runtime.cap.mint(mint_request).await.unwrap();
    runtime
        .store
        .capabilities()
        .insert(&capability_response.lease)
        .await
        .unwrap();

    // Create an execution for this intent
    let execution_id = ExecutionId::new();
    let record = ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id: capability_response.lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Committed,
        started_at: chrono::Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };
    runtime.store.executions().insert(&record).await.unwrap();

    // Build an aligned outcome report
    let report = OutcomeReport {
        execution_id,
        actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
        description: "completed read-only analysis".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: ferrum_proto::JsonMap::new(),
    };

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&report).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let result: EvaluateOutcomeResponse = serde_json::from_slice(&body).unwrap();
    assert!(result.aligned);
}

#[tokio::test]
async fn test_evaluate_outcome_endpoint_forbidden() {
    let runtime = test_runtime().await;
    let router = build_router(runtime.clone());

    // Create an intent that explicitly forbids GitMutation.
    let intent_id = ferrum_proto::IntentId::new();
    let intent = IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: vec![OutcomeClause {
            id: "no-git".to_string(),
            description: "no git mutations allowed".to_string(),
            effect_type: ferrum_proto::EffectType::GitMutation,
            required: true,
        }],
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
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
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };
    runtime.store.intents().insert(&intent).await.unwrap();

    // Create a proposal to satisfy foreign key constraints
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test_tool".to_string(),
        server_name: "test_server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read only analysis".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Mint a capability to satisfy foreign key constraints
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test_server".to_string(),
            tool_name: "test_tool".to_string(),
            tool_version: None,
        },
        resource_bindings: Vec::new(),
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let capability_response = runtime.cap.mint(mint_request).await.unwrap();
    runtime
        .store
        .capabilities()
        .insert(&capability_response.lease)
        .await
        .unwrap();

    // Create an execution
    let execution_id = ExecutionId::new();
    let record = ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id: capability_response.lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Committed,
        started_at: chrono::Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };
    runtime.store.executions().insert(&record).await.unwrap();

    // Build an outcome report with a non-allowed effect (git mutation instead of read-only)
    let report = OutcomeReport {
        execution_id,
        actual_effect: ferrum_proto::EffectType::GitMutation,
        description: "mutated git repository".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: ferrum_proto::JsonMap::new(),
    };

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&report).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let result: EvaluateOutcomeResponse = serde_json::from_slice(&body).unwrap();
    assert!(!result.aligned);
}

#[tokio::test]
async fn test_evaluate_outcome_execution_not_found() {
    let runtime = test_runtime().await;
    let router = build_router(runtime.clone());

    let execution_id = ExecutionId::new();
    let report = OutcomeReport {
        execution_id,
        actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
        description: "test".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: ferrum_proto::JsonMap::new(),
    };

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&report).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_evaluate_outcome_id_mismatch() {
    let runtime = test_runtime().await;
    let router = build_router(runtime.clone());

    // Create an intent
    let intent_id = ferrum_proto::IntentId::new();
    let intent = IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
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
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };
    runtime.store.intents().insert(&intent).await.unwrap();

    // Create a proposal to satisfy foreign key constraints
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test_tool".to_string(),
        server_name: "test_server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read only analysis".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Mint a capability to satisfy foreign key constraints
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test_server".to_string(),
            tool_name: "test_tool".to_string(),
            tool_version: None,
        },
        resource_bindings: Vec::new(),
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let capability_response = runtime.cap.mint(mint_request).await.unwrap();
    runtime
        .store
        .capabilities()
        .insert(&capability_response.lease)
        .await
        .unwrap();

    // Create an execution
    let execution_id = ExecutionId::new();
    let record = ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id: capability_response.lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Committed,
        started_at: chrono::Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };
    runtime.store.executions().insert(&record).await.unwrap();

    // Report with mismatched execution_id in body
    let report = OutcomeReport {
        execution_id: ExecutionId::new(), // different id
        actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
        description: "test".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: ferrum_proto::JsonMap::new(),
    };

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&report).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
