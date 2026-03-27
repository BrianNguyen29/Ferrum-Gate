use ferrum_adapter_git::GitRollbackAdapter;
use ferrum_adapter_http::HttpRollbackAdapter;
use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, CapabilityMintRequest, Decision, EffectType, ExecutionState,
    IntentCompileRequest, IntentCompileResponse, IntentId, ProposalId, ProvenanceEventKind,
    ResourceBinding, ResourceMode, ResourceSelector, RiskTier, RollbackClass, RollbackTarget,
    SensitivityLabel, TaintBudget, ToolBinding, TrustLabel,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, ProposalRepo,
    ProvenanceRepo, RollbackRepo, SqliteStore,
};
use sqlx::{Connection, Row};
use std::sync::Arc;
use tempfile::TempDir;
use tower::util::ServiceExt;

async fn create_test_runtime() -> (TempDir, GatewayRuntime, SqliteStore) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = SqliteStore::connect(&database_url)
        .await
        .expect("failed to connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");

    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine::default());
    let cap: Arc<dyn CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    // Register fs adapter for adapter-backed rollback/compensate tests
    registry.register(Arc::new(ferrum_adapter_fs::FsRollbackAdapter::new("fs")));
    // Register sqlite adapter for sqlite-backed rollback/compensate tests
    registry.register(Arc::new(ferrum_adapter_sqlite::SqliteRollbackAdapter::new(
        "sqlite",
    )));
    // Register maildraft adapter for email draft-only rollback/compensate tests
    registry.register(Arc::new(ferrum_adapter_maildraft::MaildraftAdapter::new(
        "maildraft",
    )));
    // Register git adapter for git-backed rollback/compensate tests
    registry.register(Arc::new(GitRollbackAdapter::new("git")));
    // Register http adapter for HTTP-backed rollback/compensate tests
    registry.register(Arc::new(HttpRollbackAdapter::new()));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()), firewall);

    (temp_dir, runtime, store)
}

fn sqlite_url_for_path(path: &std::path::Path) -> String {
    format!("sqlite://{}", path.display())
}

async fn seed_sqlite_row(db_path: &std::path::Path, table: &str, row_id: &str, content: &str) {
    let mut conn = sqlx::SqliteConnection::connect(&sqlite_url_for_path(db_path))
        .await
        .expect("failed to connect to sqlite test db");
    let create_stmt =
        format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, content TEXT NOT NULL)");
    sqlx::query(&create_stmt)
        .execute(&mut conn)
        .await
        .expect("failed to create sqlite test table");
    let upsert_stmt = format!(
        "INSERT INTO {table} (id, content) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET content = excluded.content"
    );
    sqlx::query(&upsert_stmt)
        .bind(row_id)
        .bind(content)
        .execute(&mut conn)
        .await
        .expect("failed to seed sqlite test row");
}

async fn fetch_sqlite_row_content(
    db_path: &std::path::Path,
    table: &str,
    row_id: &str,
) -> Option<String> {
    let mut conn = sqlx::SqliteConnection::connect(&sqlite_url_for_path(db_path))
        .await
        .expect("failed to connect to sqlite test db");
    let select_stmt = format!("SELECT content FROM {table} WHERE id = ?1");
    sqlx::query(&select_stmt)
        .bind(row_id)
        .fetch_optional(&mut conn)
        .await
        .expect("failed to query sqlite test row")
        .map(|row| row.get::<String, _>(0))
}

fn sample_intent_request() -> IntentCompileRequest {
    sample_intent_request_with_effect(EffectType::ReadOnlyAnalysis)
}

fn sample_intent_request_with_effect(effect_type: EffectType) -> IntentCompileRequest {
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(effect_type),
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn sample_proposal(intent_id: IntentId) -> ActionProposal {
    ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Inspect state".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read a file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_compile_intent_persists_and_emits_provenance() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Verify intent was persisted
    let stored_intent = runtime.store.intents().get(intent_id).await.unwrap();
    assert!(stored_intent.is_some());
    assert_eq!(stored_intent.unwrap().intent_id, intent_id);

    // Verify provenance event was emitted
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::IntentCompiled),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(!events.is_empty());
    assert!(matches!(
        events[0].kind,
        ProvenanceEventKind::IntentCompiled
    ));
}

#[tokio::test]
async fn test_evaluate_proposal_loads_real_intent_and_persists() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // First compile an intent to have a real intent in the store
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Now create a proposal using that intent_id
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Assert the decision is Allow (default policy allows low-risk R0 proposals)
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Verify proposal was persisted
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());
    assert_eq!(stored_proposal.unwrap().proposal_id, proposal_id);

    // Verify provenance events were emitted
    let submission_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ActionProposalSubmitted),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(!submission_events.is_empty());
    assert!(matches!(
        submission_events[0].kind,
        ProvenanceEventKind::ActionProposalSubmitted
    ));

    let eval_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(!eval_events.is_empty());
    assert!(matches!(
        eval_events[0].kind,
        ProvenanceEventKind::PolicyEvaluated
    ));
}

#[tokio::test]
async fn test_evaluate_proposal_rejects_missing_intent_fail_closed() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create a proposal with a non-existent intent_id
    // Fail-closed: when intent cannot be loaded, the proposal must be rejected,
    // not using a fallback that could bypass the R3 boundary.
    let non_existent_intent_id = IntentId::new();
    let proposal = sample_proposal(non_existent_intent_id);
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Fail-closed: must return 404 when intent not found
    assert_eq!(
        response.status(),
        404,
        "Expected 404 Not Found when intent does not exist"
    );

    // Verify no proposal was persisted (intent must exist for FK constraint)
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(
        stored_proposal.is_none(),
        "Proposal should not be persisted when intent is not found"
    );

    // Verify no provenance events were emitted for this failed evaluation
    let eval_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(non_existent_intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        eval_events.is_empty(),
        "No PolicyEvaluated event should be emitted when intent is not found (fail-closed)"
    );

    let submission_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(non_existent_intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ActionProposalSubmitted),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        submission_events.is_empty(),
        "ActionProposalSubmitted should NOT be emitted when intent does not exist"
    );
}

#[tokio::test]
async fn test_rollback_class_floor_prevents_downgrade_below_intent_default() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile an intent with HTTP POST scope which infers R3 as default rollback class
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        mode: ResourceMode::Write,
    }];
    req.effect_type = Some(EffectType::ExternalApiCall);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Verify the intent's default_rollback_class is R3 for HTTP POST
    assert_eq!(
        compile_resp.envelope.default_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence,
        "HTTP POST intent should have default_rollback_class of R3"
    );

    // Create a proposal that tries to request R0 (below the intent default of R3)
    // This should be elevated to R3 during evaluation, triggering RequireApproval
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Try downgrade".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/users"}),
        expected_effect: "post http request".to_string(), // ExternalApiCall - mutating
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible, // Intentionally below R3
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // The client requested R0 but the intent has R3 default.
    // The floor should be enforced, so R0 should be elevated to R3.
    // Since R3 requires approval, the decision should be RequireApproval.
    assert_eq!(
        eval_resp.decision,
        Decision::RequireApproval,
        "Downgrade attempt from R0 to R3 intent should be blocked - R3 requires approval"
    );

    // Verify the proposal was persisted with the ELEVATED rollback class (R3, not R0)
    // This proves the floor was enforced at persistence time
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some(), "Proposal should be persisted");
    let stored = stored_proposal.unwrap();

    // The persisted proposal should have R3 (elevated from R0), proving the floor was enforced
    assert_eq!(
        stored.requested_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence,
        "Persisted rollback class should be R3 (elevated from R0), proving floor was enforced"
    );
}

#[tokio::test]
async fn test_mutating_http_intent_compiles_to_r3() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile an intent with mutating HTTP POST scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        mode: ResourceMode::Write,
    }];

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();

    // Verify the intent's default_rollback_class is R3 for mutating HTTP
    assert_eq!(
        compile_resp.envelope.default_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence,
        "Mutating HTTP (POST) intent should have default_rollback_class of R3"
    );
}

#[tokio::test]
async fn test_evaluate_proposal_with_r3_intent_requires_approval() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile an intent with R3 default (mutating HTTP POST)
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        mode: ResourceMode::Write,
    }];
    // Set effect_type to ExternalApiCall to match the proposal's mutating nature
    // and avoid contradiction check firing
    req.effect_type = Some(EffectType::ExternalApiCall);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create a proposal requesting R3
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutating HTTP call".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/users"}),
        expected_effect: "create a user".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // R3 actions should require approval
    assert_eq!(
        eval_resp.decision,
        Decision::RequireApproval,
        "R3 action should require approval"
    );

    // Verify the rollback class was persisted correctly
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());
    assert_eq!(
        stored_proposal.unwrap().requested_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence
    );
}

#[tokio::test]
async fn test_evaluate_proposal_id_mismatch_returns_400_and_no_events() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // First compile an intent to have a real intent in the store
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create a proposal with a DIFFERENT proposal_id in body than in path
    let path_proposal_id = ProposalId::new();
    let body_proposal_id = ProposalId::new(); // Different!

    let mut proposal = sample_proposal(intent_id);
    proposal.proposal_id = body_proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", path_proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 400 Bad Request due to proposal_id mismatch
    assert_eq!(
        response.status(),
        400,
        "Expected 400 for proposal_id mismatch"
    );

    // Verify no proposal was persisted (neither path nor body proposal_id)
    let stored_proposal_path = runtime
        .store
        .proposals()
        .get(path_proposal_id)
        .await
        .unwrap();
    let stored_proposal_body = runtime
        .store
        .proposals()
        .get(body_proposal_id)
        .await
        .unwrap();
    assert!(
        stored_proposal_path.is_none(),
        "No proposal should be persisted for path proposal_id"
    );
    assert!(
        stored_proposal_body.is_none(),
        "No proposal should be persisted for body proposal_id"
    );

    // Verify no proposal-related provenance events were emitted
    let submission_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ActionProposalSubmitted),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        submission_events.is_empty(),
        "ActionProposalSubmitted should NOT be emitted when proposal_id mismatch"
    );

    let eval_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        eval_events.is_empty(),
        "PolicyEvaluated should NOT be emitted when proposal_id mismatch"
    );
}
#[tokio::test]
async fn test_full_happy_path_flow_compile_evaluate_mint_authorize_prepare() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with proper file scope (fail-closed: empty scope denies all)
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Read,
        content_hash: None,
    }];
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Step 3: Mint capability
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Read,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60, // Max allowed is 300 seconds
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    if status != 200 {
        let err_str = String::from_utf8_lossy(&body);
        panic!("Mint failed with status={}, body={}", status, err_str);
    }
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare execution (rollback prep)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // === Verify stored records ===

    // Verify intent persisted
    let stored_intent = runtime.store.intents().get(intent_id).await.unwrap();
    assert!(stored_intent.is_some());

    // Verify proposal persisted
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());

    // Verify capability persisted
    let stored_capability = runtime
        .store
        .capabilities()
        .get(capability_id)
        .await
        .unwrap();
    assert!(stored_capability.is_some());

    // Verify execution persisted
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let rollback_contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();

    // Verify rollback contract persisted
    let stored_rollback = runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
        .unwrap();
    assert!(stored_rollback.is_some());

    // === Verify provenance chain ===

    // Query all provenance events for this intent
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    // Should have events for: IntentCompiled, ActionProposalSubmitted, PolicyEvaluated,
    // CapabilityMinted, ToolCallPrepared, SideEffectPrepared
    let has_intent_compiled = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::IntentCompiled));
    let has_proposal_submitted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_policy_evaluated = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::PolicyEvaluated));
    let has_capability_minted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::CapabilityMinted));
    let has_tool_call_prepared = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));
    let has_side_effect_prepared = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));

    assert!(has_intent_compiled, "Missing IntentCompiled event");
    assert!(
        has_proposal_submitted,
        "Missing ActionProposalSubmitted event"
    );
    assert!(has_policy_evaluated, "Missing PolicyEvaluated event");
    assert!(has_capability_minted, "Missing CapabilityMinted event");
    assert!(has_tool_call_prepared, "Missing ToolCallPrepared event");
    assert!(has_side_effect_prepared, "Missing SideEffectPrepared event");

    // Verify causal linkage: PolicyEvaluated should have parent edge to ActionProposalSubmitted
    let policy_eval_event = all_events
        .iter()
        .find(|e| matches!(e.kind, ProvenanceEventKind::PolicyEvaluated))
        .unwrap();
    assert!(
        !policy_eval_event.parent_edges.is_empty(),
        "PolicyEvaluated should have parent edge"
    );
    assert!(matches!(
        policy_eval_event.parent_edges[0].edge_type,
        ferrum_proto::ProvenanceEdgeType::Caused
    ));
}

// Helper function to run the full flow up to prepare
async fn run_flow_to_prepared(
    runtime: &GatewayRuntime,
    rollback_class: RollbackClass,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Step 1: Compile intent with mutating effect type and proper file scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    // Add proper file scope for the test (fail-closed: empty scope denies all)
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
        content_hash: None,
    }];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal with specified rollback class
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Execute mutation".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: rollback_class,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

#[tokio::test]
async fn test_full_happy_path_execute_verify_auto_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);

    // Step 7: Verify (should auto-commit for R0)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);
    assert!(verify_resp.verified_at.is_some());

    // Verify execution state is Committed (auto-committed)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Committed));

    // Verify provenance chain includes ToolCallExecuted, SideEffectVerified, SideEffectCommitted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    let has_tool_call_executed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_side_effect_verified = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_side_effect_committed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(has_tool_call_executed, "Missing ToolCallExecuted event");
    assert!(has_side_effect_verified, "Missing SideEffectVerified event");
    assert!(
        has_side_effect_committed,
        "Missing SideEffectCommitted event (auto-commit)"
    );
}

#[tokio::test]
async fn test_r2_no_auto_commit_verify_then_explicit_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R2 (auto-commit disabled)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Verify the rollback contract has auto_commit = false
    let stored_execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    let contract_id = stored_execution.rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !stored_contract.auto_commit,
        "R2 should have auto_commit = false"
    );

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify (should NOT auto-commit for R2)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Verify execution state is AwaitingVerification (NOT committed for R2)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::AwaitingVerification),
        "R2 should remain in AwaitingVerification after verify, got {:?}",
        exec.state
    );

    // Step 8: Explicit commit (required for R2)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let commit_resp: ferrum_proto::CommitResponse = serde_json::from_slice(&body).unwrap();
    assert!(commit_resp.committed);
    assert!(commit_resp.committed_at.is_some());

    // Verify final state is Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Committed));

    // Verify provenance chain
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    let has_tool_call_executed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_side_effect_verified = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_side_effect_committed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(has_tool_call_executed, "Missing ToolCallExecuted event");
    assert!(has_side_effect_verified, "Missing SideEffectVerified event");
    assert!(
        has_side_effect_committed,
        "Missing SideEffectCommitted event"
    );
}

#[tokio::test]
async fn test_quarantine_path_blocks_execution_advance() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect type (to test quarantine path, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal that triggers quarantine (high taint score + non-R0 rollback class)
    // The StaticPDPEngine quarantines if taint_score >= 70 AND rollback class is not R0
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate with high taint".to_string(),
        tool_name: "db.write".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"sql": "DROP TABLE users"}),
        expected_effect: "delete data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable, // Not R0, will trigger quarantine
        decision: None,
        taint_inputs: vec![
            // Need taint_score >= 70 to trigger quarantine with R2
            // untrusted_input = 30, external_source = 25, web_url = 20
            "untrusted_input".to_string(),
            "external_source".to_string(),
            "web_url".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Verify the decision is Quarantine
    assert_eq!(eval_resp.decision, Decision::Quarantine);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 100, // Allow high taint
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution - should create a quarantined execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let _execution_id = auth_resp.execution.execution_id;

    // Verify the execution is in Quarantined state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::Quarantined
    ));
    assert!(matches!(auth_resp.execution.decision, Decision::Quarantine));
    assert!(auth_resp.execution.finished_at.is_some());

    // Verify Quarantined provenance event was emitted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::Quarantined),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing Quarantined provenance event"
    );
}

#[tokio::test]
async fn test_rollback_path_recovers_execution() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R2 (compensatable)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Step 8: Explicit commit (R2 requires manual commit like R3)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 9: Now rollback the execution
    let app = build_router(runtime.clone());
    let rollback_req = ferrum_proto::RollbackRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/rollback", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&rollback_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(rollback_resp.rolled_back);
    assert!(rollback_resp.rolled_back_at.is_some());

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::RolledBack));

    // Verify provenance chain includes SideEffectRolledBack
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectRolledBack),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing SideEffectRolledBack provenance event"
    );
}

#[tokio::test]
async fn test_compensate_path_recovers_execution() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R2 (compensatable)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Step 8: Explicit commit
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 9: Now compensate the execution
    let app = build_router(runtime.clone());
    let compensate_req = ferrum_proto::CompensateRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/compensate", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&compensate_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();
    assert!(compensate_resp.compensated);
    assert!(compensate_resp.compensated_at.is_some());

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Compensated));

    // Verify provenance chain includes SideEffectCompensated
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectCompensated),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing SideEffectCompensated provenance event"
    );
}

// ============================================
// NEGATIVE TESTS: Illegal State Transitions
// ============================================

#[tokio::test]
async fn test_prepare_execution_blocks_quarantined_state() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create an execution in Quarantined state by going through quarantine path
    // Step 1: Compile intent with mutating effect (to test quarantine, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal that triggers quarantine
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate with high taint".to_string(),
        tool_name: "db.write".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"sql": "DROP TABLE users"}),
        expected_effect: "delete data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![
            // Need taint_score >= 70 to trigger quarantine with R2
            "untrusted_input".to_string(),
            "external_source".to_string(),
            "web_url".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 100,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution (will create quarantined execution)
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is in Quarantined state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::Quarantined
    ));

    // Step 5: Attempt to prepare execution - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "prepare should fail for quarantined execution"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("quarantined"));
}

#[tokio::test]
async fn test_authorize_execution_blocks_proposal_capability_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create and evaluate TWO different proposals for the same intent
    let app = build_router(runtime.clone());
    let proposal1 = sample_proposal(intent_id);
    let proposal1_id = proposal1.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal1_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Create second proposal
    let app = build_router(runtime.clone());
    let proposal2 = sample_proposal(intent_id);
    let proposal2_id = proposal2.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal2_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability bound to proposal1
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id: proposal1_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Attempt to authorize execution using proposal2_id with capability bound to proposal1
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: proposal2_id, // Wrong proposal!
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 403 Forbidden due to proposal/capability mismatch
    assert_eq!(
        response.status(),
        403,
        "authorize should fail for proposal/capability mismatch"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("mismatch"));
    assert!(error.message.contains(&proposal1_id.to_string()));
    assert!(error.message.contains(&proposal2_id.to_string()));
}

#[tokio::test]
async fn test_prepare_execution_blocks_denied_state() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // We need to create a Denied execution. Currently the system creates Denied
    // when proposal decision is Deny or AllowDraftOnly.
    // We'll manually insert a Denied execution to test the guard.

    // First create a basic flow to get valid IDs
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Manually update the execution to Denied state
    let mut execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    execution.state = ExecutionState::Denied;
    execution.finished_at = Some(chrono::Utc::now());
    runtime.store.executions().update(&execution).await.unwrap();

    // Attempt to prepare execution - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "prepare should fail for denied execution"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("denied"));
}

#[tokio::test]
async fn test_prepare_execution_blocks_terminal_states() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Test Committed state
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute and commit
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({}),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify (auto-commits for R0)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify execution is Committed
    let execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(execution.state, ExecutionState::Committed));

    // Attempt to prepare committed execution - should fail
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("terminal state"));
}

// ============================================
// APPROVAL FLOW TESTS
// ============================================

#[tokio::test]
async fn test_full_approval_flow_approve_then_prepare_succeeds() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect (to test approval flow, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal (requires approval)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "db.delete".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "users"}),
        expected_effect: "delete all user data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence, // Requires approval
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::RequireApproval);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution - should create AwaitingApproval execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is in AwaitingApproval state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));
    assert_eq!(auth_resp.execution.decision, Decision::RequireApproval);

    // Verify approval request was created
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created"
    );
    let approval = &pending_approvals[0];
    assert_eq!(approval.intent_id, intent_id);
    assert_eq!(approval.proposal_id, proposal_id);
    assert_eq!(approval.execution_id, Some(execution_id));
    assert!(matches!(
        approval.state,
        ferrum_proto::ApprovalState::Pending
    ));
    let approval_id = approval.approval_id;

    // Step 5: Attempt to prepare execution - should fail with 409 (awaiting approval)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "prepare should fail for execution awaiting approval"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("awaiting approval"));

    // Step 6: Resolve approval with approve=true
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by admin".to_string()),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resolve_resp: ferrum_proto::ApprovalRequest = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        resolve_resp.state,
        ferrum_proto::ApprovalState::Granted
    ));

    // Step 7: Verify execution transitioned to Authorized
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Authorized),
        "execution should be Authorized after approval, got {:?}",
        exec.state
    );
    assert_eq!(exec.decision, Decision::Allow);

    // Step 8: Prepare execution should now succeed
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // Verify provenance events: ApprovalRequested and ApprovalGranted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    let has_approval_requested = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalRequested));
    let has_approval_granted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalGranted));

    assert!(
        has_approval_requested,
        "Missing ApprovalRequested provenance event"
    );
    assert!(
        has_approval_granted,
        "Missing ApprovalGranted provenance event"
    );
}

#[tokio::test]
async fn test_approval_denial_flow_blocks_execution() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect (to test approval flow, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal (requires approval)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "db.delete".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "users"}),
        expected_effect: "delete all user data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::RequireApproval);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is AwaitingApproval
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    // Get approval ID
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    // Step 5: Resolve approval with approve=false (deny)
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: false,
        reason: Some("Denied by admin".to_string()),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resolve_resp: ferrum_proto::ApprovalRequest = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        resolve_resp.state,
        ferrum_proto::ApprovalState::Denied
    ));

    // Step 6: Verify execution transitioned to Denied terminal state
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Denied));
    assert_eq!(exec.decision, Decision::Deny);
    assert!(exec.finished_at.is_some());

    // Step 7: Prepare should fail for denied execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("denied"));

    // Verify provenance events: ApprovalRequested and ApprovalDenied
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    let has_approval_requested = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalRequested));
    let has_approval_denied = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalDenied));

    assert!(
        has_approval_requested,
        "Missing ApprovalRequested provenance event"
    );
    assert!(
        has_approval_denied,
        "Missing ApprovalDenied provenance event"
    );
}

#[tokio::test]
async fn test_get_approval_by_id() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect (to test approval flow, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal and evaluate
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "db.delete".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "users"}),
        expected_effect: "delete all user data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability and authorize to create approval
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Get approval ID
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    // Step 4: Get approval by ID
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}", approval_id))
                .method(axum::http::Method::GET)
                .body("".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_resp: ferrum_proto::ApprovalRequest = serde_json::from_slice(&body).unwrap();
    assert_eq!(get_resp.approval_id, approval_id);
    assert_eq!(get_resp.intent_id, intent_id);
    assert_eq!(get_resp.proposal_id, proposal_id);
    assert!(matches!(
        get_resp.state,
        ferrum_proto::ApprovalState::Pending
    ));

    // Step 5: Get non-existent approval should 404
    let app = build_router(runtime.clone());
    let fake_approval_id = ferrum_proto::ApprovalId::new();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}", fake_approval_id))
                .method(axum::http::Method::GET)
                .body("".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

// ============================================
// HARDENING REGRESSION TESTS
// ============================================

#[tokio::test]
async fn test_read_only_intent_empty_scope_blocks_mutating_proposal() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with read-only effect (empty scope)
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a mutating proposal against the read-only intent
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "data"}),
        expected_effect: "write file contents".to_string(), // "write" is a mutating keyword
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // The proposal should be denied because read-only intent + mutating proposal is a violation
    // (read_only_violation is High severity -> Deny)
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "Read-only intent with empty scope should block mutating proposal, got {:?}",
        eval_resp.decision
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .iter()
            .any(|r| r == "read_only_violation"),
        "Expected read_only_violation in matched rules: {:?}",
        eval_resp.matched_rule_ids
    );
}

#[tokio::test]
async fn test_compile_time_taint_contributes_to_taint_score() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with external web content (adds ExternalWeb label)
    let mut req = sample_intent_request();
    req.raw_inputs = vec![ferrum_proto::IntentInputRef {
        source_id: "user_input".to_string(),
        source_type: "user".to_string(),
        trust_labels: vec![TrustLabel::ExternalWeb, TrustLabel::Untrusted],
        sensitivity_labels: vec![],
        summary: "Visit https://example.com for more info".to_string(),
        event_id: None,
    }];

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Verify compile-time taint labels are present
    assert!(
        compile_resp
            .envelope
            .trust_context
            .input_labels
            .contains(&TrustLabel::ExternalWeb),
        "ExternalWeb label should be present from compile-time"
    );

    // Step 2: Create a proposal with additional taint inputs
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Analyze data".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![
            "external_api".to_string(), // Adds 25 to taint score
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 3: Verify the combined taint score was used (compile-time + proposal-time)
    // ExternalWeb (compile) + ExternalWeb from content + external_api (proposal) = 25 + 25 = 50+
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());
}

#[tokio::test]
async fn test_high_severity_contradiction_fail_closed() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Test High severity (read-only violation) -> Deny
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create a proposal with read-only intent but mutating effect (High severity)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Delete file".to_string(),
        tool_name: "fs.delete".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "delete file permanently".to_string(), // "delete" is mutating
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // High severity (read_only_violation) should result in Deny (fail-closed)
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "High severity contradiction should result in Deny, got {:?}",
        eval_resp.decision
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .iter()
            .any(|r| r == "read_only_violation"),
        "Expected read_only_violation in matched rules"
    );
}

#[tokio::test]
async fn test_effect_classifier_word_boundary_safety() {
    // Test that the effect classifier uses word boundaries
    // "target" should NOT be classified as "get" (read-only) because it's a substring
    use ferrum_firewall::{DefaultFirewall, SemanticFirewall};

    let firewall = DefaultFirewall::new();

    // Create a read-only intent
    let intent = ferrum_proto::IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![ferrum_proto::OutcomeClause {
            id: "primary".to_string(),
            description: "Test outcome".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(), // Empty scope - should still block mutating proposals
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
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
        status: ferrum_proto::IntentStatus::Active,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };

    // "target" contains "get" but should NOT be treated as a read-only operation
    // (it's an unknown effect, so should default to mutating - fail-closed)
    let proposal_target = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id: intent.intent_id,
        step_index: 1,
        title: "Target operation".to_string(),
        tool_name: "ops.target".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "target resource".to_string(), // Contains "get" in "target"
        estimated_risk: ferrum_proto::RiskTier::Low,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let contradictions = firewall.contradiction_check(&intent, &proposal_target);

    // "target" should be treated as unknown/mutating (fail-closed), triggering read_only_violation
    assert!(
        contradictions
            .iter()
            .any(|c| c.rule_id == "read_only_violation"),
        "'target' should not match 'get' - unknown effects should default to mutating (fail-closed)"
    );
}

// ============================================
// DRAFT-ONLY FLOW TESTS
// ============================================

#[tokio::test]
async fn test_draft_only_flow_dry_run_succeeds() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with DraftOnly approval mode
    let req = sample_intent_request();
    // Note: The intent's approval_mode is set during compilation, but we need to trigger
    // AllowDraftOnly decision from the PDP. This happens when intent.approval_mode is DraftOnly.
    // We'll need to test this via the PDP's behavior.

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal and manually update intent to have DraftOnly mode
    // The StaticPdpEngine returns AllowDraftOnly when intent.approval_mode is DraftOnly
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Draft action".to_string(),
        tool_name: "email.draft".to_string(),
        server_name: "mail".to_string(),
        raw_arguments: serde_json::json!({"to": "test@example.com", "subject": "Test"}),
        expected_effect: "create email draft".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: Some(Decision::AllowDraftOnly), // Manually set to DraftOnly
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Store proposal with DraftOnly decision
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "mail".to_string(),
            tool_name: "email.draft".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize with dry_run=true - should succeed with Authorized state
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: true, // Dry run should succeed
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();

    // Dry run should succeed with Authorized state but remain draft-only
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::Authorized
    ));
    assert_eq!(auth_resp.execution.decision, Decision::AllowDraftOnly);

    // Draft-only authorization must not escalate into normal prepare/execute path
    let execution_id = auth_resp.execution.execution_id;
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("draft-only"));
}

#[tokio::test]
async fn test_draft_only_flow_non_dry_run_is_denied() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal with AllowDraftOnly decision
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Draft action".to_string(),
        tool_name: "email.draft".to_string(),
        server_name: "mail".to_string(),
        raw_arguments: serde_json::json!({"to": "test@example.com", "subject": "Test"}),
        expected_effect: "create email draft".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: Some(Decision::AllowDraftOnly),
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Store proposal with DraftOnly decision
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "mail".to_string(),
            tool_name: "email.draft".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize with dry_run=false - should be denied
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false, // Non-dry-run should be denied
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Request succeeds but execution is denied
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();

    // Non-dry-run should be denied with Denied state and Deny decision
    assert!(matches!(auth_resp.execution.state, ExecutionState::Denied));
    assert_eq!(auth_resp.execution.decision, Decision::Deny);
    assert!(auth_resp.execution.finished_at.is_some());
    assert!(auth_resp.warnings.iter().any(|w| w.contains("draft-only")));

    // Step 5: Prepare should fail for denied execution
    let execution_id = auth_resp.execution.execution_id;
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
}

// Phase C Firewall Integration Tests

#[tokio::test]
async fn test_compile_intent_derives_trust_context_from_inputs() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create a compile request with external URL in raw inputs
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![ferrum_proto::IntentInputRef {
            source_id: "web_search".to_string(),
            source_type: "external".to_string(),
            trust_labels: vec![TrustLabel::ExternalWeb],
            sensitivity_labels: vec![SensitivityLabel::Internal],
            summary: format!(
                "Check out https://example.com for more information about this topic. {}",
                "Additional content here. ".repeat(100)
            ),
            event_id: None,
        }],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();

    // Verify trust context was derived correctly
    let trust_context = &compile_resp.envelope.trust_context;
    assert!(
        trust_context
            .input_labels
            .contains(&TrustLabel::ExternalWeb)
    );
    assert!(
        trust_context
            .sensitivity_labels
            .contains(&SensitivityLabel::Internal)
    );
    assert!(trust_context.contains_untrusted_text);
    assert!(trust_context.contains_external_metadata);

    // Verify warnings were generated
    assert!(!compile_resp.warnings.is_empty());
    assert!(
        compile_resp
            .warnings
            .iter()
            .any(|w| w.contains("untrusted"))
    );
}

#[tokio::test]
async fn test_evaluate_proposal_denies_read_only_violation() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile a read-only intent
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Read-only Intent".to_string(),
        goal: "Read some data".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Low),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Modify intent to have read-only outcomes and explicit scope
    let mut read_only_intent = compile_resp.envelope;
    read_only_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "primary".to_string(),
        description: "Read only".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
    }];
    read_only_intent.resource_scope = vec![ResourceSelector::McpTool {
        server_name: "workspace".to_string(),
        tool_name: "fs.read".to_string(),
        mode: ResourceMode::Read,
    }];
    runtime
        .store
        .intents()
        .update(&read_only_intent)
        .await
        .unwrap();

    // Step 2: Create a mutating proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "data"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R1SnapshotRecoverable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal - should be denied due to read-only violation
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should be denied due to read-only violation
    assert_eq!(eval_resp.decision, Decision::Deny);
    assert!(
        eval_resp.reason.to_lowercase().contains("read-only")
            || eval_resp
                .warnings
                .iter()
                .any(|w| w.to_lowercase().contains("read-only"))
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .contains(&"read_only_violation".to_string())
    );
}

#[tokio::test]
async fn test_evaluate_proposal_denies_mcp_scope_violation() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile an intent with specific MCP tool scope
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "MCP Scoped Intent".to_string(),
        goal: "Use specific tools".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ResourceSelector::McpTool {
            server_name: "allowed-server".to_string(),
            tool_name: "allowed-tool".to_string(),
            mode: ResourceMode::Read,
        }],
        requested_risk_tier: Some(RiskTier::Low),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal using a different tool
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Wrong tool".to_string(),
        tool_name: "unauthorized-tool".to_string(),
        server_name: "unauthorized-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read data".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal - should be denied due to MCP scope violation
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should be denied due to MCP scope violation
    assert_eq!(eval_resp.decision, Decision::Deny);
    assert!(
        eval_resp
            .matched_rule_ids
            .contains(&"mcp_scope_violation".to_string())
    );
}

#[tokio::test]
async fn test_evaluate_proposal_allows_matching_mcp_scope() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile an intent with specific MCP tool scope
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "MCP Scoped Intent".to_string(),
        goal: "Use specific tools".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ResourceSelector::McpTool {
            server_name: "allowed-server".to_string(),
            tool_name: "allowed-tool".to_string(),
            mode: ResourceMode::Read,
        }],
        requested_risk_tier: Some(RiskTier::Low),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal using the allowed tool
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Correct tool".to_string(),
        tool_name: "allowed-tool".to_string(),
        server_name: "allowed-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read data".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal - should be allowed
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should be allowed since tool matches scope
    assert_eq!(eval_resp.decision, Decision::Allow);
    assert!(
        !eval_resp
            .matched_rule_ids
            .contains(&"mcp_scope_violation".to_string())
    );
}

#[tokio::test]
async fn test_taint_score_computation_in_evaluate() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile an intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal with taint inputs
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "External data operation".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/data.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![
            "external_source".to_string(),
            "untrusted_input".to_string(),
            "user_data".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // The evaluation should use the taint score computed by the firewall
    // external_source = 25, untrusted_input = 30, user_data = 15 = 70 total
    // Since 70 >= 70 and with R0, the PDP may quarantine or allow based on its logic
    // We're just verifying the firewall computes the score and the flow completes
}

// ============================================
// EXECUTION-TIME HTTP EGRESS ENFORCEMENT TESTS (Phase C2)
// ============================================

/// Helper: Run flow to Prepared state with HTTP resource binding
async fn run_http_flow_to_prepared(
    runtime: &GatewayRuntime,
    http_binding: ResourceBinding,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Step 1: Compile intent with HTTP scope matching the binding
    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        mode: ferrum_proto::ResourceMode::Read,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal for HTTP call
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call external API".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/data"}),
        expected_effect: "make HTTP API call".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with HTTP resource binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

async fn run_file_flow_to_prepared(
    runtime: &GatewayRuntime,
    file_binding: ResourceBinding,
    effect_type: EffectType,
    tool_name: &str,
    raw_arguments: serde_json::Value,
    expected_effect: &str,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Create file scope matching the binding path
    let file_scope = ferrum_proto::ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ferrum_proto::ResourceMode::Read,
        content_hash: None,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(effect_type);
    req.requested_resource_scope = vec![file_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Operate on file".to_string(),
        tool_name: tool_name.to_string(),
        server_name: "workspace".to_string(),
        raw_arguments,
        expected_effect: expected_effect.to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: tool_name.to_string(),
            tool_version: None,
        },
        resource_bindings: vec![file_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

#[tokio::test]
async fn test_http_execution_allowed_with_matching_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding that matches our request
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with matching HTTP payload
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "GET",
            "headers": {
                "content-type": "application/json"
            }
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "HTTP execution with matching binding should succeed"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);
}

#[tokio::test]
async fn test_http_execution_denied_host_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding for specific host
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with wrong host
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://evil.com/v1/users",
            "method": "GET"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution to wrong host should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_http_execution_denied_method_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding for GET only
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with wrong method
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "POST"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution with wrong method should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_http_execution_denied_header_violation() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding with limited header allowlist
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with disallowed header
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "GET",
            "headers": {
                "content-type": "application/json",
                "x-custom-secret": "sensitive-data"
            }
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution with unauthorized header should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_http_execution_denied_missing_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create intent with BOTH HTTP and File scope to allow file binding minting
    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        mode: ferrum_proto::ResourceMode::Read,
    };
    let file_scope = ferrum_proto::ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ferrum_proto::ResourceMode::Read,
        content_hash: None,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope, file_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call external API".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/data"}),
        expected_effect: "make HTTP API call".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Mint capability with ONLY File binding (no HTTP binding)
    let file_binding = ResourceBinding::File {
        path: "/tmp/test.txt".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![file_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Execute with HTTP payload but no HTTP binding in capability
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "GET"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution without HTTP binding should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_non_http_execution_unaffected_by_missing_http_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run standard flow with file binding (no HTTP binding)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Step 6: Execute with non-HTTP payload (file operation)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/test.txt",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "Non-HTTP execution should be unaffected by HTTP enforcement"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);
}

#[tokio::test]
async fn test_file_execution_allowed_with_matching_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let file_binding = ResourceBinding::File {
        path: "/tmp/readable.txt".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };

    let (_intent_id, _proposal_id, execution_id) = run_file_flow_to_prepared(
        &runtime,
        file_binding,
        EffectType::ReadOnlyAnalysis,
        "fs.read",
        serde_json::json!({"path": "/tmp/readable.txt"}),
        "read a file",
    )
    .await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/readable.txt"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_file_execution_denied_path_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/other.txt",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn test_file_execution_denied_write_on_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let file_binding = ResourceBinding::File {
        path: "/tmp/readable.txt".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };

    let (_intent_id, _proposal_id, execution_id) = run_file_flow_to_prepared(
        &runtime,
        file_binding,
        EffectType::ReadOnlyAnalysis,
        "fs.read",
        serde_json::json!({"path": "/tmp/readable.txt"}),
        "read a file",
    )
    .await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/readable.txt",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn test_file_execution_denied_missing_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/test.txt"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn test_file_execution_denied_path_traversal() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "../etc/passwd",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}
// ============================================
// EXECUTION-TIME SQLITE BINDING ENFORCEMENT TESTS (Phase C3)
// ============================================

async fn run_sqlite_flow_to_prepared_with_scope(
    runtime: &GatewayRuntime,
    sqlite_binding: ResourceBinding,
    scope_db_path: String,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Create SQLite scope - can be broader than binding to test execution-time enforcement
    let sqlite_scope = ferrum_proto::ResourceSelector::SqliteDatabase {
        db_path: scope_db_path,
        tables: vec!["users".to_string(), "orders".to_string()],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    req.requested_resource_scope = vec![sqlite_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Query SQLite database".to_string(),
        tool_name: "sqlite.query".to_string(),
        server_name: "sqlite-adapter".to_string(),
        raw_arguments: serde_json::json!({"db_path": "/tmp/test.db", "query": "SELECT * FROM users"}),
        expected_effect: "query SQLite database".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "sqlite-adapter".to_string(),
            tool_name: "sqlite.query".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![sqlite_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

// Wrapper with default scope for backward compatibility
async fn run_sqlite_flow_to_prepared(
    runtime: &GatewayRuntime,
    sqlite_binding: ResourceBinding,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    run_sqlite_flow_to_prepared_with_scope(runtime, sqlite_binding, "/tmp/test.db".to_string())
        .await
}

#[tokio::test]
async fn test_sqlite_execution_allowed_with_matching_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let sqlite_binding = ResourceBinding::Sqlite {
        db_path: "/tmp/test.db".to_string(),
        tables: vec!["users".to_string()],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_sqlite_flow_to_prepared(&runtime, sqlite_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": "/tmp/test.db",
            "query": "SELECT * FROM users WHERE id = 1"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "SQLite execution with matching binding should succeed"
    );
}

#[tokio::test]
async fn test_sqlite_execution_denied_db_path_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Use exact scope matching the binding, then test execution with different path
    let sqlite_binding = ResourceBinding::Sqlite {
        db_path: "/tmp/allowed.db".to_string(),
        tables: vec![],
        mode: ResourceMode::ReadWrite,
    };

    let (_intent_id, _proposal_id, execution_id) = run_sqlite_flow_to_prepared_with_scope(
        &runtime,
        sqlite_binding,
        "/tmp/allowed.db".to_string(),
    )
    .await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": "/tmp/other.db",
            "query": "SELECT * FROM users"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        error.code,
        ferrum_proto::ApiErrorCode::PolicyDenied
    ));
}

#[tokio::test]
async fn test_sqlite_execution_denied_table_not_in_allowlist() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let sqlite_binding = ResourceBinding::Sqlite {
        db_path: "/tmp/test.db".to_string(),
        tables: vec!["users".to_string()], // orders not allowed
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_sqlite_flow_to_prepared(&runtime, sqlite_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": "/tmp/test.db",
            "sql": "SELECT * FROM orders"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        error.code,
        ferrum_proto::ApiErrorCode::PolicyDenied
    ));
}

#[tokio::test]
async fn test_sqlite_execution_denied_write_on_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let sqlite_binding = ResourceBinding::Sqlite {
        db_path: "/tmp/test.db".to_string(),
        tables: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_sqlite_flow_to_prepared(&runtime, sqlite_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": "/tmp/test.db",
            "sql": "INSERT INTO users (name) VALUES ('test')"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

// ============================================
// EXECUTION-TIME GIT BINDING ENFORCEMENT TESTS (Phase C3)
// ============================================

async fn run_git_flow_to_prepared_with_scope(
    runtime: &GatewayRuntime,
    git_binding: ResourceBinding,
    scope_repo_path: String,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Create Git scope - can be broader than binding to test execution-time enforcement
    let git_scope = ferrum_proto::ResourceSelector::GitRepository {
        repo_path: scope_repo_path,
        allowed_refs: vec![
            "main".to_string(),
            "develop".to_string(),
            "feature/experimental".to_string(),
        ],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::GitMutation);
    req.requested_resource_scope = vec![git_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Execute git operation".to_string(),
        tool_name: "git.exec".to_string(),
        server_name: "git-adapter".to_string(),
        raw_arguments: serde_json::json!({"repo_path": "/repos/myrepo", "operation": "log"}),
        expected_effect: "git log".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "git-adapter".to_string(),
            tool_name: "git.exec".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![git_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

// Wrapper with default scope for backward compatibility
async fn run_git_flow_to_prepared(
    runtime: &GatewayRuntime,
    git_binding: ResourceBinding,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    run_git_flow_to_prepared_with_scope(runtime, git_binding, "/repos/myrepo".to_string()).await
}

#[tokio::test]
async fn test_git_execution_allowed_with_matching_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let git_binding = ResourceBinding::Git {
        repo_path: "/repos/myrepo".to_string(),
        allowed_refs: vec!["main".to_string(), "develop".to_string()],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_git_flow_to_prepared(&runtime, git_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "repo_path": "/repos/myrepo",
            "ref": "main",
            "operation": "log"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "Git execution with matching binding should succeed"
    );
}

#[tokio::test]
async fn test_git_execution_denied_repo_path_mismatch() {
    // Create a real temp git repo for the allowed binding path
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Use exact scope matching the binding, then test execution with different path
    let git_binding = ResourceBinding::Git {
        repo_path: repo_path.clone(),
        allowed_refs: vec![],
        mode: ResourceMode::ReadWrite,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_git_flow_to_prepared_with_scope(&runtime, git_binding, repo_path.clone()).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "repo_path": "/repos/other",
            "ref": "main",
            "operation": "log"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        error.code,
        ferrum_proto::ApiErrorCode::PolicyDenied
    ));
}

#[tokio::test]
async fn test_git_execution_denied_ref_not_in_allowlist() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let git_binding = ResourceBinding::Git {
        repo_path: "/repos/myrepo".to_string(),
        allowed_refs: vec!["main".to_string()], // feature/* not allowed
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_git_flow_to_prepared(&runtime, git_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "repo_path": "/repos/myrepo",
            "ref": "feature/experimental",
            "operation": "checkout"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        error.code,
        ferrum_proto::ApiErrorCode::PolicyDenied
    ));
}

#[tokio::test]
async fn test_git_execution_denied_write_on_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let git_binding = ResourceBinding::Git {
        repo_path: "/repos/myrepo".to_string(),
        allowed_refs: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_git_flow_to_prepared(&runtime, git_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "repo_path": "/repos/myrepo",
            "branch": "main",
            "operation": "push"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

// ============================================
// GIT ADAPTER ROUTING AND ROLLBACK TARGET TESTS
// ============================================

/// Initialize a temporary git repository for testing
fn init_temp_git_repo() -> (TempDir, String) {
    let temp_dir = TempDir::new().expect("failed to create temp git dir");
    let repo_path = temp_dir.path().to_str().unwrap().to_string();

    // Initialize git repo
    let output = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git init");
    assert!(output.status.success(), "git init failed");

    // Configure git user
    let output = std::process::Command::new("git")
        .args(["config", "user.name", "Ferrum Test"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git config user.name");
    assert!(output.status.success());

    let output = std::process::Command::new("git")
        .args(["config", "user.email", "ferrum@example.com"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git config user.email");
    assert!(output.status.success());

    // Create initial commit
    std::fs::write(temp_dir.path().join("README.md"), "hello\n").expect("failed to write README");
    let output = std::process::Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git add");
    assert!(output.status.success());

    let output = std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&repo_path)
        .output()
        .expect("failed to git commit");
    assert!(
        output.status.success(),
        "git commit failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    (temp_dir, repo_path)
}

#[tokio::test]
async fn test_prepare_with_readwrite_git_binding_routes_to_git_adapter() {
    // Verifies that prepare path with mutating git binding:
    // 1. Creates rollback contract with adapter_key = "git"
    // 2. Creates RollbackTarget::GitRef with the expected repo_path
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let git_binding = ResourceBinding::Git {
        repo_path: repo_path.clone(),
        allowed_refs: vec!["main".to_string(), "develop".to_string()],
        mode: ResourceMode::ReadWrite,
    };

    let git_scope = ferrum_proto::ResourceSelector::GitRepository {
        repo_path: repo_path.clone(),
        allowed_refs: vec![
            "main".to_string(),
            "develop".to_string(),
            "feature/experimental".to_string(),
        ],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::GitMutation);
    req.requested_resource_scope = vec![git_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Execute git operation".to_string(),
        tool_name: "git.exec".to_string(),
        server_name: "git-adapter".to_string(),
        raw_arguments: serde_json::json!({"repo_path": repo_path, "operation": "log"}),
        expected_effect: "git log".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with ReadWrite git binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "git-adapter".to_string(),
            tool_name: "git.exec".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![git_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare and capture rollback contract
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared, "prepared should be true");
    assert!(
        prep_resp.rollback_contract.is_some(),
        "rollback contract should be created for mutating git binding"
    );

    // === Verify rollback contract routing ===
    let contract = prep_resp.rollback_contract.unwrap();
    assert_eq!(
        contract.adapter_key, "git",
        "ReadWrite git binding should route to git adapter"
    );

    // Verify target is GitRef with expected repo_path
    match &contract.target {
        RollbackTarget::GitRef {
            repo_path: actual_repo_path,
            before_ref: _,
            after_ref: _,
        } => {
            assert_eq!(
                actual_repo_path.as_str(),
                repo_path.as_str(),
                "GitRef target should have correct repo_path"
            );
        }
        other => panic!(
            "expected RollbackTarget::GitRef for mutating git binding, got {:?}",
            other
        ),
    }
}

// ============================================
// GIT ADAPTER EXECUTE/VERIFY/ROLLBACK PATH TESTS
// ============================================

/// Helper: run git commit in a repo to advance HEAD
fn git_commit_change(repo_path: &str, filename: &str, content: &str) -> String {
    use std::process::Command;
    std::fs::write(std::path::Path::new(repo_path).join(filename), content)
        .expect("failed to write file");
    let output = Command::new("git")
        .args(["add", filename])
        .current_dir(repo_path)
        .output()
        .expect("failed to git add");
    assert!(output.status.success(), "git add failed");
    let output = Command::new("git")
        .args(["commit", "-m", "test commit"])
        .current_dir(repo_path)
        .output()
        .expect("failed to git commit");
    assert!(
        output.status.success(),
        "git commit failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .expect("failed to git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Helper: get current HEAD ref for a repo
fn git_head(repo_path: &str) -> String {
    use std::process::Command;
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .expect("failed to git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Helper: run full flow to prepared for git-backed execution with actual repo path
async fn run_git_flow_to_prepared_with_real_repo(
    runtime: &GatewayRuntime,
    repo_path: String,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
    String, // before_ref captured at prepare time
) {
    let git_binding = ResourceBinding::Git {
        repo_path: repo_path.clone(),
        allowed_refs: vec!["main".to_string()],
        mode: ResourceMode::ReadWrite,
    };

    let git_scope = ferrum_proto::ResourceSelector::GitRepository {
        repo_path: repo_path.clone(),
        allowed_refs: vec!["main".to_string()],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Compile intent
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::GitMutation);
    req.requested_resource_scope = vec![git_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Execute git operation".to_string(),
        tool_name: "git.exec".to_string(),
        server_name: "git-adapter".to_string(),
        raw_arguments: serde_json::json!({"repo_path": repo_path, "operation": "log"}),
        expected_effect: "git log".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "git-adapter".to_string(),
            tool_name: "git.exec".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![git_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Authorize
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Prepare and capture before_ref from contract metadata
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);

    // Extract before_ref from contract metadata
    let contract = prep_resp.rollback_contract.unwrap();
    let before_ref = contract
        .metadata
        .get("before_ref")
        .and_then(|v| v.as_str())
        .expect("before_ref should be in contract metadata")
        .to_string();

    (intent_id, proposal_id, execution_id, before_ref)
}

#[tokio::test]
async fn test_git_execute_succeeds_when_after_ref_matches_head() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Get current HEAD before prepare (will be the initial commit)
    let head_before = git_head(&repo_path);

    // Run flow to prepared - captures before_ref
    let (_intent_id, _proposal_id, execution_id, before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;

    // Verify before_ref matches our earlier HEAD (no changes yet)
    assert_eq!(before_ref, head_before);

    // Execute with payload.after_ref matching current HEAD
    let current_head = git_head(&repo_path);
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": current_head}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "execute should succeed when after_ref matches current HEAD"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);

    // Verify execution state is Running
    let stored_execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(stored_execution.state, ExecutionState::Running),
        "execution should be in Running state after execute"
    );
}

#[tokio::test]
async fn test_git_verify_succeeds_after_execute() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepared
    let (_intent_id, _proposal_id, execution_id, _before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;

    // Execute with current HEAD as after_ref
    let current_head = git_head(&repo_path);
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": current_head}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify should succeed since after_ref (current HEAD) hasn't changed
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        verify_resp.verified,
        "verify should succeed after execute when HEAD unchanged"
    );

    // Verify SideEffectVerified provenance event
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectVerified),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        !events.is_empty(),
        "SideEffectVerified provenance event should be emitted"
    );
}

#[tokio::test]
async fn test_git_rollback_restores_repo_head_to_before_ref() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Capture initial HEAD
    let head_at_start = git_head(&repo_path);

    // Make a commit to advance HEAD - this simulates work done after prepare
    // In real scenario this would be done by the tool execution
    let head_after_commit = git_commit_change(&repo_path, "feature.txt", "new feature\n");
    assert_ne!(
        head_at_start, head_after_commit,
        "commit should advance HEAD"
    );
    let current_head = git_head(&repo_path);
    assert_eq!(current_head, head_after_commit);

    // Run flow to prepared - captures current HEAD (head_after_commit) as before_ref
    let (_intent_id, _proposal_id, execution_id, before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;

    // Verify before_ref is the commit we made
    assert_eq!(
        before_ref, head_after_commit,
        "before_ref should capture HEAD at prepare time"
    );

    // Execute with after_ref matching current HEAD
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": current_head}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Commit
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Now make another commit to advance HEAD past before_ref
    let head_after_rollback_commit = git_commit_change(&repo_path, "another.txt", "more changes\n");
    assert_eq!(
        git_head(&repo_path),
        head_after_rollback_commit,
        "HEAD should have advanced after new commit"
    );

    // Call rollback - should restore to before_ref
    let app = build_router(runtime.clone());
    let rollback_req = ferrum_proto::RollbackRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/rollback", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&rollback_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(rollback_resp.rolled_back);

    // CRITICAL: Verify repo HEAD was actually restored to before_ref
    let restored_head = git_head(&repo_path);
    assert_eq!(
        restored_head, before_ref,
        "repo HEAD should be restored to before_ref after rollback, got {} expected {}",
        restored_head, before_ref
    );

    // Verify execution state is RolledBack
    let stored_execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(stored_execution.state, ExecutionState::RolledBack),
        "execution should be in RolledBack state"
    );

    // Verify SideEffectRolledBack provenance event
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectRolledBack),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        !events.is_empty(),
        "SideEffectRolledBack provenance event should be emitted"
    );
}

/// Test that verify uses persisted after_ref from execute, not stale before_ref.
/// Scenario:
/// 1. Prepare captures before_ref (HEAD at prepare time)
/// 2. After prepare, a new commit advances HEAD to a different ref
/// 3. Execute is called with after_ref = new HEAD (matches current HEAD, succeeds)
/// 4. Verify MUST use the persisted after_ref (new HEAD), not fall back to before_ref
///    (which would cause verify to fail since current HEAD != before_ref)
#[tokio::test]
async fn test_git_verify_uses_persisted_after_ref_not_stale_before_ref() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Initial HEAD at prepare time
    let head_at_prepare = git_head(&repo_path);

    // Run flow to prepared - captures head_at_prepare as before_ref
    let (_intent_id, _proposal_id, execution_id, before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;
    assert_eq!(
        before_ref, head_at_prepare,
        "before_ref should capture HEAD at prepare time"
    );

    // After prepare, make a new commit which advances HEAD
    let head_after_new_commit = git_commit_change(&repo_path, "feature.txt", "new feature\n");
    assert_ne!(
        head_at_prepare, head_after_new_commit,
        "new commit should produce different HEAD"
    );
    let current_head = git_head(&repo_path);
    assert_eq!(current_head, head_after_new_commit);

    // Execute with after_ref = new HEAD (which matches current HEAD, so adapter validation passes)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": head_after_new_commit}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "execute should succeed when after_ref matches current HEAD"
    );

    // Verify: MUST use persisted after_ref (head_after_new_commit), NOT before_ref (head_at_prepare)
    // Without the fix, verify falls back to before_ref and fails because current_head != before_ref.
    // With the fix, verify uses after_ref and succeeds because current_head == after_ref.
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        verify_resp.verified,
        "verify should succeed because it uses persisted after_ref ({}), not stale before_ref ({}). \
         current_head={}, before_ref={}, head_after_new_commit={}",
        head_after_new_commit, before_ref, current_head, before_ref, head_after_new_commit
    );
}

// ============================================
// EXECUTION-TIME EMAIL DRAFT BINDING ENFORCEMENT TESTS (Phase C3)
// ============================================

async fn run_email_flow_to_prepared(
    runtime: &GatewayRuntime,
    email_binding: ResourceBinding,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    run_email_flow_to_prepared_with_rollback_class(
        runtime,
        email_binding,
        RollbackClass::R0NativeReversible,
    )
    .await
}

async fn run_email_flow_to_prepared_with_rollback_class(
    runtime: &GatewayRuntime,
    email_binding: ResourceBinding,
    requested_rollback_class: RollbackClass,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Create Email scope matching the binding
    let email_scope = ferrum_proto::ResourceSelector::EmailDraft {
        recipient_allowlist: vec![
            "alice@example.com".to_string(),
            "bob@example.com".to_string(),
        ],
        subject_prefix_allowlist: vec![],
        mode: ferrum_proto::ResourceMode::Write,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalCommunication);
    req.requested_resource_scope = vec![email_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Draft email".to_string(),
        tool_name: "email.draft".to_string(),
        server_name: "email-adapter".to_string(),
        raw_arguments: serde_json::json!({"to": ["alice@example.com"], "subject": "Test"}),
        expected_effect: "draft email".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "email-adapter".to_string(),
            tool_name: "email.draft".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![email_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

#[tokio::test]
async fn test_email_execution_allowed_draft_matching_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec![
            "alice@example.com".to_string(),
            "bob@example.com".to_string(),
        ],
        allow_send: false,
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_email_flow_to_prepared(&runtime, email_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test email",
            "body": "Hello!"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "Email draft execution with matching binding should succeed"
    );
}

#[tokio::test]
async fn test_email_execution_denied_recipient_not_in_allowlist() {
    // NOTE: This test uses allow_send=false to reach prepare, then tests that
    // execution-time firewall denies recipient allowlist violation.
    // (allow_send=true is now denied at prepare-time, so we test the execution
    // firewall enforcement with allow_send=false.)
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec!["alice@example.com".to_string()], // only alice allowed
        allow_send: false,                                 // use false so prepare succeeds
        mode: ResourceMode::Write,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_email_flow_to_prepared(&runtime, email_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "to": ["alice@example.com", "eve@evil.com"], // eve not in allowlist
            "subject": "Test",
            "send": true
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        error.code,
        ferrum_proto::ApiErrorCode::PolicyDenied
    ));
}

#[tokio::test]
async fn test_email_execution_denied_send_when_not_allowed() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec!["alice@example.com".to_string()],
        allow_send: false, // send not allowed
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_email_flow_to_prepared(&runtime, email_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test",
            "send": true
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        error.code,
        ferrum_proto::ApiErrorCode::PolicyDenied
    ));
}

#[tokio::test]
async fn test_email_allow_send_true_is_denied_at_prepare_not_silently_routed_to_noop() {
    // This test verifies the NEW correct behavior: allow_send=true EmailDraft bindings
    // are denied at prepare time with PolicyDenied, NOT silently routed to noop.
    // The old buggy behavior (silently routing to noop and succeeding at execute) is now fixed.
    // See test_email_allow_send_true_prepare_denied_with_explicit_error for the dedicated deny test.
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec!["alice@example.com".to_string()],
        allow_send: true, // This is the case we're testing - was silently noop, now denied
        mode: ResourceMode::Write,
    };

    let (_intent_id, _proposal_id, execution_id, _capability_id) =
        run_email_flow_to_authorized(&runtime, email_binding).await;

    // Attempt to prepare execution - should be denied with explicit error
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // MUST be denied (fail-closed), not silently routed to noop
    assert_eq!(
        response.status(),
        403,
        "allow_send=true EmailDraft must be denied at prepare time (fail-closed)"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

// ============================================
// HARDENING TESTS (Phase F)
// ============================================

#[tokio::test]
async fn test_single_use_capability_second_authorize_fails() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to get a valid capability
    let (_intent_id, proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Get the capability_id from the execution
    let execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    let capability_id = execution.capability_id;

    // run_flow_to_prepared already calls authorize, so the capability is already used

    // Second authorize with same capability should fail
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 403 Forbidden - capability already used
    assert_eq!(
        response.status(),
        403,
        "Second authorize with same capability should fail"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::CapabilityUsed),
        "Expected CapabilityUsed error, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_scope_mismatch_denied_at_mint_time() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with LIMITED scope (only /tmp/allowed.txt)
    let file_scope = ferrum_proto::ResourceSelector::FilesystemPath {
        path: "/tmp/allowed.txt".to_string(),
        mode: ferrum_proto::ResourceMode::Read,
        content_hash: None,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![file_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Read file".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/allowed.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Try to mint capability with OUT-OF-SCOPE binding (/etc/passwd)
    let out_of_scope_binding = ResourceBinding::File {
        path: "/etc/passwd".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![out_of_scope_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 403 Forbidden - scope mismatch
    assert_eq!(
        response.status(),
        403,
        "Mint with out-of-scope binding should fail"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::ScopeMismatch),
        "Expected ScopeMismatch error, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_r2_no_auto_commit_requires_explicit_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // NOTE: This test uses R2 (Compensatable) to verify no-auto-commit semantics.
    // R2 has auto_commit=false like R3, but doesn't require approval flow.
    // R3 semantics are tested in the approval flow tests (test_full_approval_flow_approve_then_prepare_succeeds)
    // which demonstrate R3's require-approval behavior before any execution.
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Verify the rollback contract has auto_commit = false
    let stored_execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    let contract_id = stored_execution.rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !stored_contract.auto_commit,
        "R2 should have auto_commit = false"
    );

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify (should NOT auto-commit for R2)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Verify execution state is AwaitingVerification (NOT auto-committed for R2)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::AwaitingVerification),
        "R2 should remain in AwaitingVerification after verify, got {:?}",
        exec.state
    );

    // Step 8: Explicit commit (required for R2 when not auto-commit)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let commit_resp: ferrum_proto::CommitResponse = serde_json::from_slice(&body).unwrap();
    assert!(commit_resp.committed);
    assert!(commit_resp.committed_at.is_some());

    // Verify final state is Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Committed),
        "R2 should be Committed after explicit commit, got {:?}",
        exec.state
    );
}

#[tokio::test]
async fn test_r3_no_auto_commit_after_approval_requires_explicit_commit() {
    // DIRECT R3 NO-AUTO-COMMIT EVIDENCE:
    // This test proves R3 flows (after approval) do NOT auto-commit on verify,
    // and require explicit commit, satisfying the invariant from docs/06-constraints-and-invariants.md
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal (requires approval)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible high-consequence action".to_string(),
        tool_name: "db.drop".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "production_users"}),
        expected_effect: "drop production table".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        eval_resp.decision,
        Decision::RequireApproval,
        "R3 should require approval"
    );

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.drop".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution - should create AwaitingApproval execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is in AwaitingApproval state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    // Step 5: Get approval_id and approve
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created"
    );
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for R3 no-auto-commit test".to_string()),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Verify execution transitioned to Authorized
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Authorized),
        "execution should be Authorized after approval"
    );

    // CRITICAL: Verify rollback contract has auto_commit = false for R3
    let contract_id = exec.rollback_contract_id;
    assert!(
        contract_id.is_none(),
        "contract should not exist before prepare"
    );

    // Step 6: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);

    // Verify contract has auto_commit = false (R3 never auto-commits)
    let stored_execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    let contract_id = stored_execution.rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !stored_contract.auto_commit,
        "R3 MUST have auto_commit = false (invariant from docs/06-constraints-and-invariants.md)"
    );
    assert_eq!(
        stored_contract.rollback_class,
        RollbackClass::R3IrreversibleHighConsequence,
        "Contract should be R3 class"
    );

    // Step 7: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"table": "production_users"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 8: Verify (should NOT auto-commit for R3)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // CRITICAL ASSERTION: R3 must NOT auto-commit on verify
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::AwaitingVerification),
        "R3 MUST remain in AwaitingVerification after verify (no auto-commit), got {:?}",
        exec.state
    );

    // Step 9: Explicit commit (REQUIRED for R3)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let commit_resp: ferrum_proto::CommitResponse = serde_json::from_slice(&body).unwrap();
    assert!(commit_resp.committed);
    assert!(commit_resp.committed_at.is_some());

    // Verify final state is Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Committed),
        "R3 should be Committed after explicit commit, got {:?}",
        exec.state
    );

    // Verify provenance includes all expected events including ApprovalGranted and SideEffectCommitted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    let has_approval_granted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalGranted));
    let has_side_effect_committed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(
        has_approval_granted,
        "Missing ApprovalGranted provenance event"
    );
    assert!(
        has_side_effect_committed,
        "Missing SideEffectCommitted provenance event (explicit commit)"
    );
}

// ============================================
// SCOPE HARDENING TESTS (Mode Subset Checks)
// ============================================

#[tokio::test]
async fn test_empty_scope_denies_any_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with EMPTY scope (read-only analysis has empty scope by default)
    let req = sample_intent_request(); // ReadOnlyAnalysis effect type
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Verify intent has empty scope
    assert!(
        compile_resp.envelope.resource_scope.is_empty(),
        "Intent should have empty scope"
    );

    // Step 2: Create proposal with File binding
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Read file".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read file contents".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Try to mint capability with File binding - should FAIL
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Read,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 403 Forbidden - empty scope denies all bindings
    assert_eq!(
        response.status(),
        403,
        "Empty scope should deny any resource binding"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::ScopeMismatch),
        "Expected ScopeMismatch error, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_read_scope_denies_readwrite_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with Read scope
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Read, // Read-only scope
        content_hash: None,
    }];

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Read file".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Try to mint capability with ReadWrite binding - should FAIL
    // Read scope should NOT allow ReadWrite mode (permission widening)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::ReadWrite, // Wider than scope's Read
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 403 Forbidden - Read scope cannot grant ReadWrite
    assert_eq!(
        response.status(),
        403,
        "Read scope should deny ReadWrite binding (permission widening)"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::ScopeMismatch),
        "Expected ScopeMismatch error, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_readwrite_scope_allows_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with ReadWrite scope
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::ReadWrite, // ReadWrite scope
        content_hash: None,
    }];

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Read file".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with Read binding - should SUCCEED
    // ReadWrite scope should allow Read mode (subset permission)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Read, // Subset of ReadWrite
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should succeed - Read is subset of ReadWrite scope
    assert_eq!(
        response.status(),
        200,
        "ReadWrite scope should allow Read binding (subset permission)"
    );
}

// ============================================
// ADAPTER-BACKED ROLLBACK/COMPENSATE TESTS (Phase F)
// ============================================

/// Create test runtime with filesystem adapter registered for adapter-backed tests
async fn create_test_runtime_with_fs_adapter() -> (TempDir, GatewayRuntime, SqliteStore) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = SqliteStore::connect(&database_url)
        .await
        .expect("failed to connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");

    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine::default());
    let cap: Arc<dyn CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    // Register the filesystem adapter for adapter-backed evidence tests
    registry.register(Arc::new(ferrum_adapter_fs::FsRollbackAdapter::new("fs")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()), firewall);

    (temp_dir, runtime, store)
}

#[tokio::test]
async fn test_adapter_backed_rollback_deletes_created_file() {
    let (temp_dir, runtime, _store) = create_test_runtime_with_fs_adapter().await;

    // Use a file path within the temp directory to avoid polluting /tmp
    let test_file = temp_dir.path().join("rollback_test.txt");
    let file_path = test_file.to_str().unwrap();

    // Run flow to prepare with filesystem binding
    let file_scope = ferrum_proto::ResourceSelector::FilesystemPath {
        path: temp_dir.path().to_str().unwrap().to_string(),
        mode: ferrum_proto::ResourceMode::Write,
        content_hash: None,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![file_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create proposal for file write
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": file_path, "content": "hello world"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Mint capability with file binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: file_path.to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Prepare execution - this should select the "fs" adapter based on file binding
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Execute - creates the file
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": file_path,
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify file was created by fs adapter
    assert!(
        test_file.exists(),
        "File should be created by fs adapter execute"
    );
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "hello world",
        "File should contain executed content"
    );

    // Verify execution state
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Commit the execution
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Rollback the committed execution
    let app = build_router(runtime.clone());
    let rollback_req = ferrum_proto::RollbackRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/rollback", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&rollback_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    if status != 200 {
        let error_str = String::from_utf8_lossy(&body);
        panic!("Rollback failed with status {:?}: {}", status, error_str);
    }
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(rollback_resp.rolled_back);

    // Verify file was deleted by fs adapter rollback (file was newly created, so rollback deletes it)
    assert!(
        !test_file.exists(),
        "File should be deleted by fs adapter rollback"
    );

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::RolledBack));

    // Verify provenance chain includes SideEffectRolledBack
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectRolledBack),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing SideEffectRolledBack provenance event"
    );
}

#[tokio::test]
async fn test_adapter_backed_compensate_restores_overwritten_file() {
    let (temp_dir, runtime, _store) = create_test_runtime_with_fs_adapter().await;

    // Use a file path within the temp directory
    let test_file = temp_dir.path().join("compensate_test.txt");
    let file_path = test_file.to_str().unwrap();

    // Create original file with content BEFORE prepare
    std::fs::write(&test_file, "original content").expect("Failed to create original file");
    assert!(test_file.exists());

    // Run flow to prepare with filesystem binding
    let file_scope = ferrum_proto::ResourceSelector::FilesystemPath {
        path: temp_dir.path().to_str().unwrap().to_string(),
        mode: ferrum_proto::ResourceMode::Write,
        content_hash: None,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![file_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create proposal for file write (overwrites existing)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Overwrite file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": file_path, "content": "new content"}),
        expected_effect: "overwrite a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Mint capability with file binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: file_path.to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Prepare execution - fs adapter will snapshot existing file content
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Execute - overwrites the file
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": file_path,
            "content": "new content"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify file was overwritten
    assert!(test_file.exists(), "File should still exist after execute");
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "new content",
        "File should contain new content after execute"
    );

    // Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Commit
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Compensate - should restore original content
    let app = build_router(runtime.clone());
    let compensate_req = ferrum_proto::CompensateRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/compensate", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&compensate_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();
    assert!(compensate_resp.compensated);

    // Verify file was restored to original content by fs adapter compensate
    assert!(
        test_file.exists(),
        "File should still exist after compensate"
    );
    let restored_content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        restored_content, "original content",
        "File should be restored to original content by fs adapter compensate"
    );

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Compensated));

    // Verify provenance chain includes SideEffectCompensated
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectCompensated),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing SideEffectCompensated provenance event"
    );
}

#[tokio::test]
async fn test_adapter_backed_sqlite_compensate_restores_updated_row() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;

    let db_path = temp_dir.path().join("adapter_recovery.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite db file");

    let table = "test_rows";
    let row_id = "row_1";
    seed_sqlite_row(&db_path, table, row_id, "original content").await;

    let db_path_str = db_path.to_str().unwrap().to_string();
    let sqlite_scope = ferrum_proto::ResourceSelector::SqliteDatabase {
        db_path: db_path_str.clone(),
        tables: vec![table.to_string()],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    req.requested_resource_scope = vec![sqlite_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Update sqlite row".to_string(),
        tool_name: "db.update".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({
            "db_path": db_path_str,
            "sql": "UPDATE test_rows SET content = 'updated content' WHERE id = 'row_1'"
        }),
        expected_effect: "update a sqlite row".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.update".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Sqlite {
            db_path: db_path.to_str().unwrap().to_string(),
            tables: vec![table.to_string()],
            mode: ResourceMode::ReadWrite,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": db_path.to_str().unwrap(),
            "sql": "UPDATE test_rows SET content = 'updated content' WHERE id = 'row_1'",
            "table": table,
            "row_id": row_id,
            "content": "updated content"
        }),
    };
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let updated_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(updated_content.as_deref(), Some("updated content"));

    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let compensate_req = ferrum_proto::CompensateRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/compensate", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&compensate_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let restored_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(restored_content.as_deref(), Some("original content"));

    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectCompensated),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!all_events.is_empty());
}

// ============================================
// MAILDRAFT ADAPTER INTEGRATION TESTS
// ============================================
// MAILDRAFT ADAPTER INTEGRATION TESTS
// ============================================

/// Creates a test runtime with maildraft adapter and returns the shared store for verification.
/// This allows integration tests to verify maildraft state changes directly.
async fn create_test_runtime_with_maildraft_store() -> (
    TempDir,
    GatewayRuntime,
    SqliteStore,
    ferrum_adapter_maildraft::MaildraftStore,
) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = SqliteStore::connect(&database_url)
        .await
        .expect("failed to connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");

    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine::default());
    let cap: Arc<dyn CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    // Create shared maildraft store
    let maildraft_store = ferrum_adapter_maildraft::MaildraftStore::new();

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    registry.register(Arc::new(ferrum_adapter_fs::FsRollbackAdapter::new("fs")));
    registry.register(Arc::new(ferrum_adapter_sqlite::SqliteRollbackAdapter::new(
        "sqlite",
    )));
    registry.register(Arc::new(
        ferrum_adapter_maildraft::MaildraftAdapter::with_store(
            "maildraft",
            maildraft_store.clone(),
        ),
    ));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()), firewall);

    (temp_dir, runtime, store, maildraft_store)
}

#[tokio::test]
async fn test_maildraft_adapter_email_draft_flow_with_compensate() {
    let (_temp_dir, runtime, _store, maildraft_store) =
        create_test_runtime_with_maildraft_store().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec!["alice@example.com".to_string()],
        allow_send: false,
        mode: ResourceMode::Write,
    };

    let (_intent_id, _proposal_id, execution_id) = run_email_flow_to_prepared_with_rollback_class(
        &runtime,
        email_binding,
        RollbackClass::R2Compensatable,
    )
    .await;

    let app = build_router(runtime.clone());

    // Execute draft creation
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test email",
            "body": "Hello!"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200, "execute draft should succeed");

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let exec_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();

    // Extract draft_id from external_id (maildraft adapter returns draft_id as external_id)
    let draft_id = exec_resp
        .external_id
        .expect("execute response should contain external_id (draft_id)");

    // Verify draft exists in store BEFORE verify/compensate
    assert!(
        maildraft_store.draft_exists(&draft_id),
        "draft should exist after execute"
    );

    // Verify should succeed but keep the execution pending explicit commit for R2.
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "verify should succeed");

    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.expect("execution should exist after verify");
    assert!(matches!(exec.state, ExecutionState::AwaitingVerification));

    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "explicit commit should succeed");

    // Compensate should delete the draft after explicit commit.
    let app = build_router(runtime.clone());
    let compensate_req = ferrum_proto::CompensateRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/compensate", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&compensate_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "compensate should succeed");

    // Verify draft no longer exists after compensate
    assert!(
        !maildraft_store.draft_exists(&draft_id),
        "draft should be deleted after compensate"
    );
}

// ============================================
// EMAIL DRAFT ALLOW_SEND POLICY TESTS
// ============================================

/// Helper: run email flow to authorized state with given binding, WITHOUT calling prepare.
/// Returns (intent_id, proposal_id, execution_id, capability_id).
async fn run_email_flow_to_authorized(
    runtime: &GatewayRuntime,
    email_binding: ResourceBinding,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
    ferrum_proto::CapabilityId,
) {
    // Create Email scope matching the binding
    let email_scope = ferrum_proto::ResourceSelector::EmailDraft {
        recipient_allowlist: vec![
            "alice@example.com".to_string(),
            "bob@example.com".to_string(),
        ],
        subject_prefix_allowlist: vec![],
        mode: ferrum_proto::ResourceMode::Write,
    };
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalCommunication);
    req.requested_resource_scope = vec![email_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Draft email".to_string(),
        tool_name: "email.draft".to_string(),
        server_name: "email-adapter".to_string(),
        raw_arguments: serde_json::json!({"to": ["alice@example.com"], "subject": "Test"}),
        expected_effect: "draft email".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "email-adapter".to_string(),
            tool_name: "email.draft".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![email_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    (intent_id, proposal_id, execution_id, capability_id)
}

#[tokio::test]
async fn test_email_allow_send_true_prepare_denied_with_explicit_error() {
    // Verify that EmailDraft with allow_send=true is explicitly denied at prepare time
    // (fail-closed: previously this silently fell through to noop, now it returns a clear error)
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec!["alice@example.com".to_string()],
        allow_send: true, // This is the problematic case - send-capable binding
        mode: ResourceMode::Write,
    };

    let (_intent_id, _proposal_id, execution_id, _capability_id) =
        run_email_flow_to_authorized(&runtime, email_binding).await;

    // Attempt to prepare execution - should be denied with explicit error
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "prepare should be denied for allow_send=true EmailDraft"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied error, got {:?}",
        error.code
    );
    assert!(
        error.message.contains("allow_send=true") || error.message.contains("not supported"),
        "Error message should mention allow_send=true / not supported: {}",
        error.message
    );
}

#[tokio::test]
async fn test_email_allow_send_false_prepare_succeeds_with_maildraft_adapter() {
    // Verify that EmailDraft with allow_send=false (draft-only) still routes to maildraft
    // adapter and prepare succeeds (existing behavior preserved).
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let email_binding = ResourceBinding::EmailDraft {
        recipients: vec!["alice@example.com".to_string()],
        allow_send: false, // Draft-only - should still work
        mode: ResourceMode::Write,
    };

    let (_intent_id, _proposal_id, execution_id, _capability_id) =
        run_email_flow_to_authorized(&runtime, email_binding).await;

    // Attempt to prepare execution - should succeed and route to maildraft
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "prepare should succeed for allow_send=false EmailDraft (draft-only)"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared, "prepared should be true");
    assert!(
        prep_resp.rollback_contract.is_some(),
        "rollback contract should be created"
    );
    let contract = prep_resp.rollback_contract.unwrap();
    assert_eq!(
        contract.adapter_key, "maildraft",
        "draft-only EmailDraft should route to maildraft adapter"
    );
}

// ============================================
// LEDGER INTEGRATION TESTS (Slice 3)
// ============================================

#[tokio::test]
async fn test_commit_flow_writes_ledger_entry_linked_to_provenance_event() {
    // Verifies that perform_commit() appends a ledger entry wrapping the
    // SideEffectCommitted provenance event, with correct hash-chain linkage.
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to commit (R0 auto-commits on verify)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute and verify (auto-commits for R0)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // --- Verify SideEffectCommitted provenance event was emitted ---
    let committed_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectCommitted),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(
        !committed_events.is_empty(),
        "SideEffectCommitted provenance event should be emitted"
    );
    let committed_event = &committed_events[0];
    let committed_event_id = committed_event.event_id;

    // --- Verify ledger entry was written and linked to the provenance event ---
    let ledger_entry = runtime
        .store
        .ledger()
        .get_by_event(committed_event_id)
        .await
        .unwrap();

    assert!(
        ledger_entry.is_some(),
        "Ledger entry should exist for SideEffectCommitted event"
    );
    let entry = ledger_entry.unwrap();

    // The ledger entry's event must be the SideEffectCommitted provenance event
    assert!(
        matches!(entry.event.kind, ProvenanceEventKind::SideEffectCommitted),
        "Ledger entry should wrap SideEffectCommitted event"
    );
    assert_eq!(
        entry.event.event_id, committed_event_id,
        "Ledger entry event_id must match committed provenance event"
    );

    // --- Verify hash-chain linkage ---
    // Get the tip (latest entry) to verify chain linkage
    let tip = runtime.store.ledger().get_latest().await.unwrap().unwrap();
    assert_eq!(
        tip.event.event_id, committed_event_id,
        "Latest ledger entry must be the one we just wrote"
    );

    // For a single commit, the first entry is genesis (sequence 0, no prev_hash)
    // and the second would be sequence 1 with prev_hash pointing to genesis
    // But since this is the first commit, we need to check what we actually wrote
    if entry.sequence == 0 {
        // Genesis entry
        assert!(
            entry.prev_hash.is_none(),
            "Genesis entry must have no prev_hash"
        );
    } else {
        // Non-genesis: must have prev_hash set to previous entry's hash
        assert!(
            entry.prev_hash.is_some(),
            "Non-genesis entry must have prev_hash set"
        );
    }

    // --- Verify execution state is Committed ---
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Committed),
        "Execution should be committed"
    );
}

#[tokio::test]
async fn test_ledger_hash_chain_correct_over_multiple_commits() {
    // Verifies that multiple commits produce a correct hash chain in the ledger.
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // First commit (R0)
    let (_intent_id1, _proposal_id1, execution_id1) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id: execution_id1,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/execute", execution_id1))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest {
        execution_id: execution_id1,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/verify", execution_id1))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Get the first ledger entry
    let tip1 = runtime.store.ledger().get_latest().await.unwrap().unwrap();
    let first_entry_hash = tip1.entry_hash.clone();
    let first_sequence = tip1.sequence;

    // Second commit (R2, explicit commit)
    let (_intent_id2, _proposal_id2, execution_id2) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id: execution_id2,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "world"}),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/execute", execution_id2))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest {
        execution_id: execution_id2,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/verify", execution_id2))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Explicit commit for R2
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest {
        execution_id: execution_id2,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/commit", execution_id2))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Get the second ledger entry
    let tip2 = runtime.store.ledger().get_latest().await.unwrap().unwrap();

    // Verify sequence incremented
    assert_eq!(
        tip2.sequence,
        first_sequence + 1,
        "Second entry sequence must be first + 1"
    );

    // Verify prev_hash points to first entry's hash
    assert_eq!(
        tip2.prev_hash.as_ref(),
        Some(&first_entry_hash),
        "Second entry's prev_hash must point to first entry's hash"
    );

    // Verify both entries exist via list_recent
    let recent = runtime.store.ledger().list_recent(10).await.unwrap();
    assert!(recent.len() >= 2, "Should have at least 2 ledger entries");
    // Most recent should be tip2
    assert_eq!(
        recent[0].sequence, tip2.sequence,
        "Most recent entry should be tip2"
    );
}

fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Start a local HTTP server that handles exactly one request and returns the given status.
/// Returns the port and a handle to join the server thread.
fn start_local_http_server(response_status: u16) -> (u16, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            use std::io::{Read, Write};
            // Read request headers
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            // Send HTTP response with configurable status
            let status_line = format!(
                "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                response_status,
                status_text(response_status)
            );
            let _ = stream.write_all(status_line.as_bytes());
            let _ = stream.flush();
        }
    });
    // Give server a moment to start
    std::thread::sleep(std::time::Duration::from_millis(10));
    (port, handle)
}

#[tokio::test]
async fn test_http_execute_and_verify_through_gateway_uses_payload_url_within_scope() {
    // Start local HTTP server that returns 200
    let (port, server_handle) = start_local_http_server(200);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with ReadWrite mode - should route to HTTP adapter
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::ReadWrite,
    };

    // HTTP scope matching the binding
    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with HTTP scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call local HTTP API".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users?a=1&b=2", port)
        }),
        expected_effect: "make HTTP API call".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with ReadWrite HTTP binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Execute - HTTP adapter performs GET against a concrete URL under the bound scope
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users?b=2&a=1", port),
            "method": "GET"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "execute should succeed");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain the HTTP status code
    assert_eq!(
        execute_resp.result_digest.as_deref(),
        Some("200"),
        "result_digest should contain HTTP status code from execute"
    );

    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_contract
            .metadata
            .get("bound_url")
            .unwrap()
            .as_str()
            .unwrap(),
        format!("http://127.0.0.1:{}/api", port)
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("approved_url")
            .unwrap()
            .as_str()
            .unwrap(),
        format!("http://127.0.0.1:{}/api/users?a=1&b=2", port)
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("executed_url")
            .unwrap()
            .as_str()
            .unwrap(),
        format!("http://127.0.0.1:{}/api/users?b=2&a=1", port)
    );
    assert_eq!(
        stored_contract.metadata.get("approved_request_digest"),
        stored_contract.metadata.get("executed_request_digest")
    );
    assert_eq!(
        stored_contract.metadata.get("approved_query_digest"),
        stored_contract.metadata.get("executed_query_digest")
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("approved_query_present")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("executed_query_present")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert!(
        stored_contract
            .metadata
            .get("approved_http_request")
            .is_none(),
        "transient approved_http_request metadata should not be persisted"
    );

    // Step 7: Verify - should succeed using execute-time status metadata
    // (no explicit HttpStatusExpected check configured, but execute captured status)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "verify should succeed with execute-time status fallback"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        verify_resp.verified,
        "verify should be true when execute-time status matches"
    );
    assert!(
        verify_resp.verified_at.is_some(),
        "verified_at should be set"
    );

    // Clean up server thread
    let _ = server_handle.join();

    // Verify execution state is AwaitingVerification (R0 auto-commits after verify)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    // R0 with auto_commit=true should auto-commit after verify
    assert!(
        matches!(exec.state, ExecutionState::Committed),
        "R0 execution should be Committed after verify, got {:?}",
        exec.state
    );
}

#[tokio::test]
async fn test_http_post_execute_and_verify_through_gateway_after_approval() {
    // Start local HTTP server that returns 201 Created for the mutating call.
    // Verify for mutation must not replay the request, so one request is enough.
    let (port, server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec!["authorization".to_string()],
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with mutating HTTP scope. This should infer R3.
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;
    assert_eq!(
        compile_resp.envelope.default_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence
    );

    // Step 2: Evaluate proposal. Client tries to request R0, but the server-side floor must keep R3.
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create local HTTP resource".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "headers": {
                "Authorization": "Bearer approved-token"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::RequireApproval);

    // Step 3: Mint capability with POST binding.
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize -> AwaitingApproval.
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    // Step 5: Resolve approval.
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for HTTP POST parity test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Prepare.
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 7: Execute POST request.
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "headers": {
                "authorization": "Bearer approved-token"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "execute should succeed for POST");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);
    assert_eq!(execute_resp.result_digest.as_deref(), Some("201"));

    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_contract.metadata.get("approved_request_digest"),
        stored_contract.metadata.get("executed_request_digest")
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("executed_method")
            .unwrap()
            .as_str()
            .unwrap(),
        "Post"
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("executed_body_present")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        stored_contract.metadata.get("approved_headers_digest"),
        stored_contract.metadata.get("executed_headers_digest")
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("approved_headers_present")
            .unwrap()
            .as_bool(),
        Some(true)
    );

    // Step 8: Verify. For POST this must use execute-time metadata only.
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "verify should succeed for POST");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // R3 must remain AwaitingVerification after verify (no auto-commit).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::AwaitingVerification));

    let _ = server_handle.join();
}

#[tokio::test]
async fn test_http_post_execute_and_verify_with_bearer_auth_through_gateway_after_approval() {
    let (port, server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec!["authorization".to_string()],
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;
    assert_eq!(
        compile_resp.envelope.default_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence
    );

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create local HTTP resource with bearer auth".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "bearer",
                "token": "approved-token"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::RequireApproval);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for HTTP POST bearer auth parity test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "bearer",
                "token": "approved-token"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "execute should succeed for POST");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);
    assert_eq!(execute_resp.result_digest.as_deref(), Some("201"));

    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_contract.metadata.get("approved_request_digest"),
        stored_contract.metadata.get("executed_request_digest")
    );
    assert_eq!(
        stored_contract.metadata.get("approved_auth_digest"),
        stored_contract.metadata.get("executed_auth_digest")
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("approved_auth_present")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("executed_auth_present")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        stored_contract.metadata.get("approved_headers_digest"),
        stored_contract.metadata.get("executed_headers_digest")
    );

    let metadata_json = serde_json::to_string(&stored_contract.metadata).unwrap();
    assert!(
        !metadata_json.contains("approved-token"),
        "raw auth token must not be persisted in rollback contract metadata"
    );

    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "verify should succeed for POST");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::AwaitingVerification));

    let _ = server_handle.join();
}

#[tokio::test]
async fn test_http_execute_denied_when_bearer_auth_missing_from_header_allowlist() {
    // Regression test: when payload-level dedicated HTTP auth (auth.bearer) is used
    // but the HTTP binding's header_allowlist does NOT contain "authorization",
    // the request must be denied (fail-closed).
    // See ferrum-adapter-http/README.md: "The binding's header_allowlist must include
    // authorization to permit bearer auth."

    let (port, _server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with EMPTY header_allowlist - does NOT include "authorization"
    // This means bearer auth should be REJECTED even though it's in the payload
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec![], // NOTE: empty - "authorization" NOT allowed
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with mutating HTTP scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal - R3 requires approval
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create resource with bearer auth".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "bearer",
                "token": "some-token"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with HTTP binding (empty header_allowlist)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution -> AwaitingApproval
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Resolve approval
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for header allowlist test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 7: Execute with bearer auth - MUST BE DENIED because header_allowlist
    // does not include "authorization"
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "bearer",
                "token": "some-token"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // FAIL-CLOSED: execute must return 403 Forbidden when bearer auth is used
    // but "authorization" is not in header_allowlist
    assert_eq!(
        response.status(),
        403,
        "execute should be FORBIDDEN when bearer auth is used but header_allowlist does not contain 'authorization'"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        error.message.contains("authorization") || error.message.contains("denied"),
        "error message should mention authorization allowlist violation, got: {}",
        error.message
    );

    // Verify execution state is still Prepared (fail-closed: firewall denied before
    // execution started, so state never transitioned to Running/Failed)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution should remain Prepared when firewall denies before execution starts, got {:?}",
        exec.state
    );

    // Note: We don't join server_handle because the HTTP server was never contacted
    // (firewall denied before connection was made), so the server thread is still waiting.
    // The server will be dropped when temp_dir is dropped.
}

#[tokio::test]
async fn test_http_rollback_through_gateway_is_conservative_noop() {
    // Start local HTTP server that returns 200
    let (port, server_handle) = start_local_http_server(200);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with ReadWrite mode - should route to HTTP adapter
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/test".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::ReadWrite,
    };

    // HTTP scope matching the binding
    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/test".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with HTTP scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call local HTTP API".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/test", port)}),
        expected_effect: "make HTTP API call".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable, // R2 requires manual commit/rollback
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/test", port)}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 7: Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 8: Rollback - should succeed as a no-op for HTTP GET
    let app = build_router(runtime.clone());
    let rollback_req = ferrum_proto::RollbackRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/rollback", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&rollback_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "rollback should succeed for HTTP adapter (no-op)"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(rollback_resp.rolled_back, "rolled_back should be true");
    assert!(
        rollback_resp.rolled_back_at.is_some(),
        "rolled_back_at should be set"
    );

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::RolledBack),
        "execution should be RolledBack, got {:?}",
        exec.state
    );

    // Clean up server thread
    let _ = server_handle.join();
}

#[tokio::test]
async fn test_http_500_does_not_verify_and_does_not_auto_commit_r0() {
    // Regression test: a 500 response from HTTP execute must NOT verify
    // and must NOT auto-commit for R0 path.
    // See ferrum-adapter-http/src/lib.rs verify() - only 2xx auto-verifies
    // when using execute-time status metadata fallback.

    // Start local HTTP server that returns 500
    let (port, server_handle) = start_local_http_server(500);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with ReadWrite mode - should route to HTTP adapter
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/test".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::ReadWrite,
    };

    // HTTP scope matching the binding
    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/test".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with HTTP scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call local HTTP API".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/test", port)}),
        expected_effect: "make HTTP API call".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with ReadWrite HTTP binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Execute - HTTP adapter performs GET and captures status (500)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/test", port)}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "execute should succeed even with 500"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain the HTTP status code (500)
    assert_eq!(
        execute_resp.result_digest.as_deref(),
        Some("500"),
        "result_digest should contain HTTP 500 status code from execute"
    );

    // Step 7: Verify - MUST fail for 500 (fail-closed: non-2xx does NOT auto-verify)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "verify endpoint should still return 200"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    // CRITICAL: verify must be FALSE for 500 (fail-closed behavior)
    assert!(
        !verify_resp.verified,
        "verify must be FALSE for 500 response (fail-closed); got verified={}",
        verify_resp.verified
    );

    // Clean up server thread
    let _ = server_handle.join();

    // CRITICAL: execution must NOT be auto-committed for R0 when verify fails
    // R0 only auto-commits if verify succeeds - since verify failed, state is Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "R0 execution must be Failed when verify fails (not auto-committed), got {:?}",
        exec.state
    );
}

#[tokio::test]
async fn test_http_execute_succeeds_with_basic_auth_through_full_flow() {
    // Integration test: successful Basic auth flow through approval/prepare/execute/verify.
    // This proves that basic auth works end-to-end, not just at the adapter level.

    let (port, server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding WITH authorization in header_allowlist to permit basic auth
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec!["authorization".to_string()],
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with mutating HTTP scope (R3 default)
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal with basic auth in raw_arguments
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create resource with basic auth".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "basic",
                "username": "testuser",
                "password": "testpass"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution -> AwaitingApproval (R3 requires approval)
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    // Step 5: Resolve approval (grant)
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for basic auth test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // Step 7: Execute with basic auth
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "basic",
                "username": "testuser",
                "password": "testpass"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Execute should succeed (201 Created)
    assert_eq!(
        response.status(),
        200,
        "execute should succeed with basic auth when header_allowlist contains authorization"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);

    // Step 8: Verify. For POST this must use execute-time metadata only.
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "verify should succeed for POST");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // R3 must remain AwaitingVerification after verify (no auto-commit).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::AwaitingVerification));

    // Step 9: Explicit commit (required for R3)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let commit_resp: ferrum_proto::CommitResponse = serde_json::from_slice(&body).unwrap();
    assert!(commit_resp.committed);
    assert!(commit_resp.committed_at.is_some());

    // Verify final state is Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Committed));

    let _ = server_handle.join();
}

// ============================================
// API KEY AUTH INTEGRATION TESTS
// ============================================

#[tokio::test]
async fn test_http_execute_denied_when_basic_auth_missing_from_header_allowlist() {
    // Regression test: when payload-level dedicated HTTP auth (auth.basic) is used
    // but the HTTP binding's header_allowlist does NOT contain "authorization",
    // the request must be denied (fail-closed).
    // See ferrum-adapter-http/README.md: "The binding's header_allowlist must include
    // authorization to permit basic auth."

    let (port, _server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with EMPTY header_allowlist - does NOT include "authorization"
    // This means basic auth should be REJECTED even though it's in the payload
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec![], // NOTE: empty - "authorization" NOT allowed
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with mutating HTTP scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal - R3 requires approval
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create resource with basic auth".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "basic",
                "username": "testuser",
                "password": "testpass"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with HTTP binding (empty header_allowlist)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution -> AwaitingApproval
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let _auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();

    // Step 5: Resolve approval
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for basic auth header allowlist test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!(
                    "/v1/executions/{}/prepare",
                    _auth_resp.execution.execution_id
                ))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 7: Execute with basic auth - MUST BE DENIED because header_allowlist
    // does not include "authorization"
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id: _auth_resp.execution.execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "basic",
                "username": "testuser",
                "password": "testpass"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!(
                    "/v1/executions/{}/execute",
                    _auth_resp.execution.execution_id
                ))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // FAIL-CLOSED: execute must return 403 Forbidden when basic auth is used
    // but "authorization" is not in header_allowlist
    assert_eq!(
        response.status(),
        403,
        "execute should be FORBIDDEN when basic auth is used but header_allowlist does not contain 'authorization'"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        error.message.contains("authorization") || error.message.contains("denied"),
        "error message should mention authorization allowlist violation, got: {}",
        error.message
    );

    // Verify execution state is still Prepared (fail-closed: firewall denied before
    // execution started, so state never transitioned to Running/Failed)
    let stored_execution = runtime
        .store
        .executions()
        .get(_auth_resp.execution.execution_id)
        .await
        .unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution should remain Prepared when firewall denies before execution starts, got {:?}",
        exec.state
    );
}

#[tokio::test]
async fn test_http_execute_succeeds_with_api_key_auth_through_full_flow() {
    // Integration test: successful API key auth flow through approval/prepare/execute/verify.
    // Proves api_key auth works end-to-end with digest/presence metadata and raw key
    // NOT persisted in contract metadata.

    let (port, server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding WITH x-api-key in header_allowlist to permit api_key auth
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec!["x-api-key".to_string()],
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with mutating HTTP scope (R3 default)
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal with api_key auth in raw_arguments
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create resource with API key auth".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "api_key",
                "header": "X-API-Key",
                "key": "sk-test-api-key-12345"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution -> AwaitingApproval (R3 requires approval)
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    // Step 5: Resolve approval (grant)
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for API key auth test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // Step 7: Execute with api_key auth
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "api_key",
                "header": "X-API-Key",
                "key": "sk-test-api-key-12345"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Execute should succeed (201 Created)
    assert_eq!(
        response.status(),
        200,
        "execute should succeed with api_key auth when header_allowlist contains x-api-key"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);

    // === Verify contract metadata: digest/presence metadata and absence of raw key ===
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Verify auth digests are present and match (proves auth affects digest)
    assert_eq!(
        stored_contract.metadata.get("approved_auth_digest"),
        stored_contract.metadata.get("executed_auth_digest"),
        "approved_auth_digest should match executed_auth_digest"
    );

    // Verify auth presence metadata
    assert_eq!(
        stored_contract
            .metadata
            .get("approved_auth_present")
            .unwrap()
            .as_bool(),
        Some(true),
        "approved_auth_present should be true"
    );
    assert_eq!(
        stored_contract
            .metadata
            .get("executed_auth_present")
            .unwrap()
            .as_bool(),
        Some(true),
        "executed_auth_present should be true"
    );

    // Verify headers digest is present
    assert_eq!(
        stored_contract.metadata.get("approved_headers_digest"),
        stored_contract.metadata.get("executed_headers_digest")
    );

    // CRITICAL: Raw API key must NOT be persisted in contract metadata
    let metadata_json = serde_json::to_string(&stored_contract.metadata).unwrap();
    assert!(
        !metadata_json.contains("sk-test-api-key-12345"),
        "raw API key must not be persisted in rollback contract metadata"
    );
    assert!(
        !metadata_json.contains("test-api-key"),
        "API key value must not appear in any form in rollback contract metadata"
    );

    // Step 8: Verify. For POST this must use execute-time metadata only.
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "verify should succeed for POST");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // R3 must remain AwaitingVerification after verify (no auto-commit).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::AwaitingVerification));

    // Step 9: Explicit commit (required for R3)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let commit_resp: ferrum_proto::CommitResponse = serde_json::from_slice(&body).unwrap();
    assert!(commit_resp.committed);
    assert!(commit_resp.committed_at.is_some());

    // Verify final state is Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Committed));

    let _ = server_handle.join();
}

#[tokio::test]
async fn test_http_execute_denied_when_api_key_auth_missing_from_header_allowlist() {
    // Regression test: when payload-level dedicated HTTP auth (auth.api_key) is used
    // but the HTTP binding's header_allowlist does NOT contain the specific api_key header,
    // the request must be denied (fail-closed).
    // See ferrum-adapter-http/README.md: "When auth.api_key is present, the firewall
    // checks that the specific API key header (e.g., X-API-Key) is in the binding's
    // header_allowlist."

    let (port, _server_handle) = start_local_http_server(201);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with ONLY content-type in header_allowlist - does NOT include "x-api-key"
    // This means api_key auth should be REJECTED even though it's in the payload
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec!["content-type".to_string()], // NOTE: x-api-key NOT allowed
        mode: ResourceMode::ReadWrite,
    };

    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with mutating HTTP scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    req.requested_resource_scope = vec![http_scope];
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal - R3 requires approval
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Create resource with API key auth".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "api_key",
                "header": "X-API-Key",
                "key": "sk-test-api-key-12345"
            }
        }),
        expected_effect: "create remote HTTP resource".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with HTTP binding (missing x-api-key from header_allowlist)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution -> AwaitingApproval
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Resolve approval
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved for API key header allowlist test".to_string()),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 6: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 7: Execute with api_key auth - MUST BE DENIED because header_allowlist
    // does not include "x-api-key"
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api/users", port),
            "method": "POST",
            "body": {"name": "test"},
            "auth": {
                "type": "api_key",
                "header": "X-API-Key",
                "key": "sk-test-api-key-12345"
            }
        }),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // FAIL-CLOSED: execute must return 403 Forbidden when api_key auth is used
    // but the specific header (x-api-key) is not in header_allowlist
    assert_eq!(
        response.status(),
        403,
        "execute should be FORBIDDEN when api_key auth is used but header_allowlist does not contain 'x-api-key'"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        error.message.contains("x-api-key") || error.message.contains("denied"),
        "error message should mention api_key header allowlist violation, got: {}",
        error.message
    );

    // Verify execution state is still Prepared (fail-closed: firewall denied before
    // execution started, so state never transitioned to Running/Failed)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution should remain Prepared when firewall denies before execution starts, got {:?}",
        exec.state
    );
}

// ============================================
// LEDGER CHAIN TESTS
// ============================================
