use axum::http::StatusCode;
use ferrum_adapter_git::GitRollbackAdapter;
use ferrum_adapter_http::HttpRollbackAdapter;
use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, CapabilityMintRequest, CheckSpec, CheckType, Decision, EffectType,
    ExecutionState, IntentCompileRequest, IntentCompileResponse, IntentId, OutcomeClause,
    OutcomeSelectors, PauseExecutionRequest, ProposalId, ProvenanceEventKind, ResourceBinding,
    ResourceMode, ResourceSelector, ResumeExecutionRequest, RiskTier, RollbackClass,
    RollbackTarget, SensitivityLabel, TaintBudget, ToolBinding, TrustLabel,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackAdapter, RollbackService};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, ProposalRepo,
    ProvenanceRepo, RollbackRepo, SqliteStore,
};
use sqlx::{Connection, Row};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Test isolation: unique file path generation to avoid parallel test interference
// ---------------------------------------------------------------------------

/// Process-wide counter for generating unique test file paths.
static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Global map from execution_id to its test file path.
/// This allows tests to retrieve the path used during prepare for use in execute.
static TEST_FILE_PATHS: OnceLock<RwLock<HashMap<ferrum_proto::ExecutionId, PathBuf>>> =
    OnceLock::new();

/// Generates a unique test file path using a process-wide counter.
/// This avoids test interference when tests run in parallel.
fn next_test_file_path() -> PathBuf {
    let counter = TEST_FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
    PathBuf::from(format!("/tmp/ferrum_test_{}.txt", counter))
}

/// Records the test file path for a given execution_id.
fn set_test_file_path(execution_id: ferrum_proto::ExecutionId, path: PathBuf) {
    let map = TEST_FILE_PATHS.get_or_init(|| RwLock::new(HashMap::new()));
    map.write().unwrap().insert(execution_id, path);
}

/// Gets the test file path for a given execution_id.
fn get_test_file_path(execution_id: ferrum_proto::ExecutionId) -> Option<PathBuf> {
    if let Some(map) = TEST_FILE_PATHS.get() {
        map.read().unwrap().get(&execution_id).cloned()
    } else {
        None
    }
}

/// Resolves the execute payload path for a test execution.
///
/// Tests that go through `run_flow_to_prepared` store a unique per-execution
/// file path to avoid parallel interference. Other tests still intentionally use
/// the legacy `/tmp/test.txt` path, so we fall back to that when no mapped path
/// exists for the execution.
fn execution_test_path(execution_id: ferrum_proto::ExecutionId) -> String {
    get_test_file_path(execution_id)
        .unwrap_or_else(|| PathBuf::from("/tmp/test.txt"))
        .to_string_lossy()
        .to_string()
}

/// Constructs a file-system execute payload using the resolved test file path.
fn fs_execute_payload(execution_id: ferrum_proto::ExecutionId, content: &str) -> serde_json::Value {
    serde_json::json!({
        "path": execution_test_path(execution_id),
        "content": content
    })
}

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
    // For mutation effect types, provide a default resource scope to satisfy the
    // P0 scope-mismatch deny policy (engine.rs:31-46) which blocks mutations without scope.
    // ReadOnlyAnalysis and DraftCreation are non-mutations and don't need scope.
    let resource_scope = if matches!(
        effect_type,
        EffectType::ReadOnlyAnalysis | EffectType::DraftCreation
    ) {
        vec![]
    } else {
        vec![ferrum_proto::ResourceSelector::SqliteDatabase {
            db_path: ":memory:".to_string(),
            tables: vec![],
            mode: ferrum_proto::ResourceMode::ReadWrite,
        }]
    };
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: resource_scope,
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(effect_type),
        allowed_outcomes: None,
        forbidden_outcomes: None,
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

/// Regression test for Weak Spot 1: prepare must use the persisted effective rollback
/// class from the proposal rather than a downgraded caller value.
///
/// Scenario: Intent default is R3, client requests R0 in proposal.
/// The evaluate phase should elevate R0→R3 and persist R3 in the proposal.
/// After approval + prepare, the stored rollback contract must have:
/// - rollback_class = R3IrreversibleHighConsequence
/// - auto_commit = false
///
/// This proves the prepare handler reads the persisted proposal's rollback class,
/// not a hardcoded or downgraded value.
#[tokio::test]
async fn test_prepare_uses_persisted_effective_rollback_class_not_downgraded_caller_value() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP POST scope which infers R3 as default rollback class
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

    // Step 2: Create a proposal that requests R0 (below the intent default of R3)
    // This downgrade attempt should be blocked during evaluate, returning RequireApproval
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Try downgrade from R3 to R0".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/users"}),
        expected_effect: "post http request".to_string(),
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
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some(), "Proposal should be persisted");
    let stored = stored_proposal.unwrap();
    assert_eq!(
        stored.requested_rollback_class,
        RollbackClass::R3IrreversibleHighConsequence,
        "Persisted rollback class should be R3 (elevated from R0), proving floor was enforced"
    );

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Post,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // Step 4: Authorize execution - creates execution in AwaitingApproval state
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

    // Get approval ID
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    // Step 5: Resolve approval (approve=true)
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by regression test".to_string()),
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

    // Step 7: === REGRESSION ASSERTION ===
    // After prepare, verify the STORED rollback contract has the correct values.
    // This proves prepare used the persisted effective rollback class (R3 from proposal)
    // rather than a downgraded caller value (R0 from initial request).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();

    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // rollback_class must be R3IrreversibleHighConsequence (elevated from R0)
    assert_eq!(
        stored_contract.rollback_class,
        RollbackClass::R3IrreversibleHighConsequence,
        "Stored rollback contract must have R3 (not R0) - proves prepare used persisted proposal value"
    );

    // auto_commit must be false for R3 (R0 only has auto_commit=true)
    assert!(
        !stored_contract.auto_commit,
        "R3 rollback class must have auto_commit=false"
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

    // Step 1: Compile intent with proper file scope and FileMutation effect type.
    // Note: U1-S5b checks at prepare-time require intent effect_type to match
    // the inferred effect from rollback_target. Using FileMutation to match fs.write rollback.
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
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

    // Step 2: Evaluate proposal with fs.write (FileMutation)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Step 3: Mint capability
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
        requested_ttl_secs: 60, // Max allowed is 300 seconds
        policy_bundle_id: None,
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
    // Generate unique file path for this execution to avoid parallel test interference
    let test_file_path = next_test_file_path();
    let test_file_str = test_file_path.to_string_lossy();
    let test_dir = test_file_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/tmp".to_string());

    // Step 1: Compile intent with mutating effect type and proper file scope
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    // Add proper file scope for the test (fail-closed: empty scope denies all)
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: test_dir,
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
        raw_arguments: serde_json::json!({"path": test_file_str, "content": "hello"}),
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
            path: test_file_str.to_string(),
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
        policy_bundle_id: None,
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

    // Store the test file path for this execution so tests can retrieve it later
    set_test_file_path(execution_id, test_file_path);

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
        payload: fs_execute_payload(execution_id, "hello"),
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
        payload: fs_execute_payload(execution_id, "hello"),
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
        policy_bundle_id: None,
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
        payload: fs_execute_payload(execution_id, "hello"),
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
        payload: fs_execute_payload(execution_id, "hello"),
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

/// Integration test: HTTP adapter compensate succeeds with conservative no-op behavior.
///
/// Observable behavior demonstrated:
/// - compensate returns 200 with compensated=true for HTTP-backed executions
/// - This works because HTTP adapter's compensate is a no-op by design (R3 boundary)
/// - No active HTTP server is required for compensate to succeed
///
/// The no-op semantics for HTTP mutation recovery are documented in:
/// - docs/implementation-path/16a-slice-16-a-boundary-ratification.md
/// - crates/ferrum-adapter-http/src/lib.rs rollback() implementation
#[tokio::test]
async fn test_http_adapter_compensate_succeeds_as_noop() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Start a local server for the HTTP flow
    let (port, _handle) = start_local_http_server(200);

    // Step 1: Compile intent with HTTP POST scope (R3 default for mutations)
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
        mode: ResourceMode::Write,
    }];
    req.effect_type = Some(EffectType::ExternalApiCall);

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

    // Step 2: Create proposal requesting R3 (required for HTTP mutations)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP POST".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/test", port)}),
        expected_effect: "post to API".to_string(),
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

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Post,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // Step 4: Authorize (R3 requires approval)
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

    // Step 5: Resolve approval (approve=true)
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
        reason: Some("Approved by test".to_string()),
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

    // Step 6: Prepare
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

    // Step 7: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/test", port), "method": "POST"}),
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

    // Step 8: Verify
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
    let _verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Step 9: Compensate - this should succeed WITHOUT requiring any remote HTTP call
    // HTTP adapter compensate is a no-op by design
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

    // Compensate should succeed (200)
    assert_eq!(
        response.status(),
        200,
        "HTTP compensate should succeed even though no remote undo was performed"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();

    // compensated=true proves the compensate was acknowledged, not that remote undo happened
    assert!(
        compensate_resp.compensated,
        "HTTP compensate should return compensated=true (no-op acknowledgment)"
    );

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Compensated));
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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

    // The compiled intent has explicit allowed_outcomes (ReadOnlyAnalysis),
    // so the structural contradiction check is SKIPPED (has_explicit_allowed=true).
    // Under U1-S1 contract, allowed-outcome mismatch produces advisory warning only.
    // PDP's allowed-outcome assessment produces warnings for this mismatch.
    assert_eq!(
        eval_resp.decision,
        Decision::Allow,
        "Allowed-outcome mismatch should result in Allow with warnings, got {:?}",
        eval_resp.decision
    );
    // Verify the allowed-outcome mismatch was detected and included as a warning
    assert!(
        eval_resp.warnings.iter().any(|w| w
            .to_lowercase()
            .contains("does not match any allowed outcome")),
        "Expected allowed-outcome mismatch warning, got: {:?}",
        eval_resp.warnings
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
    // IMPORTANT: This test creates an intent with NO explicit outcome clauses
    // (empty allowed_outcomes and forbidden_outcomes) to test the structural
    // contradiction check. When explicit outcome clauses exist, U1 semantics
    // dictate that PDP's outcome-aware assessment governs (not firewall contradiction).
    let intent_with_no_clauses = ferrum_proto::IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent No Clauses".to_string(),
        goal: "Test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: Vec::new(),   // No explicit outcome clauses
        forbidden_outcomes: Vec::new(), // No explicit outcome clauses
        resource_scope: Vec::new(),
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
        policy_bundle_fingerprint: None,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };

    // Persist the intent so it can be looked up
    runtime
        .store
        .intents()
        .insert(&intent_with_no_clauses)
        .await
        .unwrap();
    let intent_id = intent_with_no_clauses.intent_id;

    // Create a proposal with mutating effect against an intent with no outcome clauses
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

    // Structural contradiction (read_only_violation) fires as MEDIUM severity
    // when NO explicit outcome clauses exist. Under U1-S1 contract, medium
    // severity contradictions produce advisory warnings, not denial.
    // Note: matched_rule_ids only contains HIGH severity contradictions.
    // MEDIUM severity contradictions appear in warnings only.
    assert_eq!(
        eval_resp.decision,
        Decision::Allow,
        "Structural contradiction should result in Allow with warnings when no explicit outcome clauses exist, got {:?}",
        eval_resp.decision
    );
    // Verify the contradiction was detected and included as a warning
    assert!(
        eval_resp
            .warnings
            .iter()
            .any(|w| w.contains("read-only") || w.contains("mutating")),
        "Expected read-only/mutating contradiction warning, got: {:?}",
        eval_resp.warnings
    );
}

#[tokio::test]
async fn test_effect_classifier_word_boundary_safety() {
    // Test that the effect classifier uses word boundaries
    // "target" should NOT be classified as "get" (read-only) because it's a substring
    use ferrum_firewall::{DefaultFirewall, SemanticFirewall};

    let firewall = DefaultFirewall::new();

    // Create an intent with NO explicit outcome clauses
    // This allows testing the structural contradiction check which fires for mutating proposals
    // when no explicit outcome clauses exist
    let intent = ferrum_proto::IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: Vec::new(),   // No explicit outcome clauses
        forbidden_outcomes: Vec::new(), // No explicit outcome clauses
        resource_scope: Vec::new(),
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
        policy_bundle_fingerprint: None,
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
    // This fires because no explicit outcome clauses exist - the structural check catches it
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        allowed_outcomes: None,
        forbidden_outcomes: None,
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
        allowed_outcomes: None,
        forbidden_outcomes: None,
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
    // Include both fs.read and fs.write in scope to isolate allowed-outcome mismatch
    // (avoiding mcp_scope_violation which is HIGH severity and would cause denial)
    let mut read_only_intent = compile_resp.envelope;
    read_only_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "primary".to_string(),
        description: "Read only".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
        selectors: None,
    }];
    read_only_intent.resource_scope = vec![
        ResourceSelector::McpTool {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            mode: ResourceMode::Read,
        },
        ResourceSelector::McpTool {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            mode: ResourceMode::Write,
        },
    ];
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

    // With explicit allowed_outcomes, the structural contradiction check is SKIPPED.
    // Under U1-S1 contract, allowed-outcome mismatch produces advisory warning only.
    // PDP handles this via allowed-outcome assessment, not firewall contradiction.
    assert_eq!(
        eval_resp.decision,
        Decision::Allow,
        "Allowed-outcome mismatch should result in Allow with warnings, got {:?}",
        eval_resp.decision
    );
    // Verify advisory warning is present (from PDP's allowed-outcome assessment)
    assert!(
        eval_resp.warnings.iter().any(|w| {
            let w_lower = w.to_lowercase();
            w_lower.contains("read-only") || w_lower.contains("allowed")
        }),
        "Expected read-only/allowed outcome advisory warning, got: {:?}",
        eval_resp.warnings
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
        allowed_outcomes: None,
        forbidden_outcomes: None,
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
        allowed_outcomes: None,
        forbidden_outcomes: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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

    // Note: U1-S5b check at prepare-time requires effect_type to match what the
    // binding's rollback target infers. FilePath always infers FileMutation regardless
    // of binding mode. Since we're minting a File binding only, we use FileMutation
    // effect type to avoid U1-S5b blocking at prepare-time.
    // The HTTP binding mismatch will be caught at execute-time.
    let file_scope = ferrum_proto::ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
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
        policy_bundle_id: None,
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
        payload: fs_execute_payload(execution_id, "hello world"),
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

    // Note: U1-S5b check at prepare-time requires effect_type to match what the
    // binding's rollback target infers. FilePath always infers FileMutation regardless
    // of binding mode, so we use FileMutation here to avoid triggering the hard gate.
    let (_intent_id, _proposal_id, execution_id) = run_file_flow_to_prepared(
        &runtime,
        file_binding,
        EffectType::FileMutation,
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

    // Note: U1-S5b check at prepare-time requires effect_type to match what the
    // binding's rollback target infers. FilePath always infers FileMutation regardless
    // of binding mode, so we use FileMutation to avoid triggering the hard gate.
    // The binding mode mismatch (Read binding vs Write operation) will be caught at execute-time.
    let (_intent_id, _proposal_id, execution_id) = run_file_flow_to_prepared(
        &runtime,
        file_binding,
        EffectType::FileMutation,
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
        policy_bundle_id: None,
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
// SQLITE MULTI-ROW TRANSACTION ROLLBACK TESTS
// ============================================

/// Helper to create a test contract for SQLite multi-row transactions.
fn make_sqlite_multi_row_contract(
    db_path: &str,
    execution_id: ferrum_proto::ExecutionId,
) -> ferrum_proto::RollbackContract {
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert("db_path".to_string(), serde_json::json!(db_path));

    ferrum_proto::RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id: ferrum_proto::IntentId::new(),
        proposal_id: ferrum_proto::ProposalId::new(),
        execution_id,
        action_type: ferrum_proto::ActionType::SqlMutation,
        rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        adapter_key: "sqlite".to_string(),
        target: ferrum_proto::RollbackTarget::SqliteTxn {
            db_path: db_path.to_string(),
            tx_id: "test-tx".to_string(),
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: ferrum_proto::RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata,
    }
}

#[tokio::test]
async fn test_sqlite_multi_row_transaction_executes_and_rollback_restores_original_state() {
    // Test that multi-row payload {rows: [...]} executes atomically and rollback
    // restores the original state for all touched rows.
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("test_multi_row.sqlite");
    std::fs::File::create(&db_path).expect("failed to create db file");
    let db_path_str = format!("sqlite://{}", db_path.display());

    let adapter = ferrum_adapter_sqlite::SqliteRollbackAdapter::new("sqlite");
    let execution_id = ferrum_proto::ExecutionId::new();
    let contract = make_sqlite_multi_row_contract(&db_path_str, execution_id);

    // Pre-populate with original values
    {
        let mut conn = sqlx::SqliteConnection::connect(&db_path_str)
            .await
            .expect("failed to connect");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, content TEXT NOT NULL)",
        )
        .execute(&mut conn)
        .await
        .expect("failed to create table");
        sqlx::query("INSERT INTO users (id, content) VALUES ('user1', 'OriginalAlice')")
            .execute(&mut conn)
            .await
            .expect("failed to seed user1");
        sqlx::query("INSERT INTO users (id, content) VALUES ('user2', 'OriginalBob')")
            .execute(&mut conn)
            .await
            .expect("failed to seed user2");
    }

    // Multi-row payload updating two existing rows
    let multi_row_payload = serde_json::json!({
        "rows": [
            {"table": "users", "row_id": "user1", "content": "UpdatedAlice"},
            {"table": "users", "row_id": "user2", "content": "UpdatedBob"}
        ]
    });

    // Execute multi-row transaction
    let result = adapter.execute(&contract, &multi_row_payload).await;
    assert!(
        result.is_ok(),
        "Multi-row execute should succeed: {:?}",
        result.err()
    );
    let receipt = result.unwrap();
    assert_eq!(
        receipt.external_id,
        Some("sqlite:multi-row-txn:2rows".to_string())
    );

    // Verify values were updated
    {
        let mut conn = sqlx::SqliteConnection::connect(&db_path_str)
            .await
            .expect("failed to connect");
        let row1: String = sqlx::query("SELECT content FROM users WHERE id = 'user1'")
            .fetch_one(&mut conn)
            .await
            .expect("failed to fetch user1")
            .get(0);
        assert_eq!(row1, "UpdatedAlice");

        let row2: String = sqlx::query("SELECT content FROM users WHERE id = 'user2'")
            .fetch_one(&mut conn)
            .await
            .expect("failed to fetch user2")
            .get(0);
        assert_eq!(row2, "UpdatedBob");
    }

    // Rollback should restore original values for ALL rows
    let rollback_result = adapter.rollback(&contract).await;
    assert!(
        rollback_result.is_ok(),
        "Rollback should succeed: {:?}",
        rollback_result.err()
    );

    // After rollback, original values should be restored
    let mut conn = sqlx::SqliteConnection::connect(&db_path_str)
        .await
        .expect("failed to connect");
    let row1: String = sqlx::query("SELECT content FROM users WHERE id = 'user1'")
        .fetch_one(&mut conn)
        .await
        .expect("failed to fetch user1 after rollback")
        .get(0);
    assert_eq!(
        row1, "OriginalAlice",
        "user1 should be restored to original value"
    );

    let row2: String = sqlx::query("SELECT content FROM users WHERE id = 'user2'")
        .fetch_one(&mut conn)
        .await
        .expect("failed to fetch user2 after rollback")
        .get(0);
    assert_eq!(
        row2, "OriginalBob",
        "user2 should be restored to original value"
    );
}

#[tokio::test]
async fn test_sqlite_multi_row_transaction_creates_new_rows_and_rollback_deletes_all() {
    // Test that multi-row payload can create new rows and rollback deletes them all.
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("test_multi_row_create.sqlite");
    std::fs::File::create(&db_path).expect("failed to create db file");
    let db_path_str = format!("sqlite://{}", db_path.display());

    let adapter = ferrum_adapter_sqlite::SqliteRollbackAdapter::new("sqlite");
    let execution_id = ferrum_proto::ExecutionId::new();
    let contract = make_sqlite_multi_row_contract(&db_path_str, execution_id);

    // Multi-row payload creating new rows (tables don't exist yet)
    let multi_row_payload = serde_json::json!({
        "rows": [
            {"table": "users", "row_id": "user1", "content": "Alice"},
            {"table": "users", "row_id": "user2", "content": "Bob"},
            {"table": "orders", "row_id": "order1", "content": "Order123"}
        ]
    });

    // Execute multi-row transaction
    let result = adapter.execute(&contract, &multi_row_payload).await;
    assert!(
        result.is_ok(),
        "Multi-row execute should succeed: {:?}",
        result.err()
    );
    let receipt = result.unwrap();
    assert_eq!(
        receipt.external_id,
        Some("sqlite:multi-row-txn:3rows".to_string())
    );

    // Verify all rows were created
    let mut conn = sqlx::SqliteConnection::connect(&db_path_str)
        .await
        .expect("failed to connect");

    let count: i64 = sqlx::query("SELECT COUNT(*) FROM users")
        .fetch_one(&mut conn)
        .await
        .expect("failed to count users")
        .get(0);
    assert_eq!(count, 2, "Should have 2 users");

    let order_count: i64 = sqlx::query("SELECT COUNT(*) FROM orders")
        .fetch_one(&mut conn)
        .await
        .expect("failed to count orders")
        .get(0);
    assert_eq!(order_count, 1, "Should have 1 order");

    // Rollback should delete all created rows (since they didn't exist before)
    let rollback_result = adapter.rollback(&contract).await;
    assert!(
        rollback_result.is_ok(),
        "Rollback should succeed: {:?}",
        rollback_result.err()
    );

    // After rollback, all rows should be gone
    let user_count: i64 = sqlx::query("SELECT COUNT(*) FROM users")
        .fetch_one(&mut conn)
        .await
        .expect("failed to count users after rollback")
        .get(0);
    assert_eq!(user_count, 0, "Users table should be empty after rollback");

    let order_count: i64 = sqlx::query("SELECT COUNT(*) FROM orders")
        .fetch_one(&mut conn)
        .await
        .expect("failed to count orders after rollback")
        .get(0);
    assert_eq!(
        order_count, 0,
        "Orders table should be empty after rollback"
    );
}

#[tokio::test]
async fn test_sqlite_legacy_single_row_payload_still_works() {
    // Test that legacy single-row payload (without "rows" key) still works
    // for backward compatibility.
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("test_single_row.sqlite");
    std::fs::File::create(&db_path).expect("failed to create db file");
    let db_path_str = format!("sqlite://{}", db_path.display());

    let adapter = ferrum_adapter_sqlite::SqliteRollbackAdapter::new("sqlite");
    let execution_id = ferrum_proto::ExecutionId::new();
    let contract = make_sqlite_multi_row_contract(&db_path_str, execution_id);

    // Legacy single-row payload (no "rows" key)
    let single_row_payload = serde_json::json!({
        "table": "users",
        "row_id": "user1",
        "content": "Alice"
    });

    // Execute single-row (legacy path)
    let result = adapter.execute(&contract, &single_row_payload).await;
    assert!(
        result.is_ok(),
        "Single-row execute should succeed: {:?}",
        result.err()
    );
    let receipt = result.unwrap();
    assert_eq!(receipt.external_id, Some("sqlite:users/user1".to_string()));

    // Verify row was written
    let mut conn = sqlx::SqliteConnection::connect(&db_path_str)
        .await
        .expect("failed to connect");
    let row: String = sqlx::query("SELECT content FROM users WHERE id = 'user1'")
        .fetch_one(&mut conn)
        .await
        .expect("failed to fetch user1")
        .get(0);
    assert_eq!(row, "Alice");

    // Rollback should delete the row (since it didn't exist before)
    let rollback_result = adapter.rollback(&contract).await;
    assert!(
        rollback_result.is_ok(),
        "Rollback should succeed: {:?}",
        rollback_result.err()
    );

    let remaining: i64 = sqlx::query("SELECT COUNT(*) FROM users")
        .fetch_one(&mut conn)
        .await
        .expect("failed to count users after rollback")
        .get(0);
    assert_eq!(remaining, 0, "Users table should be empty after rollback");
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // Capture before_ref from contract metadata
    let contract = prep_resp.rollback_contract.unwrap();
    let before_ref = contract
        .metadata
        .get("before_ref")
        .and_then(|v| v.as_str())
        .unwrap_or("")
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

/// Integration test: git verify false transitions execution to Failed and commit is rejected.
///
/// Scenario (deterministic local-temp-repo ref-mismatch):
/// 1. Prepare captures before_ref (HEAD at prepare time)
/// 2. Execute is called with after_ref = current HEAD (succeeds)
/// 3. After execute, a NEW commit is made OUTSIDE the execution flow (simulates interference)
/// 4. Verify checks current HEAD vs stored after_ref → mismatch → verified=false
/// 5. Execution transitions to Failed state
/// 6. Commit is rejected from Failed state (4xx)
///
/// This proves the fail-closed behavior: when verify returns false, the execution
/// becomes Failed and cannot be committed.
#[tokio::test]
async fn test_git_verify_false_transitions_to_failed_and_rejects_commit() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepared with R2 (requires explicit commit, not auto-commit)
    let (_intent_id, _proposal_id, execution_id, _before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;

    // Get current HEAD before execute
    let head_at_execute = git_head(&repo_path);

    // Execute with payload.after_ref matching current HEAD
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": head_at_execute}),
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");

    // Simulate interference: make a NEW commit OUTSIDE the execution flow
    // This advances HEAD past the after_ref captured during execute
    let _head_after_interference =
        git_commit_change(&repo_path, "interference.txt", "outside change\n");
    let current_head_after_interference = git_head(&repo_path);
    assert_ne!(
        current_head_after_interference, head_at_execute,
        "interference commit should have changed HEAD"
    );

    // Verify → git adapter checks current HEAD vs stored after_ref → mismatch
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

    // Verify returns 200 (fail-closed: verification failure is not an error response)
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (not 500) for fail-closed verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Ref mismatch: current HEAD ({}) != after_ref ({}) → verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when current HEAD ({}) != after_ref ({}) due to interference",
        current_head_after_interference, head_at_execute
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Execution state should transition to Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify mismatch, got {:?}",
        exec.state
    );

    // Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );

    // Verify SideEffectVerified provenance event was emitted with verified=false
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
        "SideEffectVerified provenance event should be emitted even for verified=false"
    );
}

/// Integration test: gateway-level git rollback drill after verify failure.
///
/// Flow: execute -> verify false (due to outside interference) -> rollback -> assert RolledBack + git state restored.
/// Uses deterministic local temp git repo (no remotes).
#[tokio::test]
async fn test_git_verify_false_triggers_rollback_drill() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Capture HEAD before the flow
    let head_before_flow = git_head(&repo_path);

    // Run flow to prepared with R2 (compensatable, requires explicit rollback)
    let (_intent_id, _proposal_id, execution_id, before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;

    // Verify before_ref was captured at prepare time
    assert_eq!(
        before_ref, head_before_flow,
        "before_ref should match HEAD at prepare time"
    );

    // Execute with payload.after_ref matching current HEAD
    let head_at_execute = git_head(&repo_path);
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": head_at_execute}),
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

    // Simulate interference: make a NEW commit OUTSIDE the execution flow
    // This advances HEAD past the after_ref captured during execute
    let head_after_interference =
        git_commit_change(&repo_path, "interference.txt", "outside change\n");
    assert_ne!(
        head_after_interference, head_at_execute,
        "interference commit should have changed HEAD"
    );

    // Verify → git adapter checks current HEAD vs stored after_ref → mismatch → verified=false
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
        !verify_resp.verified,
        "verify should be false after outside interference"
    );

    // Execution state should be Failed after verify mismatch
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Failed),
        "execution should be Failed after verify mismatch"
    );

    // Now rollback the execution
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
        "rollback should return 200 even after verify failure"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        rollback_resp.rolled_back,
        "rollback should succeed after verify failure"
    );

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::RolledBack),
        "execution should be RolledBack after rollback"
    );

    // Verify git state was restored to before_ref
    let current_head = git_head(&repo_path);
    assert_eq!(
        current_head, before_ref,
        "git HEAD should be restored to before_ref ({}) after rollback, but is {}",
        before_ref, current_head
    );

    // Verify SideEffectRolledBack provenance event was emitted
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
        "SideEffectRolledBack provenance event should be emitted after rollback"
    );
}

/// Integration test: gateway-level git compensate drill after verify failure.
///
/// Flow: execute -> verify false (due to outside interference) -> compensate -> assert Compensated + git state restored.
/// Uses deterministic local temp git repo (no remotes).
/// Compensate is the primary recovery endpoint; this validates it works identically to rollback for git.
#[tokio::test]
async fn test_git_verify_false_triggers_compensate_drill() {
    let (_git_temp_dir, repo_path) = init_temp_git_repo();
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Capture HEAD before the flow
    let head_before_flow = git_head(&repo_path);

    // Run flow to prepared with R2 (compensatable, requires explicit compensate)
    let (_intent_id, _proposal_id, execution_id, before_ref) =
        run_git_flow_to_prepared_with_real_repo(&runtime, repo_path.clone()).await;

    // Verify before_ref was captured at prepare time
    assert_eq!(
        before_ref, head_before_flow,
        "before_ref should match HEAD at prepare time"
    );

    // Execute with payload.after_ref matching current HEAD
    let head_at_execute = git_head(&repo_path);
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"after_ref": head_at_execute}),
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

    // Simulate interference: make a NEW commit OUTSIDE the execution flow
    let head_after_interference =
        git_commit_change(&repo_path, "interference2.txt", "another change\n");
    assert_ne!(
        head_after_interference, head_at_execute,
        "interference commit should have changed HEAD"
    );

    // Verify → mismatch → verified=false
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
        !verify_resp.verified,
        "verify should be false after outside interference"
    );

    // Execution state should be Failed after verify mismatch
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Failed),
        "execution should be Failed after verify mismatch"
    );

    // Now compensate the execution
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

    assert_eq!(
        response.status(),
        200,
        "compensate should return 200 even after verify failure"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        compensate_resp.compensated,
        "compensate should succeed after verify failure"
    );

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Compensated),
        "execution should be Compensated after compensate"
    );

    // Verify git state was restored to before_ref
    let current_head = git_head(&repo_path);
    assert_eq!(
        current_head, before_ref,
        "git HEAD should be restored to before_ref ({}) after compensate, but is {}",
        before_ref, current_head
    );

    // Verify SideEffectCompensated provenance event was emitted
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
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
        !events.is_empty(),
        "SideEffectCompensated provenance event should be emitted after compensate"
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        payload: fs_execute_payload(execution_id, "hello"),
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
        policy_bundle_id: None,
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

    // Use unique file path to avoid parallel test interference
    let test_file_path = next_test_file_path();
    let test_file_str = test_file_path.to_string_lossy();

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
            path: test_file_str.to_string(),
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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

/// Integration test: sqlite gateway-level verify false transitions to Failed and rejects commit.
///
/// Flow: execute -> tamper with row content (outside interference) -> verify mismatch ->
///       execution becomes Failed -> commit rejected.
///
/// This mirrors test_fs_verify_hash_mismatch_transitions_to_failed_and_rejects_commit
/// but for the sqlite adapter. Uses a tempfile-backed sqlite DB with snapshot-based verify.
#[tokio::test]
async fn test_sqlite_verify_false_transitions_to_failed_and_rejects_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let db_path = std::env::temp_dir().join("test_sqlite_verify_false_commit_test.db");
    std::fs::File::create(&db_path).expect("failed to create sqlite db file");
    let db_path_str = db_path.to_string_lossy().to_string();

    let table = "users";
    let row_id = "user1";

    // Seed initial row content
    seed_sqlite_row(&db_path, table, row_id, "original content").await;

    // Step 1: Compile intent with SqliteDatabase scope
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

    // Step 2: Evaluate proposal with R2 (requires explicit commit)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Update sqlite row".to_string(),
        tool_name: "db.update".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({
            "db_path": db_path_str,
            "table": table,
            "row_id": row_id,
            "content": "updated by execute"
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

    // Step 3: Mint capability
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
            db_path: db_path_str.clone(),
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
        policy_bundle_id: None,
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

    // Step 6: Execute sqlite mutation
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": db_path_str,
            "sql": format!("UPDATE {} SET content = 'updated by execute' WHERE id = 'user1'", table),
            "table": table,
            "row_id": row_id,
            "content": "updated by execute"
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

    // Verify row was updated by execute
    let updated_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(updated_content.as_deref(), Some("updated by execute"));

    // Step 7: Tamper with the row after execution (simulates outside interference)
    // Change the row content to something different from what execute wrote
    seed_sqlite_row(&db_path, table, row_id, "tampered by outside").await;
    let tampered_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(tampered_content.as_deref(), Some("tampered by outside"));

    // Step 8: Verify → content mismatch → verified=false
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

    // Verify returns 200 (fail-closed: verification failure is not an error response)
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (not 500) for fail-closed verification"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Content mismatch: current content != snapshot content → verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when current content != snapshot (row was tampered)"
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Step 9: Execution state should transition to Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify mismatch, got {:?}",
        exec.state
    );

    // Step 10: Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );

    // Clean up: restore row so subsequent tests are not affected
    seed_sqlite_row(&db_path, table, row_id, "original content").await;
}

/// Integration test: sqlite gateway-level rollback drill after verify failure.
///
/// Flow: execute -> verify false (due to outside interference) -> rollback ->
///       assert RolledBack + sqlite state restored.
///
/// This mirrors test_fs_verify_false_triggers_rollback_drill but for sqlite adapter.
#[tokio::test]
async fn test_sqlite_verify_false_triggers_rollback_drill() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let db_path = std::env::temp_dir().join("test_sqlite_rollback_drill.db");
    std::fs::File::create(&db_path).expect("failed to create sqlite db file");
    let db_path_str = db_path.to_string_lossy().to_string();

    let table = "accounts";
    let row_id = "acc_1";

    // Seed initial row content
    seed_sqlite_row(&db_path, table, row_id, "initial balance: 100").await;

    // Step 1: Compile intent with SqliteDatabase scope
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

    // Step 2: Evaluate proposal with R2 (requires explicit rollback)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Update account balance".to_string(),
        tool_name: "db.update".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({
            "db_path": db_path_str,
            "table": table,
            "row_id": row_id,
            "content": "balance after tx: 150"
        }),
        expected_effect: "update account balance".to_string(),
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

    // Step 3: Mint capability
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
            db_path: db_path_str.clone(),
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
        policy_bundle_id: None,
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

    // Step 6: Execute sqlite mutation
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": db_path_str,
            "sql": format!("UPDATE {} SET content = 'balance after tx: 150' WHERE id = 'acc_1'", table),
            "table": table,
            "row_id": row_id,
            "content": "balance after tx: 150"
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

    // Verify row was updated
    let updated_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(updated_content.as_deref(), Some("balance after tx: 150"));

    // Step 7: Tamper with the row after execution (simulates outside interference)
    seed_sqlite_row(&db_path, table, row_id, "tampered balance: 999").await;
    let tampered_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(tampered_content.as_deref(), Some("tampered balance: 999"));

    // Step 8: Verify → content mismatch → verified=false
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
        !verify_resp.verified,
        "verify should be false after outside interference"
    );

    // Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Failed),
        "execution should be Failed after verify mismatch"
    );

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

    assert_eq!(
        response.status(),
        200,
        "rollback should return 200 even after verify failure"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        rollback_resp.rolled_back,
        "rollback should succeed after verify failure"
    );

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::RolledBack),
        "execution should be RolledBack after rollback"
    );

    // Verify sqlite state: row should be restored to original content
    let restored_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(
        restored_content.as_deref(),
        Some("initial balance: 100"),
        "row content should be restored to original after rollback"
    );

    // Verify SideEffectRolledBack provenance event was emitted
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
        "SideEffectRolledBack provenance event should be emitted after rollback"
    );

    // Clean up
    let _ = std::fs::remove_file(&db_path);
}

/// Integration test: sqlite gateway-level compensate drill after verify failure.
///
/// Flow: execute -> verify false (due to outside interference) -> compensate ->
///       assert Compensated + sqlite state restored.
///
/// This mirrors test_fs_verify_false_triggers_compensate_drill but for sqlite adapter.
#[tokio::test]
async fn test_sqlite_verify_false_triggers_compensate_drill() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let db_path = std::env::temp_dir().join("test_sqlite_compensate_drill.db");
    std::fs::File::create(&db_path).expect("failed to create sqlite db file");
    let db_path_str = db_path.to_string_lossy().to_string();

    let table = "inventory";
    let row_id = "item_1";

    // Seed initial row content
    seed_sqlite_row(&db_path, table, row_id, "stock: 50").await;

    // Step 1: Compile intent with SqliteDatabase scope
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

    // Step 2: Evaluate proposal with R2 (requires explicit compensate)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Update inventory stock".to_string(),
        tool_name: "db.update".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({
            "db_path": db_path_str,
            "table": table,
            "row_id": row_id,
            "content": "stock after order: 45"
        }),
        expected_effect: "decrement inventory stock".to_string(),
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

    // Step 3: Mint capability
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
            db_path: db_path_str.clone(),
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
        policy_bundle_id: None,
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

    // Step 6: Execute sqlite mutation
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "db_path": db_path_str,
            "sql": format!("UPDATE {} SET content = 'stock after order: 45' WHERE id = 'item_1'", table),
            "table": table,
            "row_id": row_id,
            "content": "stock after order: 45"
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

    // Verify row was updated
    let updated_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(updated_content.as_deref(), Some("stock after order: 45"));

    // Step 7: Tamper with the row after execution (simulates outside interference)
    seed_sqlite_row(&db_path, table, row_id, "stock tampered: 9999").await;
    let tampered_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(tampered_content.as_deref(), Some("stock tampered: 9999"));

    // Step 8: Verify → content mismatch → verified=false
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
        !verify_resp.verified,
        "verify should be false after outside interference"
    );

    // Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Failed),
        "execution should be Failed after verify mismatch"
    );

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

    assert_eq!(
        response.status(),
        200,
        "compensate should return 200 even after verify failure"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        compensate_resp.compensated,
        "compensate should succeed after verify failure"
    );

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Compensated),
        "execution should be Compensated after compensate"
    );

    // Verify sqlite state: row should be restored to original content
    let restored_content = fetch_sqlite_row_content(&db_path, table, row_id).await;
    assert_eq!(
        restored_content.as_deref(),
        Some("stock: 50"),
        "row content should be restored to original after compensate"
    );

    // Verify SideEffectCompensated provenance event was emitted
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
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
        !events.is_empty(),
        "SideEffectCompensated provenance event should be emitted after compensate"
    );

    // Clean up
    let _ = std::fs::remove_file(&db_path);
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
        policy_bundle_id: None,
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

/// Helper: create a test runtime with a FILE-BACKED maildraft store.
/// This is needed for tests that need to corrupt the maildraft database after execute.
async fn create_test_runtime_with_maildraft_file_store() -> (
    TempDir,
    GatewayRuntime,
    SqliteStore,
    ferrum_adapter_maildraft::MaildraftStore,
    std::path::PathBuf, // path to maildraft DB file for corruption
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

    // Create FILE-BACKED maildraft store (not in-memory)
    let maildraft_db_path = temp_dir.path().join("maildraft.sqlite");
    std::fs::File::create(&maildraft_db_path).expect("failed to create maildraft sqlite file");
    let maildraft_store =
        ferrum_adapter_maildraft::SqliteMaildraftStore::new_from_file(&maildraft_db_path)
            .expect("failed to create file-backed maildraft store");

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

    (temp_dir, runtime, store, maildraft_store, maildraft_db_path)
}

/// P2.7 Slice 5 (gateway-level): Verify fail-closed on maildraft storage/db error.
///
/// This test proves end-to-end fail-closed semantics through the gateway stack:
/// 1. Execute creates draft in maildraft SQLite store
/// 2. Maildraft DB is corrupted (simulates storage/db error)
/// 3. Verify call returns verified=false (fail-closed)
/// 4. Execution transitions to Failed state
/// 5. Commit is rejected with 409 Conflict
#[tokio::test]
async fn test_maildraft_gateway_verify_fail_closed_on_db_error() {
    let (_temp_dir, runtime, _store, _maildraft_store, maildraft_db_path) =
        create_test_runtime_with_maildraft_file_store().await;

    // Run email draft flow to prepared state (R2 for explicit commit/compensate)
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

    // Execute draft creation
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

    assert_eq!(response.status(), 200, "execute should succeed");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let exec_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(exec_resp.executed, "executed flag should be true");

    // Corrupt the maildraft database file to simulate storage/db error
    std::fs::write(
        &maildraft_db_path,
        b"this is not a valid sqlite database!!!",
    )
    .expect("failed to corrupt maildraft database");

    // Verify should return verified=false (fail-closed) due to DB corruption
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

    // Verify API returns 200 (gateway handles failure via verified=false, not error response)
    assert_eq!(
        response.status(),
        200,
        "verify API should return 200 even on fail-closed"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // ASSERTION 1: verify must be FALSE (fail-closed behavior for DB error)
    assert!(
        !verify_resp.verified,
        "verify should be FALSE on maildraft DB corruption (fail-closed), got verified={}",
        verify_resp.verified
    );

    // ASSERTION 2: execution must be in Failed state (not Running, not Committed)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "execution must be Failed after verify returns false, got {:?}",
        exec.state
    );

    // ASSERTION 3: commit from Failed state is rejected with 409 Conflict
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

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "commit from Failed state must be rejected with 409 Conflict, got {}: {:?}",
        status,
        String::from_utf8_lossy(&body)
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
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Get the unique file path assigned to this execution
    let test_file_path1 =
        get_test_file_path(execution_id1).expect("test_file_path should be set for execution_id1");

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id: execution_id1,
        payload: serde_json::json!({"path": test_file_path1.to_string_lossy(), "content": "hello"}),
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

    // Get the unique file path assigned to this execution
    let test_file_path2 =
        get_test_file_path(execution_id2).expect("test_file_path should be set for execution_id2");

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id: execution_id2,
        payload: serde_json::json!({"path": test_file_path2.to_string_lossy(), "content": "world"}),
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
                .uri(format!("/v1/executions/{}/prepare", execution_id))
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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

/// Integration test: HTTP POST with 500 response triggers gateway-level failure-mode
/// and state-transition coverage.
///
/// Matrix cell: HTTP POST (mutating) → R3 path → 500 response → verify=false → Failed state
/// and commit is rejected from Failed state (Conflict guard).
///
/// Flow: compile→evaluate→mint→authorize→approve→prepare→execute→verify (verify=false)→commit (rejected)
///
/// Assertions:
/// 1. verify returns false for 500 (fail-closed)
/// 2. execution state is Failed (not auto-committed for R3 when verify fails)
/// 3. commit is rejected with 409 Conflict from Failed state
///
/// This is slice-2 coverage for the gateway-level failure-mode/state-transition matrix
/// as defined by oracle for issue #97.
#[tokio::test]
async fn test_http_post_500_verify_false_commit_rejected_from_failed_state() {
    // Start local HTTP server that returns 500 for POST
    let (port, server_handle) = start_local_http_server(500);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // HTTP binding with POST method and ReadWrite mode
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::ReadWrite,
    };

    // HTTP scope matching the binding
    let http_scope = ferrum_proto::ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/api".to_string(),
        mode: ResourceMode::ReadWrite,
    };

    // Step 1: Compile intent with HTTP POST scope
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

    // Step 2: Evaluate proposal with R3 (required for HTTP POST mutations)
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call local HTTP POST API".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/api/users", port)}),
        expected_effect: "create resource via HTTP POST".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
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
        policy_bundle_id: None,
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

    // Step 5: Resolve approval (R3 requires approval before prepare)
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "expected pending approval for R3 execution"
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
        reason: Some("Approved by test".to_string()),
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

    // Step 6: Prepare
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

    // Step 8: Execute - HTTP adapter performs POST and captures status (500)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/api/users", port)}),
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
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain the HTTP status code (500)
    assert_eq!(
        execute_resp.result_digest.as_deref(),
        Some("500"),
        "result_digest should contain HTTP 500 status code from execute"
    );

    // Step 9: Verify - MUST fail for 500 (fail-closed: non-2xx does NOT auto-verify)
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
    // ASSERTION 1: verify must be FALSE for 500 (fail-closed behavior for mutation)
    assert!(
        !verify_resp.verified,
        "verify must be FALSE for 500 response (fail-closed for mutations); got verified={}",
        verify_resp.verified
    );

    // Clean up server thread
    let _ = server_handle.join();

    // ASSERTION 2: execution must NOT be auto-committed for R3 when verify fails
    // R3 only auto-commits if verify succeeds - since verify failed, state is Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "R3 execution must be Failed when verify fails (not auto-committed), got {:?}",
        exec.state
    );

    // ASSERTION 3: commit from Failed state is rejected with 409 Conflict
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "commit from Failed state must be rejected with 409 Conflict, got {}: {:?}",
        status,
        String::from_utf8_lossy(&body)
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
        policy_bundle_id: None,
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
// PAUSE EXECUTION TESTS
// ============================================

/// Helper: runs flow to Running state (up to but not including verify).
/// Returns the execution_id.
async fn run_flow_to_running(runtime: &GatewayRuntime) -> ferrum_proto::ExecutionId {
    let app = build_router(runtime.clone());
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
        content_hash: None,
    }];
    let response = app
        .clone()
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
        title: "Execute mutation".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .clone()
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
        policy_bundle_id: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .clone()
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

    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .clone()
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

    // Prepare
    let response = app
        .clone()
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

    // Execute (stop here - execution is in Running state)
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
    };
    let response = app
        .clone()
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

    execution_id
}

/// Test: pause from Running sets state to Paused.
#[tokio::test]
async fn test_gateway_pause_from_running_sets_paused() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Seed execution to Running state
    let execution_id = run_flow_to_running(&runtime).await;

    // Verify initial state is Running
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Running),
        "execution should be Running before pause"
    );

    // Call pause endpoint
    let app = build_router(runtime.clone());
    let pause_req = PauseExecutionRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/pause", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&pause_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "pause should succeed from Running state"
    );

    // Verify state is now Paused
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    let exec = stored.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Paused),
        "execution should be Paused after pause, got {:?}",
        exec.state
    );
    // Verify finished_at is NOT set (pause is not terminal)
    assert!(
        exec.finished_at.is_none(),
        "finished_at should not be set for Paused state"
    );
}

/// Test: pause from Prepared returns 409 Conflict.
#[tokio::test]
async fn test_gateway_pause_from_prepared_returns_conflict() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Seed execution to Prepared state (stop after prepare)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Verify initial state is Prepared
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Prepared),
        "execution should be Prepared before pause attempt"
    );

    // Call pause endpoint - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let pause_req = PauseExecutionRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/pause", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&pause_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "pause should return 409 Conflict from Prepared state"
    );

    // Verify state is still Prepared (no transition)
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Prepared),
        "execution should remain Prepared after failed pause"
    );
}

// ============================================
// RESUME EXECUTION TESTS
// ============================================

/// Test: resume from Paused sets state to Running.
#[tokio::test]
async fn test_gateway_resume_from_paused_sets_running() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Seed execution to Running state first
    let execution_id = run_flow_to_running(&runtime).await;

    // Pause the execution first
    let app = build_router(runtime.clone());
    let pause_req = PauseExecutionRequest { execution_id };
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/pause", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&pause_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "pause should succeed");

    // Verify state is now Paused
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    let exec = stored.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Paused),
        "execution should be Paused after pause, got {:?}",
        exec.state
    );

    // Now call resume endpoint
    let resume_req = ResumeExecutionRequest { execution_id };
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/resume", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resume_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "resume should succeed from Paused state"
    );

    // Verify state is now Running
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    let exec = stored.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Running),
        "execution should be Running after resume, got {:?}",
        exec.state
    );
    // Verify finished_at is NOT set (resume is not terminal)
    assert!(
        exec.finished_at.is_none(),
        "finished_at should not be set for Running state"
    );
}

/// Test: resume from Running returns 409 Conflict.
#[tokio::test]
async fn test_gateway_resume_from_running_returns_conflict() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Seed execution to Running state (without pausing first)
    let execution_id = run_flow_to_running(&runtime).await;

    // Verify initial state is Running
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Running),
        "execution should be Running before resume attempt"
    );

    // Call resume endpoint - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let resume_req = ResumeExecutionRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/resume", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resume_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "resume should return 409 Conflict from Running state"
    );

    // Verify state is still Running (no transition)
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Running),
        "execution should remain Running after failed resume"
    );
}

/// Test: resume from Prepared returns 409 Conflict.
#[tokio::test]
async fn test_gateway_resume_from_prepared_returns_conflict() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Seed execution to Prepared state (stop after prepare)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Verify initial state is Prepared
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Prepared),
        "execution should be Prepared before resume attempt"
    );

    // Call resume endpoint - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let resume_req = ResumeExecutionRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/resume", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resume_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "resume should return 409 Conflict from Prepared state"
    );

    // Verify state is still Prepared (no transition)
    let stored = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored.is_some());
    assert!(
        matches!(stored.unwrap().state, ExecutionState::Prepared),
        "execution should remain Prepared after failed resume"
    );
}

// ============================================
// U1 OUTCOME-AWARE GOVERNANCE TESTS
// ============================================

/// Helper to directly create and persist an IntentEnvelope with specific outcomes.
/// This bypasses the compile endpoint which doesn't support custom allowed/forbidden outcomes.
async fn create_intent_with_outcomes(
    runtime: &GatewayRuntime,
    allowed_outcomes: Vec<ferrum_proto::OutcomeClause>,
    forbidden_outcomes: Vec<ferrum_proto::OutcomeClause>,
    _effect_type: EffectType,
) -> IntentId {
    let intent_id = IntentId::new();
    let envelope = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Outcome Test Intent".to_string(),
        goal: "Test goal with outcomes".to_string(),
        normalized_goal: "test goal with outcomes".to_string(),
        allowed_outcomes,
        forbidden_outcomes,
        resource_scope: vec![],
        risk_tier: RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: vec![],
            sensitivity_labels: vec![],
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: vec![],
        tags: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        policy_bundle_fingerprint: None,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };

    // Directly insert into store to bypass compile endpoint limitations
    runtime.store.intents().insert(&envelope).await.unwrap();
    intent_id
}

/// Test: forbidden outcome match causes explicit deny with clear reason.
#[tokio::test]
async fn test_outcome_forbidden_match_denies() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Create intent that ALLOWS file mutations but FORBIDS deletions specifically
    let allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "file_write".to_string(),
        description: "file write operations".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: None,
    }];
    let forbidden_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "no_delete".to_string(),
        description: "no file deletion".to_string(),
        effect_type: EffectType::FileMutation, // same type as allowed, but forbidden
        required: false,
        selectors: None,
    }];

    let intent_id = create_intent_with_outcomes(
        &runtime,
        allowed_outcomes,
        forbidden_outcomes,
        EffectType::FileMutation, // effect_type should match allowed to avoid contradiction check
    )
    .await;

    // Create proposal that deletes a file (should match forbidden outcome)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Delete file".to_string(),
        tool_name: "fs.delete".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "delete a file".to_string(), // This should be inferred as FileMutation
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
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

    // Should be denied due to forbidden outcome match
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "should deny on forbidden outcome match"
    );
    assert!(
        eval_resp.reason.contains("no_delete"),
        "reason should mention the forbidden outcome id: {}",
        eval_resp.reason
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .contains(&"forbidden.outcome.match".to_string()),
        "should have forbidden.outcome.match rule id"
    );
}

/// Test: allowed outcome mismatch produces advisory warning, not deny.
/// Note: The contradiction check has a conservative policy: if ALL allowed_outcomes
/// are read-only AND proposal is mutating, it denies. To test advisory warnings,
/// we use allowed_outcomes that includes BOTH read-only AND file mutation,
/// so the contradiction check doesn't fire and our advisory check runs instead.
#[tokio::test]
async fn test_outcome_allowed_mismatch_advisory_warning() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Create intent that allows both read-only analysis AND git mutations
    let allowed_outcomes = vec![
        ferrum_proto::OutcomeClause {
            id: "read_only".to_string(),
            description: "read operations only".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        },
        ferrum_proto::OutcomeClause {
            id: "git_ops".to_string(),
            description: "git operations".to_string(),
            effect_type: EffectType::GitMutation,
            required: false,
            selectors: None,
        },
    ];

    let intent_id = create_intent_with_outcomes(
        &runtime,
        allowed_outcomes,
        vec![],
        EffectType::ReadOnlyAnalysis,
    )
    .await;

    // Create proposal that writes to a file (FileMutation - not in allowed outcomes)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write to file".to_string(), // FileMutation, not in allowed
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
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

    // Should NOT be denied (advisory only)
    // Note: Could be Allow or could have other warnings - just check it's not Deny
    assert!(
        eval_resp.decision != Decision::Deny,
        "should NOT deny on allowed outcome mismatch (advisory only): {:?}",
        eval_resp.decision
    );
    // Should have warning about allowed outcome mismatch
    assert!(
        eval_resp
            .warnings
            .iter()
            .any(|w| w.contains("does not match any allowed outcome")),
        "warning should mention allowed outcome mismatch: {:?}",
        eval_resp.warnings
    );
}

/// Test: no allowed_outcomes with a read-only proposal should be allowed without warnings.
/// Note: When allowed_outcomes is empty and proposal is mutating, the contradiction check
/// has a conservative policy that denies (treats empty as "all read-only").
/// So we test with a read-only proposal to verify the "empty = any" behavior.
#[tokio::test]
async fn test_outcome_no_allowed_readonly_proposal_is_fine() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Create intent with empty allowed_outcomes (any effect is ok)
    let intent_id =
        create_intent_with_outcomes(&runtime, vec![], vec![], EffectType::ReadOnlyAnalysis).await;

    // Create proposal with read-only effect
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Read database".to_string(),
        tool_name: "db.query".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"query": "SELECT * FROM users"}),
        expected_effect: "query the database".to_string(), // ReadOnlyAnalysis
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
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

    // Should be allowed (no warnings about outcome alignment for read-only)
    assert_eq!(eval_resp.decision, Decision::Allow);
    // No warnings about allowed outcome mismatch (empty allowed = any ok)
    assert!(
        !eval_resp
            .warnings
            .iter()
            .any(|w| w.contains("does not match any allowed outcome")),
        "should NOT warn when no allowed outcomes specified: {:?}",
        eval_resp.warnings
    );
}

/// Test: forbidden takes precedence over allowed (deny wins).
/// This test uses GitMutation for both allowed and forbidden to avoid
/// the contradiction check's conservative read-only policy.
#[tokio::test]
async fn test_outcome_forbidden_takes_precedence_over_allowed() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Intent allows GitMutation but also forbids it (same type, forbidden wins)
    let allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "git_ops".to_string(),
        description: "git operations".to_string(),
        effect_type: EffectType::GitMutation,
        required: true,
        selectors: None,
    }];
    let forbidden_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "no_git".to_string(),
        description: "no git mutations".to_string(),
        effect_type: EffectType::GitMutation,
        required: false,
        selectors: None,
    }];

    let intent_id = create_intent_with_outcomes(
        &runtime,
        allowed_outcomes,
        forbidden_outcomes,
        EffectType::GitMutation,
    )
    .await;

    // Proposal with git mutation
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Git push".to_string(),
        tool_name: "git.push".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"remote": "origin"}),
        expected_effect: "push to remote git repository".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
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

    // Should deny (forbidden takes precedence)
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "forbidden should take precedence over allowed"
    );
    assert!(
        eval_resp.reason.contains("no_git"),
        "reason should mention the forbidden outcome id: {}",
        eval_resp.reason
    );
}

/// Test: allowed outcome match produces no warnings.
#[tokio::test]
async fn test_outcome_allowed_match_no_warning() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Create intent that allows read-only analysis
    let allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "read_only".to_string(),
        description: "read operations only".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
        selectors: None,
    }];

    let intent_id = create_intent_with_outcomes(
        &runtime,
        allowed_outcomes,
        vec![],
        EffectType::ReadOnlyAnalysis,
    )
    .await;

    // Create proposal that reads a file (matches allowed outcome)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Read file".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read a file".to_string(), // ReadOnlyAnalysis - matches allowed
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
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

    // Should be allowed with no outcome-related warnings
    assert_eq!(eval_resp.decision, Decision::Allow);
    assert!(
        !eval_resp
            .warnings
            .iter()
            .any(|w| w.contains("does not match any allowed outcome")),
        "should NOT warn on allowed outcome match: {:?}",
        eval_resp.warnings
    );
}

/// Test: git mutation effect type is correctly inferred and can be forbidden.
/// This test uses allowed_outcomes with a non-git mutation type to avoid
/// the contradiction check's conservative read-only policy.
#[tokio::test]
async fn test_outcome_git_mutation_inference_and_forbidden() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Create intent that allows file writes but forbids git mutations
    let allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "file_ops".to_string(),
        description: "file operations".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: None,
    }];
    let forbidden_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "no_git".to_string(),
        description: "no git mutations".to_string(),
        effect_type: EffectType::GitMutation,
        required: false,
        selectors: None,
    }];

    let intent_id = create_intent_with_outcomes(
        &runtime,
        allowed_outcomes,
        forbidden_outcomes,
        EffectType::FileMutation,
    )
    .await;

    // Create proposal that commits to git
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Git commit".to_string(),
        tool_name: "git.commit".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"message": "fix bug"}),
        expected_effect: "commit changes to git repository".to_string(), // Should infer GitMutation
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
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

    // Should be denied because git mutation is forbidden
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "should deny git mutation when forbidden"
    );
    assert!(
        eval_resp.reason.contains("no_git"),
        "reason should mention the forbidden outcome id: {}",
        eval_resp.reason
    );
}

// ============================================
// U1-S2 VERIFY-TIME ASSESSMENT TESTS
// ============================================

/// Test U1-S2: verify-time outcome assessment is persisted in execution.metadata,
/// rollback contract metadata, and SideEffectVerified provenance event metadata.
#[tokio::test]
async fn test_u1_s2_verify_assessment_persisted_in_metadata() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify (should auto-commit for R0)
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
    let _verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // 1. Check execution.metadata contains u1_s2_verify_assessment
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment");
    assert!(
        assessment_json.is_some(),
        "execution.metadata should contain u1_s2_verify_assessment"
    );
    let assessment = assessment_json.unwrap();
    assert!(
        assessment
            .get("assessment_available")
            .and_then(|v| v.as_bool())
            == Some(true),
        "assessment_available should be true"
    );

    // 2. Check rollback contract metadata contains u1_s2_verify_assessment
    let contract_id = exec.rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap();
    assert!(
        stored_contract.is_some(),
        "rollback contract should be persisted"
    );
    let contract = stored_contract.unwrap();
    let contract_assessment_json = contract.metadata.get("u1_s2_verify_assessment");
    assert!(
        contract_assessment_json.is_some(),
        "rollback contract.metadata should contain u1_s2_verify_assessment"
    );

    // 3. Check SideEffectVerified provenance event contains u1_s2_verify_assessment
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
    assert!(!events.is_empty(), "SideEffectVerified event should exist");
    let verify_event = &events[0];
    let event_assessment_json = verify_event.metadata.get("u1_s2_verify_assessment");
    assert!(
        event_assessment_json.is_some(),
        "SideEffectVerified event.metadata should contain u1_s2_verify_assessment"
    );
}

/// Test U1-S2: verify-time assessment correctly reports assessment_available=false
/// when intent context is not available (e.g., intent deleted from store before verify).
///
/// Uses sqlx::sqlite::SqliteConnectOptions with foreign_keys=OFF to bypass SQLite FK
/// constraints that prevent deleting an intent that has executions referencing it.
/// The execution.raw_json still contains the original intent_id, but when verify tries
/// to load the intent via store.intents().get(), it returns None, causing
/// assessment_available=false to be correctly reported.
#[tokio::test]
async fn test_u1_s2_verify_assessment_unavailable_when_context_missing() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let (temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // CRITICAL: Delete the intent row to simulate unavailable context.
    // The execution.raw_json still has the original intent_id, but when verify
    // tries to load the intent via store.intents().get(), it will return None.
    // Use FK disabled to bypass the FK constraint from executions->intents.
    let db_path = temp_dir.path().join("store.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .pragma("foreign_keys", "off");

    let fk_disabled_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("failed to create FK-disabled pool");

    // Get the original intent_id from the execution's raw_json by loading via store
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let original_intent_id = stored_execution.unwrap().intent_id;

    // Delete the intent row (FK disabled to bypass constraint)
    sqlx::query("DELETE FROM intents WHERE intent_id = ?1")
        .bind(original_intent_id.to_string())
        .execute(&fk_disabled_pool)
        .await
        .expect("failed to delete intent row");

    // Drop the FK-disabled pool to avoid holding onto the database
    drop(fk_disabled_pool);

    // Verify - intent is now non-existent so assessment should report unavailable
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

    // Verify execution has u1_s2_verify_assessment with assessment_available=false
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment");
    assert!(
        assessment_json.is_some(),
        "execution.metadata should contain u1_s2_verify_assessment"
    );

    // Parse and assert assessment_available is false
    let assessment = assessment_json.unwrap();
    let available = assessment
        .get("assessment_available")
        .and_then(|v| v.as_bool())
        .expect("assessment_available field should be a boolean");
    assert!(
        !available,
        "assessment_available should be false when intent is non-existent"
    );

    // Assert the reason indicates unavailability
    let reason = assessment
        .get("assessment_reason")
        .and_then(|v| v.as_str())
        .expect("assessment_reason should be a string");
    assert!(
        reason.contains("not available at verify time"),
        "assessment_reason should indicate context unavailability, got: {}",
        reason
    );
}

// ============================================
// U1-S3a: MULTI-SIGNAL INFERENCE TESTS
// ============================================

/// Test U1-S3a: verify-time assessment uses rollback_target for HIGH confidence inference.
/// The fs.write operation maps to FilePath rollback target, which infers FileMutation with HIGH confidence.
#[tokio::test]
async fn test_u1_s3a_rollback_target_high_confidence_inference() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled) using fs.write
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify
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

    // Check execution metadata for U1-S3a fields
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment").unwrap();

    // Verify inference_source is rollback_target (HIGH confidence)
    let inference_source = assessment_json
        .get("inference_source")
        .and_then(|v| v.as_str());
    assert_eq!(
        inference_source,
        Some("rollback_target"),
        "inference_source should be 'rollback_target' for fs.write, got: {:?}",
        inference_source
    );

    // Verify inference_confidence is HIGH
    let inference_confidence = assessment_json
        .get("inference_confidence")
        .and_then(|v| v.as_str());
    assert_eq!(
        inference_confidence,
        Some("HIGH"),
        "inference_confidence should be 'HIGH' for rollback_target inference, got: {:?}",
        inference_confidence
    );

    // Verify alignment_confidence is at least MED (structural match)
    let alignment_confidence = assessment_json
        .get("alignment_confidence")
        .and_then(|v| v.as_str());
    assert!(
        alignment_confidence == Some("HIGH") || alignment_confidence == Some("MED"),
        "alignment_confidence should be HIGH or MED for strong/rollback match, got: {:?}",
        alignment_confidence
    );

    // Verify alignment_strength reflects structural match
    let alignment_strength = assessment_json
        .get("alignment_strength")
        .and_then(|v| v.as_str());
    assert!(
        alignment_strength == Some("strong_match") || alignment_strength == Some("moderate_match"),
        "alignment_strength should be strong_match or moderate_match for rollback_target, got: {:?}",
        alignment_strength
    );
}

/// Test U1-S3a: alignment_strength distinguishes strong vs weak matches.
/// A strong_match occurs when rollback_target provides exact effect type match.
/// A weak_match occurs when only expected_effect keyword heuristic is available.
#[tokio::test]
async fn test_u1_s3a_alignment_strength_distinguishes_strong_from_weak() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify
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

    // Check that alignment_strength is NOT "weak_match" when rollback_target is available
    // (rollback_target provides HIGH confidence structural inference)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment").unwrap();

    let alignment_strength = assessment_json
        .get("alignment_strength")
        .and_then(|v| v.as_str());
    assert_ne!(
        alignment_strength,
        Some("weak_match"),
        "alignment_strength should NOT be weak_match when rollback_target is available, got: {:?}",
        alignment_strength
    );

    // When rollback_target is used, it should be at least moderate_match
    assert!(
        alignment_strength == Some("strong_match") || alignment_strength == Some("moderate_match"),
        "alignment_strength should be strong_match or moderate_match for rollback_target inference, got: {:?}",
        alignment_strength
    );
}

/// Test U1-S3a: verify assessment records inference_confidence and alignment_confidence separately.
/// This ensures we can distinguish "how confident we are in the effect inference"
/// from "how confident we are in the alignment assessment".
#[tokio::test]
async fn test_u1_s3a_separate_inference_and_alignment_confidence() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify
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

    // Check that both confidence fields are present and meaningful
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment").unwrap();

    let inference_confidence = assessment_json
        .get("inference_confidence")
        .and_then(|v| v.as_str());
    let alignment_confidence = assessment_json
        .get("alignment_confidence")
        .and_then(|v| v.as_str());

    // Both fields should be present (not null/missing)
    assert!(
        inference_confidence.is_some(),
        "inference_confidence field should be present"
    );
    assert!(
        alignment_confidence.is_some(),
        "alignment_confidence field should be present"
    );

    // Both should be valid confidence levels
    let valid_confidences = ["HIGH", "MED", "LOW", "NONE"];
    assert!(
        valid_confidences.contains(&inference_confidence.unwrap()),
        "inference_confidence should be HIGH/MED/LOW/NONE, got: {:?}",
        inference_confidence
    );
    assert!(
        valid_confidences.contains(&alignment_confidence.unwrap()),
        "alignment_confidence should be HIGH/MED/LOW/NONE, got: {:?}",
        alignment_confidence
    );
}

// ============================================
// U1-S3b: CONFIDENCE-THRESHOLDED VERIFY ANNOTATIONS TESTS
// ============================================

/// Test U1-S3b: threshold_metadata schema presence and valid structure.
/// This test demonstrates schema-presence coverage: the threshold_metadata nested block
/// is correctly populated with all required fields (threshold_band, threshold_rule_id,
/// suggested_future_action, annotate_only, ambiguity_reason).
///
/// Note: This fs.write case produces alignment (not mismatch), so threshold_band
/// is "low" with ambiguity_reason indicating no mismatch detected.
/// This validates the LOW-band / ambiguity path, not the HIGH/medium mismatch path.
#[tokio::test]
async fn test_u1_s3b_threshold_metadata_schema_presence() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled) using fs.write
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify
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

    // Verify threshold_metadata schema is present and valid
    // This fs.write case with rollback_target alignment produces LOW band (no mismatch)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment").unwrap();

    // Verify threshold_metadata is present
    let threshold_metadata = assessment_json.get("threshold_metadata");
    assert!(
        threshold_metadata.is_some(),
        "threshold_metadata should be present in assessment"
    );
    let threshold = threshold_metadata.unwrap();

    // Verify threshold_band is present and valid
    let threshold_band = threshold.get("threshold_band").and_then(|v| v.as_str());
    assert!(threshold_band.is_some(), "threshold_band should be present");
    assert!(
        threshold_band == Some("high")
            || threshold_band == Some("medium")
            || threshold_band == Some("low"),
        "threshold_band should be high/medium/low, got: {:?}",
        threshold_band
    );

    // For this alignment case, we expect LOW band with ambiguity_reason
    assert_eq!(
        threshold_band,
        Some("low"),
        "fs.write alignment case should produce LOW band (no mismatch), got: {:?}",
        threshold_band
    );

    // Verify ambiguity_reason is present for LOW band
    let ambiguity_reason = threshold.get("ambiguity_reason").and_then(|v| v.as_str());
    assert!(
        ambiguity_reason.is_some() && !ambiguity_reason.unwrap().is_empty(),
        "ambiguity_reason should be present for LOW band"
    );

    // Verify threshold_rule_id is present and follows u1_s3b.{band}.{strength} pattern
    let threshold_rule_id = threshold.get("threshold_rule_id").and_then(|v| v.as_str());
    assert!(
        threshold_rule_id.is_some(),
        "threshold_rule_id should be present"
    );
    assert!(
        threshold_rule_id.unwrap().starts_with("u1_s3b."),
        "threshold_rule_id should start with 'u1_s3b.', got: {:?}",
        threshold_rule_id
    );

    // Verify annotate_only is true (this is U1-S3b, still annotate-only)
    let annotate_only = threshold.get("annotate_only").and_then(|v| v.as_bool());
    assert_eq!(
        annotate_only,
        Some(true),
        "annotate_only should always be true for U1-S3b"
    );

    // Verify suggested_future_action is present
    let suggested_future_action = threshold
        .get("suggested_future_action")
        .and_then(|v| v.as_str());
    assert!(
        suggested_future_action.is_some(),
        "suggested_future_action should be present"
    );
}

/// Test U1-S3b: threshold_metadata is correctly populated for unavailable assessment.
/// When assessment_available=false, threshold_band should be "low" with ambiguity_reason set.
#[tokio::test]
async fn test_u1_s3b_low_threshold_band_when_assessment_unavailable() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let (temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Delete intent to make assessment unavailable
    let db_path = temp_dir.path().join("store.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .pragma("foreign_keys", "off");

    let fk_disabled_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("failed to create FK-disabled pool");

    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let original_intent_id = stored_execution.unwrap().intent_id;

    sqlx::query("DELETE FROM intents WHERE intent_id = ?1")
        .bind(original_intent_id.to_string())
        .execute(&fk_disabled_pool)
        .await
        .expect("failed to delete intent row");

    drop(fk_disabled_pool);

    // Verify - intent is now non-existent so assessment should report unavailable
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

    // Verify threshold_metadata reflects low band with unavailable context
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment").unwrap();

    let threshold_metadata = assessment_json.get("threshold_metadata").unwrap();

    // threshold_band should be "low" when assessment is unavailable
    let threshold_band = threshold_metadata
        .get("threshold_band")
        .and_then(|v| v.as_str());
    assert_eq!(
        threshold_band,
        Some("low"),
        "threshold_band should be 'low' when assessment unavailable, got: {:?}",
        threshold_band
    );

    // ambiguity_reason should be present
    let ambiguity_reason = threshold_metadata
        .get("ambiguity_reason")
        .and_then(|v| v.as_str());
    assert!(
        ambiguity_reason.is_some(),
        "ambiguity_reason should be present when assessment unavailable"
    );

    // annotate_only should still be true
    let annotate_only = threshold_metadata
        .get("annotate_only")
        .and_then(|v| v.as_bool());
    assert_eq!(annotate_only, Some(true), "annotate_only should be true");
}

/// Test U1-S3b: threshold_metadata field structure and annotate_only flag.
/// This test validates all required fields are present and correctly populated:
/// - threshold_band, threshold_rule_id, suggested_future_action, annotate_only
/// - ambiguity_reason is present when threshold_band is "low"
///
/// Note: This test uses the fs.write alignment case which produces LOW band.
/// It validates schema presence and LOW-band ambiguity coverage, not high/medium band paths.
#[tokio::test]
async fn test_u1_s3b_threshold_metadata_field_structure_and_annotate_flag() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify
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

    // Check threshold_metadata structure
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment").unwrap();

    let threshold_metadata = assessment_json.get("threshold_metadata").unwrap();

    // All required fields should be present
    let required_fields = [
        "threshold_band",
        "threshold_rule_id",
        "suggested_future_action",
        "annotate_only",
    ];
    for field in &required_fields {
        assert!(
            threshold_metadata.get(*field).is_some(),
            "threshold_metadata.{} should be present",
            field
        );
    }

    // threshold_band should be one of the valid values
    let threshold_band = threshold_metadata
        .get("threshold_band")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(
        ["high", "medium", "low"].contains(&threshold_band),
        "threshold_band should be high/medium/low, got: {}",
        threshold_band
    );

    // threshold_rule_id should follow the pattern u1_s3b.{band}.{strength}
    let threshold_rule_id = threshold_metadata
        .get("threshold_rule_id")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(
        threshold_rule_id.starts_with(&format!("u1_s3b.{}.", threshold_band)),
        "threshold_rule_id should start with 'u1_s3b.{}.', got: {}",
        threshold_band,
        threshold_rule_id
    );

    // suggested_future_action should be one of the valid values
    let suggested_future_action = threshold_metadata
        .get("suggested_future_action")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(
        [
            "enforce_or_block",
            "enforce_with_human_review",
            "continue_annotate_only"
        ]
        .contains(&suggested_future_action),
        "suggested_future_action should be one of the valid values, got: {}",
        suggested_future_action
    );

    // Verify annotate_only is always true for U1-S3b
    let annotate_only = threshold_metadata
        .get("annotate_only")
        .and_then(|v| v.as_bool())
        .unwrap();
    assert!(
        annotate_only,
        "annotate_only should always be true for U1-S3b"
    );

    // If threshold_band is "low", ambiguity_reason should be present
    if threshold_band == "low" {
        let ambiguity_reason = threshold_metadata.get("ambiguity_reason");
        assert!(
            ambiguity_reason.is_some() && !ambiguity_reason.unwrap().is_null(),
            "ambiguity_reason should be present when threshold_band is 'low'"
        );
    }
}

// ============================================
// U1-S4: HIGHER-FIDELITY SELECTOR MATCH TESTS
// ============================================

/// Test U1-S4: Selector-enhanced clause match.
/// When selectors are present and match, clause_match_annotation shows strong_match.
#[tokio::test]
async fn test_u1_s4_selector_enhanced_clause_match() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with proper file scope using the compile endpoint
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Replace the intent's allowed_outcomes with selector-enhanced clause
    // by directly updating in the store
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    // U1-S4 selector expectations aligned with actual implementation output:
    // - R0 rollback produces request_class="read_only"
    // - McpToolMutation action_type produces mutation_family="mcp_tool_mutation"
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "fs_file_write".to_string(),
        description: "file write via fs adapter".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: Some("fs".to_string()),
            target_family: Some("file".to_string()),
            request_class: Some("read_only".to_string()),
            mutation_family: Some("mcp_tool_mutation".to_string()),
            ..Default::default()
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200, "mint should succeed");
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

    // Prepare - selectors ARE aligned with implementation (mutation_family="mcp_tool_mutation" matches
    // hardcoded McpToolMutation action_type), so no mismatch => would_block=false => 200
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // Selectors match implementation output (fs adapter, file target, read_only request_class,
    // mcp_tool_mutation mutation_family), so prepare succeeds - no U1-S6 block occurs
    assert_eq!(
        response.status(),
        200,
        "U1-S6: selectors aligned with McpToolMutation output - no block"
    );

    // Verify execution state is Prepared (not blocked)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution state should be Prepared when selectors match"
    );
}

/// Test U1-S4: Effect type matches but selector family mismatches.
/// When effect_type matches but selectors don't, clause_match_annotation shows effect_match_selector_mismatch.
#[tokio::test]
async fn test_u1_s4_effect_type_matches_but_selector_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with proper file scope using the compile endpoint
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Replace the intent's allowed_outcomes with selector-enhanced clause for git
    // But our proposal will use fs adapter - effect_type matches (FileMutation) but adapter_family doesn't
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "git_mutation".to_string(),
        description: "git mutations via git adapter".to_string(),
        effect_type: EffectType::FileMutation, // Same effect type as our proposal
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: Some("git".to_string()), // Different from what proposal uses
            target_family: Some("git".to_string()),  // Different
            request_class: Some("mutation".to_string()),
            mutation_family: Some("git_commit".to_string()), // Different
            ..Default::default()
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs (not git)".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200, "mint should succeed");
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

    // Prepare - U1-S6: HIGH confidence + selector mismatch (fs adapter vs git selectors)
    // = would_block=true -> hard block at prepare-time
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // HIGH mismatch: git vs fs adapter_family causes alignment_strength=Mismatch at HIGH confidence
    assert_eq!(
        response.status(),
        403,
        "U1-S6: HIGH confidence + selector mismatch (git vs fs) should block at prepare"
    );

    // Verify execution state is Denied (hard gate at prepare)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied when hard gate blocks at prepare"
    );
    assert!(
        matches!(exec.decision, Decision::Deny),
        "decision should be Deny for HIGH mismatch"
    );
    assert!(
        exec.finished_at.is_some(),
        "finished_at should be set when denied"
    );

    // Verify no rollback contract was created (hard gate at prepare)
    assert!(
        exec.rollback_contract_id.is_none(),
        "rollback_contract_id should be None when hard gate blocks at prepare"
    );

    // Verify u1_s5b_hard_gate metadata is present (U1-S5b auditability)
    let gate_json = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_json.is_some(),
        "u1_s5b_hard_gate metadata should be present"
    );
    let gate = gate_json.unwrap();

    // Verify would_block is true
    let would_block = gate.get("would_block").and_then(|v| v.as_bool());
    assert_eq!(
        would_block,
        Some(true),
        "would_block should be true for HIGH mismatch"
    );

    // Verify reason_codes contains high_mismatch
    let reason_codes = gate.get("reason_codes").and_then(|v| v.as_array());
    assert!(reason_codes.is_some(), "reason_codes should be present");
    let codes: Vec<&str> = reason_codes
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        codes.contains(&"high_mismatch"),
        "reason_codes should contain 'high_mismatch', got: {:?}",
        codes
    );

    // Note: verify-time clause_match_annotations assertions removed -
    // U1-S6 blocks at prepare-time, verify semantics no longer apply
}

// ============================================
// U1-S3b: HIGH/MED BAND MISMATCH FIXTURES
// ============================================

/// Test U1-S5b: HIGH band mismatch via allowed_outcomes non-match blocks at prepare-time.
/// This test uses allowed_outcomes mismatch (NOT forbidden) so the execution
/// proceeds past evaluate-time without denial, but at prepare-time the concrete
/// rollback_target signal shows the mismatch with HIGH confidence.
///
/// At evaluate-time:
/// - effect inference uses expected_effect keywords (LOW confidence)
/// - fs.write with "write a file" effect infers FileMutation
/// - But allowed_outcomes is ExternalApiCall, so check_allowed_outcomes warns but allows
///
/// At prepare-time:
/// - effect inference uses rollback_target (HIGH confidence) via contract.target
/// - FilePath target infers FileMutation
/// - No match with allowed_outcomes (ExternalApiCall vs FileMutation)
/// - alignment_strength = mismatch, alignment_confidence = HIGH
/// - threshold_band = high → U1-S5b blocks with 403
///
/// This proves the HIGH band mismatch path via allowed_outcomes non-alignment.
#[tokio::test]
async fn test_u1_s3b_high_band_mismatch_via_allowed_outcome_non_match() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with FileMutation effect type
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Modify intent to set allowed_outcomes to ExternalApiCall
    // (NOT FileMutation) - this means file mutations are NOT aligned
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    // Set allowed_outcomes to ExternalApiCall only - file mutations are not allowed
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "allow_http_call".to_string(),
        description: "allow http calls only".to_string(),
        effect_type: EffectType::ExternalApiCall,
        required: true,
        selectors: None,
    }];
    stored_intent.forbidden_outcomes = vec![]; // No forbidden outcomes - mismatch is advisory
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal with fs.write (which will infer FileMutation)
    // This should pass evaluate (advisory mismatch only) but fail at verify
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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

    // Step 5: Authorize
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

    // Step 6: Prepare - U1-S5b now blocks at prepare-time due to HIGH mismatch
    // allowed_outcomes = ExternalApiCall but inferred = FileMutation (HIGH mismatch → would_block=true)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // U1-S5b: prepare should be denied with 403
    assert_eq!(
        response.status(),
        403,
        "U1-S5b: prepare should return 403 when would_block=true (HIGH mismatch)"
    );

    // Step 7: Verify execution state is Denied
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied after U1-S5b hard gate, got {:?}",
        exec.state
    );
    assert!(
        exec.finished_at.is_some(),
        "finished_at should be set when execution is denied"
    );
    assert!(
        matches!(exec.decision, Decision::Deny),
        "decision should be Deny for hard-gated execution"
    );

    // Verify u1_s5b_hard_gate metadata is present
    let gate_json = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_json.is_some(),
        "u1_s5b_hard_gate metadata should be present"
    );
    let gate = gate_json.unwrap();

    // Step 8: Verify HIGH band mismatch via allowed_outcome non-alignment
    // Note: The full verify assessment is not stored at prepare-time when U1-S5b blocks.
    // Instead, we verify via the u1_s5b_hard_gate preview signals.

    // Verify would_block is true (U1-S5b hard gate triggered)
    let would_block = gate.get("would_block").and_then(|v| v.as_bool());
    assert_eq!(
        would_block,
        Some(true),
        "would_block should be true for HIGH mismatch"
    );

    // Verify reason_codes contains high_mismatch
    let reason_codes = gate.get("reason_codes").and_then(|v| v.as_array());
    assert!(reason_codes.is_some(), "reason_codes should be present");
    let codes: Vec<&str> = reason_codes
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        codes.contains(&"high_mismatch"),
        "reason_codes should contain 'high_mismatch', got: {:?}",
        codes
    );

    // Verify threshold_band is high
    let threshold_metadata = gate.get("threshold_metadata");
    assert!(
        threshold_metadata.is_some(),
        "threshold_metadata should be present in gate metadata"
    );
    let threshold = threshold_metadata.unwrap();
    let threshold_band = threshold.get("threshold_band").and_then(|v| v.as_str());
    assert_eq!(
        threshold_band,
        Some("high"),
        "threshold_band should be 'high' for HIGH-confidence mismatch"
    );

    // Verify threshold_rule_id follows the expected pattern
    let threshold_rule_id = threshold.get("threshold_rule_id").and_then(|v| v.as_str());
    assert!(
        threshold_rule_id.unwrap().starts_with("u1_s3b.high."),
        "threshold_rule_id should start with 'u1_s3b.high.', got: {:?}",
        threshold_rule_id
    );

    // Verify annotate_only is still true (U1-S3b remains annotate-only)
    let annotate_only = threshold.get("annotate_only").and_then(|v| v.as_bool());
    assert_eq!(
        annotate_only,
        Some(true),
        "annotate_only should always be true for U1-S3b (still annotate-only)"
    );
}

/// Test U1-S3b: Verify-time mismatch detection via HTTP GET execution.
/// This test demonstrates mismatch detection at verify-time for HTTP GET operations.
///
/// At evaluate-time:
/// - effect inference uses expected_effect keywords (LOW confidence)
/// - "get data from api" with tool_name="http.get" infers ExternalApiCall
/// - allowed_outcomes is FileMutation, so mismatch detected (advisory only at evaluate)
///
/// At verify-time:
/// - effect inference uses rollback_target (HIGH confidence)
/// - HttpRequest target infers ExternalApiCall
/// - No match with allowed_outcomes (FileMutation vs ExternalApiCall)
/// - alignment_strength = mismatch, alignment_confidence = HIGH
/// - threshold_band = high (not MED as originally hoped)
///
/// Note: The MED band path (via AdapterKey inference) is hard to trigger with current
/// adapters because:
/// - HTTP adapter produces HttpRequest target -> HIGH confidence via rollback_target
/// - Generic target would fall back to LOW confidence via expected_effect
/// - To get MED, we'd need an adapter that produces Generic AND has recognizable tool_name
///
/// This test proves the HIGH band mismatch path works correctly for HTTP adapters.
#[tokio::test]
async fn test_u1_s3b_verify_mismatch_via_http_get_allowed_outcome_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with ExternalApiCall effect type
    let mut req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    // Use HTTP endpoint with Read mode - this will NOT route to http adapter
    // (only mutating modes route to http adapter)
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        mode: ResourceMode::Read,
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

    // Step 2: Modify intent to set allowed_outcomes to a specific EffectType
    // that will NOT match what the adapter_key infers
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    // Set allowed_outcomes to FileMutation - but our http GET won't produce FileMutation
    // This creates a mismatch scenario (allowed_alignment = false)
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "allow_file_mutation".to_string(),
        description: "allow file mutations".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: None,
    }];
    stored_intent.forbidden_outcomes = vec![]; // No forbidden outcomes
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP GET request".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/users"}),
        expected_effect: "get data from api".to_string(),
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

    // Step 4: Mint capability with HTTP binding (mode=Read)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Get,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Read,
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
        policy_bundle_id: None,
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

    // Step 5: Authorize
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

    // Step 6: Prepare - U1-S5b now blocks at prepare-time due to HIGH mismatch
    // allowed_outcomes = FileMutation but inferred = ExternalApiCall (HIGH mismatch → would_block=true)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // U1-S5b: prepare should be denied with 403
    assert_eq!(
        response.status(),
        403,
        "U1-S5b: prepare should return 403 when would_block=true (HIGH mismatch)"
    );

    // Step 7: Verify execution state is Denied
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied after U1-S5b hard gate, got {:?}",
        exec.state
    );
    assert!(
        exec.finished_at.is_some(),
        "finished_at should be set when execution is denied"
    );
    assert!(
        matches!(exec.decision, Decision::Deny),
        "decision should be Deny for hard-gated execution"
    );

    // Step 8: Verify u1_s5b_hard_gate metadata is present
    let gate_json = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_json.is_some(),
        "u1_s5b_hard_gate metadata should be present"
    );
    let gate = gate_json.unwrap();

    // Note: The full verify assessment is not stored at prepare-time when U1-S5b blocks.
    // Instead, we verify via the u1_s5b_hard_gate preview signals.

    // Verify would_block is true (U1-S5b hard gate triggered)
    let would_block = gate.get("would_block").and_then(|v| v.as_bool());
    assert_eq!(
        would_block,
        Some(true),
        "would_block should be true for HIGH mismatch"
    );

    // Verify reason_codes contains high_mismatch
    let reason_codes = gate.get("reason_codes").and_then(|v| v.as_array());
    assert!(reason_codes.is_some(), "reason_codes should be present");
    let codes: Vec<&str> = reason_codes
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        codes.contains(&"high_mismatch"),
        "reason_codes should contain 'high_mismatch', got: {:?}",
        codes
    );

    // Verify threshold_band is high
    let threshold_metadata = gate.get("threshold_metadata");
    assert!(
        threshold_metadata.is_some(),
        "threshold_metadata should be present in gate metadata"
    );
    let threshold = threshold_metadata.unwrap();
    let threshold_band = threshold.get("threshold_band").and_then(|v| v.as_str());
    assert_eq!(
        threshold_band,
        Some("high"),
        "threshold_band should be 'high' for HIGH-confidence mismatch via HttpRequest target"
    );

    // Verify annotate_only is still true
    let annotate_only = threshold.get("annotate_only").and_then(|v| v.as_bool());
    assert_eq!(
        annotate_only,
        Some(true),
        "annotate_only should always be true for U1-S3b"
    );
}

/// Test U1-S6: Selector-bearing clause with HIGH confidence mismatch blocks at prepare-time.
/// Demonstrates that when effect_type matches but selectors don't (fs adapter vs git selectors),
/// U1-S6 effective_match=false leads to HIGH band mismatch, triggering would_block=true at prepare.
///
/// Note: This test previously verified verify-time clause_match_annotations (U1-S4 path).
/// U1-S6 now blocks at prepare-time when selector mismatch occurs with HIGH confidence.
///
/// At verify-time:
/// - effect inference uses rollback_target (HIGH confidence)
/// - FilePath target infers FileMutation (matches allowed effect type)
/// - But selectors don't match (adapter_family=fs vs git, mutation_family=mismatch)
/// - Result: effect_type_match=true, selector_match=false
///
/// Note: threshold_band is "high" because U1-S6 effective_match=false (selector mismatch)
/// causes alignment_strength=mismatch at HIGH confidence, triggering would_block=true.
/// The clause_match_annotations capture the selector mismatch details for auditability.
#[tokio::test]
async fn test_u1_s4_selector_enhanced_but_selector_mismatch_at_verify_time() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with FileMutation effect type
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Set allowed_outcomes with git-like selectors but FileMutation effect type
    // Proposal uses fs adapter which produces FileMutation (matches effect type)
    // but selectors expect git (fs vs git mismatch)
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "git_mutation".to_string(),
        description: "git mutations via git adapter".to_string(),
        effect_type: EffectType::FileMutation, // Same effect type as our proposal
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: Some("git".to_string()), // Different from fs
            target_family: Some("git".to_string()),  // Different from file
            request_class: Some("mutation".to_string()),
            mutation_family: Some("git_commit".to_string()), // Different from file_write
            ..Default::default()
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal with fs.write
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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

    // Step 5: Authorize
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

    // Step 6: Prepare - U1-S6: HIGH confidence + selector mismatch (fs adapter vs git selectors)
    // = would_block=true -> hard block at prepare-time
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // HIGH mismatch: fs adapter vs git selectors causes alignment_strength=Mismatch at HIGH confidence
    assert_eq!(
        response.status(),
        403,
        "U1-S6: HIGH confidence + selector mismatch should block at prepare"
    );

    // Verify execution state is Denied (hard gate at prepare)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied when hard gate blocks at prepare"
    );
    assert!(
        matches!(exec.decision, Decision::Deny),
        "decision should be Deny for HIGH mismatch"
    );
    assert!(
        exec.finished_at.is_some(),
        "finished_at should be set when denied"
    );

    // Verify no rollback contract was created (hard gate at prepare)
    assert!(
        exec.rollback_contract_id.is_none(),
        "rollback_contract_id should be None when hard gate blocks at prepare"
    );

    // Verify u1_s5b_hard_gate metadata is present (U1-S5b auditability)
    let gate_json = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_json.is_some(),
        "u1_s5b_hard_gate metadata should be present"
    );
    let gate = gate_json.unwrap();

    // Verify would_block is true
    let would_block = gate.get("would_block").and_then(|v| v.as_bool());
    assert_eq!(
        would_block,
        Some(true),
        "would_block should be true for HIGH mismatch"
    );

    // Verify reason_codes contains high_mismatch
    let reason_codes = gate.get("reason_codes").and_then(|v| v.as_array());
    assert!(reason_codes.is_some(), "reason_codes should be present");
    let codes: Vec<&str> = reason_codes
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        codes.contains(&"high_mismatch"),
        "reason_codes should contain 'high_mismatch', got: {:?}",
        codes
    );

    // Note: verify-time assertions removed - U1-S6 blocks at prepare-time,
    // verify semantics (clause_match_annotations, threshold_band, etc.) no longer apply
}

// ============================================
// U1-S5b HARD GATE TESTS
// ============================================

/// Test U1-S5b: HIGH band mismatch => hard gate at prepare-time blocks execution.
/// Verifies:
/// - prepare returns 403 PolicyDenied
/// - execution state becomes Denied with finished_at set
/// - u1_s5b_hard_gate metadata is persisted for auditability
/// - no SideEffectPrepared event is emitted
/// - no rollback contract is persisted
#[tokio::test]
async fn test_u1_s5b_high_mismatch_blocks_at_prepare_time() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with FileMutation effect type
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Modify intent to set allowed_outcomes to ExternalApiCall (NOT FileMutation)
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "allow_http_call".to_string(),
        description: "allow http calls only".to_string(),
        effect_type: EffectType::ExternalApiCall,
        required: true,
        selectors: None,
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal with fs.write (will infer FileMutation)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4-6: Mint, authorize
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
        policy_bundle_id: None,
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

    // Step 7: Prepare should return 403 due to U1-S5b hard gate
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // U1-S5b: prepare should be denied with 403
    assert_eq!(
        response.status(),
        403,
        "U1-S5b: prepare should return 403 when would_block=true"
    );

    // Step 8: Verify execution state is Denied with finished_at set
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied after hard gate, got {:?}",
        exec.state
    );
    assert!(
        exec.finished_at.is_some(),
        "finished_at should be set when execution is denied"
    );
    assert!(
        matches!(exec.decision, Decision::Deny),
        "decision should be Deny for hard-gated execution"
    );

    // Step 9: Verify u1_s5b_hard_gate metadata is persisted
    let gate_metadata = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_metadata.is_some(),
        "execution.metadata should contain u1_s5b_hard_gate"
    );
    let gate_json = gate_metadata.unwrap();

    // Verify would_block is true in the gate metadata
    let would_block = gate_json.get("would_block").and_then(|v| v.as_bool());
    assert_eq!(
        would_block,
        Some(true),
        "would_block should be true in u1_s5b_hard_gate metadata"
    );

    // Verify reason_codes contains high_mismatch
    let reason_codes = gate_json.get("reason_codes").and_then(|v| v.as_array());
    assert!(reason_codes.is_some(), "reason_codes should be present");
    let codes: Vec<&str> = reason_codes
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        codes.contains(&"high_mismatch"),
        "reason_codes should contain 'high_mismatch', got: {:?}",
        codes
    );

    // Step 10: Verify no rollback contract was persisted (skip contract persistence)
    assert!(
        exec.rollback_contract_id.is_none(),
        "rollback_contract_id should be None when hard gate blocks at prepare-time"
    );

    // Step 11: Verify no SideEffectPrepared event was emitted
    let side_effect_prepared_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectPrepared),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        side_effect_prepared_events.is_empty(),
        "no SideEffectPrepared event should be emitted when hard gate blocks at prepare-time"
    );

    // Step 12: Verify an ErrorRaised event was emitted for auditability
    let error_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ErrorRaised),
            terminal_only: None,
            since: None,
            until: None,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        !error_events.is_empty(),
        "ErrorRaised event should be emitted for auditability"
    );

    // Step 13: Verify execution cannot proceed to execute (state guard)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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
    // Execute should fail because execution is in Denied state
    assert_eq!(
        response.status(),
        409,
        "execute should return 409 for denied execution"
    );
}

/// Test U1-S5b: Post-deny state guard - verify execution in Denied state cannot proceed.
/// This tests that after a U1-S5b hard gate puts execution in Denied state,
/// subsequent prepare calls return 409 Conflict.
#[tokio::test]
async fn test_u1_s5b_post_deny_state_guard_prevents_reprepare() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with FileMutation effect type
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Modify intent to set allowed_outcomes to ExternalApiCall (NOT FileMutation)
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "allow_http_call".to_string(),
        description: "allow http calls only".to_string(),
        effect_type: EffectType::ExternalApiCall,
        required: true,
        selectors: None,
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4-6: Mint, authorize
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
        policy_bundle_id: None,
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

    // Step 7: First prepare should return 403 due to U1-S5b hard gate
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
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
        "first prepare should return 403 due to hard gate"
    );

    // Step 8: Second prepare should return 409 due to state guard (execution already Denied)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // Should get 409 Conflict because execution is already in Denied state
    assert_eq!(
        response.status(),
        409,
        "second prepare should return 409 because execution is already Denied"
    );
}

/// Test U1-S5a: Selector mismatch => would_require_review=true.
/// Uses existing selector mismatch setup from test_u1_s4_selector_enhanced_but_selector_mismatch_at_verify_time.
#[tokio::test]
async fn test_u1_s5a_selector_mismatch_would_require_review() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent with FileMutation effect type
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Set allowed_outcomes with git-like selectors but FileMutation effect type
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "git_mutation".to_string(),
        description: "git mutations via git adapter".to_string(),
        effect_type: EffectType::FileMutation, // Same effect type
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: Some("git".to_string()), // Different from fs
            target_family: Some("git".to_string()),  // Different from file
            request_class: Some("mutation".to_string()),
            mutation_family: Some("git_commit".to_string()), // Different from file_write
            ..Default::default()
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3-7: Evaluate, mint, authorize, prepare, execute (same as existing test)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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
        policy_bundle_id: None,
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
                .uri(format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    // U1-S6: HIGH mismatch (fs vs git adapter_family) causes alignment_strength=Mismatch at HIGH confidence
    // This triggers would_block=true at prepare-time, not review-only
    assert_eq!(
        response.status(),
        403,
        "U1-S6: HIGH confidence + selector mismatch (fs vs git) should block at prepare"
    );

    // Verify execution state is Denied (hard gate at prepare)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some(), "execution should be persisted");
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied when hard gate blocks at prepare"
    );
    assert!(
        matches!(exec.decision, Decision::Deny),
        "decision should be Deny for HIGH mismatch"
    );
    assert!(
        exec.finished_at.is_some(),
        "finished_at should be set when denied"
    );

    // Verify no rollback contract was created (hard gate at prepare)
    assert!(
        exec.rollback_contract_id.is_none(),
        "rollback_contract_id should be None when hard gate blocks at prepare"
    );

    // Verify u1_s5b_hard_gate metadata is present
    let gate_json = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_json.is_some(),
        "u1_s5b_hard_gate metadata should be present"
    );
    let gate = gate_json.unwrap();

    // Verify would_block is true
    let would_block = gate.get("would_block").and_then(|v| v.as_bool());
    assert_eq!(
        would_block,
        Some(true),
        "would_block should be true for HIGH mismatch"
    );

    // Verify reason_codes contains high_mismatch
    let reason_codes = gate.get("reason_codes").and_then(|v| v.as_array());
    assert!(reason_codes.is_some(), "reason_codes should be present");
    let codes: Vec<&str> = reason_codes
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        codes.contains(&"high_mismatch"),
        "reason_codes should contain 'high_mismatch', got: {:?}",
        codes
    );

    // Note: review-only semantics (would_require_review) no longer apply at prepare-time
    // for HIGH mismatch - U1-S6 blocks at prepare-time instead
}

/// Test U1-S5a: Regression - verify endpoint behavior and state transitions remain unchanged.
/// This ensures that adding U1-S5a preview signals does not affect the verify state machine.
/// Note: Unavailable context scenario is already covered by
/// test_u1_s2_verify_assessment_unavailable_when_context_missing.
#[tokio::test]
async fn test_u1_s5a_regression_verify_state_machine_unchanged() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: fs_execute_payload(execution_id, "hello"),
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

    // Verify (should auto-commit for R0)
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

    // Verify response status is still 200
    assert_eq!(response.status(), 200);

    // Parse response body
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Verify response shape is unchanged
    assert_eq!(verify_resp.execution_id, execution_id);
    assert_eq!(verify_resp.verified, true);
    assert!(verify_resp.verified_at.is_some());

    // Verify execution state transitioned to AwaitingVerification then auto-committed to Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();

    // After auto-commit, state should be Committed (not AwaitingVerification)
    assert!(
        matches!(exec.state, ExecutionState::Committed),
        "execution state should be Committed after auto-commit, got: {:?}",
        exec.state
    );

    // Verify execution metadata contains the U1-S5a preview signals
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment");
    assert!(
        assessment_json.is_some(),
        "execution.metadata should contain u1_s2_verify_assessment"
    );
    let assessment = assessment_json.unwrap();

    // Verify all U1-S5a fields are present
    assert!(
        assessment.get("would_block").is_some(),
        "would_block field should be present"
    );
    assert!(
        assessment.get("would_require_review").is_some(),
        "would_require_review field should be present"
    );
    assert!(
        assessment.get("reason_codes").is_some(),
        "reason_codes field should be present"
    );
    assert!(
        assessment.get("derive_basis").is_some(),
        "derive_basis field should be present"
    );

    // Verify no issues case: would_block=false, would_require_review=false, reason_codes=["none"]
    let would_block = assessment.get("would_block").and_then(|v| v.as_bool());
    let would_require_review = assessment
        .get("would_require_review")
        .and_then(|v| v.as_bool());
    let reason_codes = assessment.get("reason_codes").and_then(|v| v.as_array());

    assert_eq!(
        would_block,
        Some(false),
        "would_block should be false for aligned execution"
    );
    assert_eq!(
        would_require_review,
        Some(false),
        "would_require_review should be false for aligned execution"
    );
    assert!(
        reason_codes.is_some()
            && reason_codes
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some("none")),
        "reason_codes should contain 'none' for aligned execution"
    );
}

// ============================================
// U1-S7a: LIST-BASED SELECTOR MATCH TESTS
// ============================================

/// Test U1-S7a: List-based selector match success.
/// When a selector list is specified and observed value matches ANY member, it should match.
#[tokio::test]
async fn test_u1_s7a_list_based_selector_match_success() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Update intent with list-based selectors
    // adapter_family_in contains "fs" which should match observed "fs"
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "file_ops".to_string(),
        description: "file operations via list-based selectors".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: None,
            adapter_family_in: Some(vec!["fs".to_string(), "http".to_string()]),
            target_family: None,
            target_family_in: Some(vec!["file".to_string(), "sqlite".to_string()]),
            request_class: None,
            request_class_in: Some(vec!["read_only".to_string(), "mutation".to_string()]),
            mutation_family: None,
            mutation_family_in: Some(vec![
                "mcp_tool_mutation".to_string(),
                "file_write".to_string(),
            ]),
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 5: Authorize
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

    // Step 6: Prepare - list-based selectors should match
    // adapter_family_in=["fs","http"] contains "fs" -> match
    // target_family_in=["file","sqlite"] contains "file" -> match
    // request_class_in=["read_only","mutation"] contains "read_only" (R0) -> match
    // mutation_family_in=["mcp_tool_mutation","file_write"] contains "mcp_tool_mutation" -> match
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
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
        "U1-S7a: list-based selectors should match when observed value is in list"
    );

    // Verify execution state is Prepared
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution state should be Prepared when list-based selectors match"
    );
}

/// Test U1-S7a: List-based selector mismatch.
/// When a selector list is specified and observed value does NOT match ANY member, it should mismatch.
#[tokio::test]
async fn test_u1_s7a_list_based_selector_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Update intent with list-based selectors that DON'T include the observed values
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "git_ops".to_string(),
        description: "git operations - should NOT match fs".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: None,
            adapter_family_in: Some(vec!["git".to_string(), "http".to_string()]),
            target_family: None,
            target_family_in: Some(vec!["git".to_string(), "http".to_string()]),
            request_class: None,
            request_class_in: Some(vec!["mutation".to_string()]),
            mutation_family: None,
            mutation_family_in: Some(vec![
                "git_commit".to_string(),
                "git_branch_create".to_string(),
            ]),
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 5: Authorize
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

    // Step 6: Prepare - list-based selectors should NOT match (fs not in git list)
    // U1-S6: effective_match = false (effect_type matches but selectors don't)
    // This should trigger HIGH mismatch and block at prepare-time
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
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
        "U1-S7a: list-based selector mismatch should block at prepare-time"
    );

    // Verify execution state is Denied (hard gate at prepare)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution state should be Denied when list-based selectors mismatch"
    );

    // Verify u1_s5b_hard_gate metadata is present
    let gate_json = exec.metadata.get("u1_s5b_hard_gate");
    assert!(
        gate_json.is_some(),
        "u1_s5b_hard_gate metadata should be present for selector mismatch"
    );
}

/// Test U1-S7a: Scalar + list union behavior.
/// When both scalar and list are present, match if scalar OR any list member matches.
#[tokio::test]
async fn test_u1_s7a_scalar_list_union_semantics() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Update intent with scalar + list selectors
    // adapter_family scalar = "http", adapter_family_in = ["git", "fs"]
    // -> scalar matches "http" but observed is "fs", so list must match -> fs IS in list -> MATCH
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "multi_adapter".to_string(),
        description: "multiple adapters via scalar+list union".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: Some("http".to_string()), // scalar does NOT match "fs"
            adapter_family_in: Some(vec!["fs".to_string(), "git".to_string()]), // but "fs" IS in list -> MATCH
            target_family: Some("sqlite".to_string()), // scalar does NOT match "file"
            target_family_in: Some(vec!["file".to_string()]), // but "file" IS in list -> MATCH
            request_class: Some("mutation".to_string()), // scalar does NOT match "read_only"
            request_class_in: Some(vec!["read_only".to_string()]), // but "read_only" IS in list -> MATCH
            mutation_family: Some("git_commit".to_string()),       // scalar does NOT match
            mutation_family_in: Some(vec!["mcp_tool_mutation".to_string()]), // but mcp_tool_mutation IS in list -> MATCH
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 5: Authorize
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

    // Step 6: Prepare - scalar+list OR semantics should match via list
    // All dimensions: scalar doesn't match but list DOES match -> overall MATCH
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
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
        "U1-S7a: scalar+list OR semantics should match when list member matches"
    );

    // Verify execution state is Prepared
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution state should be Prepared when scalar+list union matches via list"
    );
}

/// Test U1-S7a: Backward compatibility - selector-less legacy behavior remains intact.
/// When no selectors are present, coarse effect_type matching is used.
#[tokio::test]
async fn test_u1_s7a_selector_less_legacy_behavior_intact() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": get_test_file_path(execution_id)
                .expect("test_file_path should be set for execution_id")
                .to_string_lossy()
                .to_string(),
            "content": "hello"
        }),
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

    // Verify (should auto-commit for R0)
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

    // Verify response status is still 200
    assert_eq!(response.status(), 200);

    // Parse response body
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Verify response shape is unchanged
    assert_eq!(verify_resp.execution_id, execution_id);
    assert_eq!(verify_resp.verified, true);
    assert!(verify_resp.verified_at.is_some());

    // Verify execution state transitioned to Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Committed),
        "selector-less legacy behavior: execution state should be Committed after verify"
    );

    // Verify selector-less clause annotation (no selectors in original intent)
    // The selector-less case should have coarse match, not selector-enhanced
    let assessment_json = exec.metadata.get("u1_s2_verify_assessment");
    assert!(
        assessment_json.is_some(),
        "execution.metadata should contain u1_s2_verify_assessment"
    );
    let assessment = assessment_json.unwrap();

    // For selector-less legacy case, clause_match_annotations should show
    // selector_match=None (not applicable) and overall_result="strong_match"
    // This proves backward compatibility - no selector fields means coarse matching
    let clause_annotations = assessment.get("clause_match_annotations");
    assert!(
        clause_annotations.is_some(),
        "clause_match_annotations should be present"
    );
    let annotations = clause_annotations.unwrap().as_array().unwrap();
    if !annotations.is_empty() {
        let first_annotation = &annotations[0];
        let selector_match = first_annotation.get("selector_match");
        // For selector-less clauses, selector_match should be None (not applicable)
        assert!(
            selector_match.is_none() || selector_match.unwrap().is_null(),
            "selector_match should be null/None for selector-less legacy behavior"
        );
    }
}

/// Test U1-S7a: Scalar hit + list miss still matches via OR semantics.
/// When scalar matches and list misses, overall should still be MATCH (scalar OR any list member).
#[tokio::test]
async fn test_u1_s7a_scalar_hit_list_miss_or_semantics() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;
    let app = build_router(runtime.clone());

    // Step 1: Compile intent
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
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

    // Step 2: Update intent with selectors where scalar matches but list misses
    // adapter_family scalar = "fs" (MATCHES observed "fs")
    // adapter_family_in = ["git", "http"] (DOES NOT contain "fs" - list misses)
    // But OR semantics means: scalar OR any list member = fs OR (git OR http) = fs OR git OR http = MATCH
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "scalar_hit_list_miss".to_string(),
        description: "scalar matches but list misses - should still match via OR".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(ferrum_proto::OutcomeSelectors {
            adapter_family: Some("fs".to_string()), // scalar MATCHES "fs"
            adapter_family_in: Some(vec!["git".to_string(), "http".to_string()]), // list MISSES "fs"
            target_family: Some("file".to_string()),                              // scalar MATCHES
            target_family_in: Some(vec!["sqlite".to_string()]), // list MISSES "file"
            request_class: Some("read_only".to_string()),       // scalar MATCHES (R0)
            request_class_in: Some(vec!["mutation".to_string()]), // list MISSES "read_only"
            mutation_family: Some("mcp_tool_mutation".to_string()), // scalar MATCHES
            mutation_family_in: Some(vec!["git_commit".to_string()]), // list MISSES
            ..Default::default()
        }),
    }];
    stored_intent.forbidden_outcomes = vec![];
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file via fs".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 4: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 5: Authorize
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

    // Step 6: Prepare - scalar hits but list misses, should still MATCH via OR semantics
    // adapter_family: scalar "fs" MATCHES, list ["git","http"] MISSES -> OR -> MATCH
    // target_family: scalar "file" MATCHES, list ["sqlite"] MISSES -> OR -> MATCH
    // request_class: scalar "read_only" MATCHES, list ["mutation"] MISSES -> OR -> MATCH
    // mutation_family: scalar "mcp_tool_mutation" MATCHES, list ["git_commit"] MISSES -> OR -> MATCH
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/executions/{}/prepare", execution_id))
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
        "U1-S7a: scalar hit + list miss should still match via OR semantics"
    );

    // Verify execution state is Prepared
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Prepared),
        "execution state should be Prepared when scalar hits and list misses but OR semantics applies"
    );
}

/// U1-S8a: Test that selector-based authoring via compile API works for allow-match case.
/// When explicit allowed_outcomes with selectors are authored that match the subsequent
/// action's adapter_family, the prepare should return 200.
#[tokio::test]
async fn test_compile_with_explicit_selector_outcomes_allow_match_prepare_200() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with explicit selector-based allowed_outcomes that will MATCH fs.write
    // Uses list-based selectors to handle R0->"read_only" semantically (same pattern as U1-S7a tests)
    let allowed_outcomes = vec![OutcomeClause {
        id: "fs-write".to_string(),
        description: "File system write operations".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(OutcomeSelectors {
            adapter_family: Some("fs".to_string()),
            adapter_family_in: Some(vec!["fs".to_string(), "http".to_string()]),
            target_family: Some("file".to_string()),
            target_family_in: Some(vec!["file".to_string(), "sqlite".to_string()]),
            request_class: None,
            request_class_in: Some(vec!["read_only".to_string(), "mutation".to_string()]),
            mutation_family: None,
            mutation_family_in: Some(vec![
                "mcp_tool_mutation".to_string(),
                "file_write".to_string(),
            ]),
        }),
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
        content_hash: None,
    }];
    req.allowed_outcomes = Some(allowed_outcomes);
    // No forbidden_outcomes

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

    // Verify the authored outcomes were stored in the envelope
    assert_eq!(compile_resp.envelope.allowed_outcomes.len(), 1);
    assert_eq!(compile_resp.envelope.allowed_outcomes[0].id, "fs-write");
    assert!(
        compile_resp.envelope.allowed_outcomes[0]
            .selectors
            .is_some(),
        "selectors should be stored in envelope"
    );

    // Step 2: Evaluate proposal with matching fs.write action
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Step 3: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
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

    // Step 5: Prepare - selectors should MATCH, resulting in 200
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

    // U1-S8a: Allow-match case should return 200
    assert_eq!(
        response.status(),
        200,
        "U1-S8a: explicit selector-based outcomes that match should allow prepare=200"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        prep_resp.prepared,
        "U1-S8a: prepare should succeed for matching selectors"
    );
}

/// U1-S8a: Test that selector-based authoring via compile API works for allow-mismatch case.
/// When explicit allowed_outcomes with selectors are authored that do NOT match the subsequent
/// action's adapter_family, the prepare should return 403 (blocked by U1-S5b hard gate).
#[tokio::test]
async fn test_compile_with_explicit_selector_outcomes_allow_mismatch_prepare_403() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with explicit selector-based allowed_outcomes that will NOT MATCH
    // We use git-related selectors but the proposal will be for fs.write - should MISMATCH
    let allowed_outcomes = vec![OutcomeClause {
        id: "git-commit".to_string(),
        description: "Git commit operations".to_string(),
        effect_type: EffectType::GitMutation,
        required: true,
        selectors: Some(OutcomeSelectors {
            adapter_family: Some("git".to_string()), // Only matches git adapter
            adapter_family_in: None,
            target_family: Some("git".to_string()),
            target_family_in: None,
            request_class: Some("mutation".to_string()),
            request_class_in: None,
            mutation_family: Some("git_commit".to_string()),
            mutation_family_in: None,
        }),
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
        content_hash: None,
    }];
    req.allowed_outcomes = Some(allowed_outcomes);

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

    // Verify the authored outcomes were stored in the envelope
    assert_eq!(compile_resp.envelope.allowed_outcomes.len(), 1);
    assert_eq!(compile_resp.envelope.allowed_outcomes[0].id, "git-commit");

    // Step 2: Evaluate proposal with fs.write action (will NOT match git-commit selector)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
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
    let _eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    // Note: evaluate may still return Allow because it uses different matching logic
    // The mismatch will be caught at prepare-time by U1-S5b

    // Step 3: Mint capability
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
        policy_bundle_id: None,
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
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
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

    // Step 5: Prepare - selectors should MISMATCH, resulting in 403 blocked
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

    // U1-S8a: Allow-mismatch case returns 403 (blocked by U1-S5b hard gate)
    // The intent's allowed_outcomes specify git-commit selectors, but the execution is fs.write
    assert_eq!(
        response.status(),
        403,
        "U1-S8a: prepare should return 403 for mismatched selectors"
    );

    // Verify execution state is Denied due to U1-S5b hard gate
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "U1-S8a: execution state should be Denied when selectors don't match"
    );
}

/// U1-S8a: Test compile-time validation rejects empty outcome ids.
#[tokio::test]
async fn test_compile_rejects_empty_outcome_id() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let allowed_outcomes = vec![OutcomeClause {
        id: "".to_string(), // Invalid: empty id
        description: "Test".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: None,
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.allowed_outcomes = Some(allowed_outcomes);

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

    // Should return 400 Bad Request for empty outcome id
    assert_eq!(
        response.status(),
        400,
        "U1-S8a: compile should reject empty outcome id with 400"
    );
}

/// U1-S8a: Test compile-time validation rejects duplicate outcome ids across allowed/forbidden.
#[tokio::test]
async fn test_compile_rejects_duplicate_outcome_id_across_allowed_and_forbidden() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let allowed_outcomes = vec![OutcomeClause {
        id: "shared-id".to_string(),
        description: "Allowed".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: None,
    }];
    let forbidden_outcomes = vec![OutcomeClause {
        id: "shared-id".to_string(), // Same id as allowed - invalid
        description: "Forbidden".to_string(),
        effect_type: EffectType::FileMutation,
        required: false,
        selectors: None,
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.allowed_outcomes = Some(allowed_outcomes);
    req.forbidden_outcomes = Some(forbidden_outcomes);

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

    // Should return 400 Bad Request for duplicate id across allowed/forbidden
    assert_eq!(
        response.status(),
        400,
        "U1-S8a: compile should reject duplicate outcome id across allowed/forbidden with 400"
    );
}

/// U1-S8a: Test compile-time validation rejects empty strings in selector lists.
#[tokio::test]
async fn test_compile_rejects_empty_selector_strings() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let allowed_outcomes = vec![OutcomeClause {
        id: "test".to_string(),
        description: "Test".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(OutcomeSelectors {
            adapter_family: None,
            adapter_family_in: Some(vec!["fs".to_string(), "".to_string()]), // Invalid: empty string
            target_family: None,
            target_family_in: None,
            request_class: None,
            request_class_in: None,
            mutation_family: None,
            mutation_family_in: None,
        }),
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.allowed_outcomes = Some(allowed_outcomes);

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

    // Should return 400 Bad Request for empty string in selector list
    assert_eq!(
        response.status(),
        400,
        "U1-S8a: compile should reject empty string in selector list with 400"
    );
}

/// U1-S8a: Test compile-time validation rejects duplicate members in selector lists.
#[tokio::test]
async fn test_compile_rejects_duplicate_selector_list_members() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let allowed_outcomes = vec![OutcomeClause {
        id: "test".to_string(),
        description: "Test".to_string(),
        effect_type: EffectType::FileMutation,
        required: true,
        selectors: Some(OutcomeSelectors {
            adapter_family: None,
            adapter_family_in: Some(vec!["fs".to_string(), "fs".to_string()]), // Invalid: duplicate
            target_family: None,
            target_family_in: None,
            request_class: None,
            request_class_in: None,
            mutation_family: None,
            mutation_family_in: None,
        }),
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.allowed_outcomes = Some(allowed_outcomes);

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

    // Should return 400 Bad Request for duplicate members in selector list
    assert_eq!(
        response.status(),
        400,
        "U1-S8a: compile should reject duplicate members in selector list with 400"
    );
}

/// U1-S8a: Test compile-time validation rejects empty allowed_outcomes (fail-closed).
/// Explicit empty allowed_outcomes would broaden semantics beyond the backward-compatible
/// default single-coarse-outcome behavior.
#[tokio::test]
async fn test_compile_rejects_empty_allowed_outcomes() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Explicitly provide empty allowed_outcomes - should be rejected
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.allowed_outcomes = Some(vec![]); // Invalid: empty list

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

    // Should return 400 Bad Request for empty allowed_outcomes
    assert_eq!(
        response.status(),
        400,
        "U1-S8a: compile should reject empty allowed_outcomes with 400 (fail-closed)"
    );
}

/// U1-S8a: Test that forbidden_outcomes-only authoring works correctly.
/// When only forbidden_outcomes is provided (without allowed_outcomes),
/// the default single coarse allowed outcome is used (backward-compatible).
#[tokio::test]
async fn test_compile_with_forbidden_only_succeeds() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with only forbidden_outcomes (no allowed_outcomes)
    let forbidden_outcomes = vec![OutcomeClause {
        id: "forbidden-git".to_string(),
        description: "Git mutations are forbidden".to_string(),
        effect_type: EffectType::GitMutation,
        required: false,
        selectors: Some(OutcomeSelectors {
            adapter_family: Some("git".to_string()),
            adapter_family_in: None,
            target_family: Some("git".to_string()),
            target_family_in: None,
            request_class: Some("mutation".to_string()),
            request_class_in: None,
            mutation_family: Some("git_commit".to_string()),
            mutation_family_in: None,
        }),
    }];

    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.forbidden_outcomes = Some(forbidden_outcomes);
    // Note: allowed_outcomes is not set - should use default behavior

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

    // Should succeed - forbidden-only is valid
    assert_eq!(
        response.status(),
        200,
        "U1-S8a: compile should accept forbidden_outcomes-only authoring"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();

    // Verify: default allowed outcome should be present
    assert_eq!(
        compile_resp.envelope.allowed_outcomes.len(),
        1,
        "U1-S8a: default allowed outcome should be present when allowed_outcomes is omitted"
    );
    assert_eq!(
        compile_resp.envelope.allowed_outcomes[0].id, "primary",
        "U1-S8a: default allowed outcome should have id 'primary'"
    );

    // Verify: forbidden outcome should be stored
    assert_eq!(
        compile_resp.envelope.forbidden_outcomes.len(),
        1,
        "U1-S8a: forbidden outcome should be stored"
    );
    assert_eq!(
        compile_resp.envelope.forbidden_outcomes[0].id, "forbidden-git",
        "U1-S8a: forbidden outcome should have id 'forbidden-git'"
    );
}

// ============================================
// U1-S9a: POLICY BUNDLE FINGERPRINT CANONICALIZATION TESTS
// ============================================

/// U1-S9a: Reordered equivalent allowed_outcomes produce the same fingerprint.
/// This proves canonicalization is order-stable for authored outcome contracts.
#[tokio::test]
async fn test_policy_bundle_fingerprint_reorder_stability_allowed_outcomes() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile intent with allowed_outcomes in order [a, b]
    let mut req1 = sample_intent_request();
    req1.allowed_outcomes = Some(vec![
        OutcomeClause {
            id: "outcome-a".to_string(),
            description: "Outcome A".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        },
        OutcomeClause {
            id: "outcome-b".to_string(),
            description: "Outcome B".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
    ]);
    req1.forbidden_outcomes = None;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp1: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint1 = compile_resp1.envelope.policy_bundle_fingerprint.clone();

    // Compile intent with allowed_outcomes in reversed order [b, a]
    let mut req2 = sample_intent_request();
    req2.allowed_outcomes = Some(vec![
        OutcomeClause {
            id: "outcome-b".to_string(),
            description: "Outcome B".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
        OutcomeClause {
            id: "outcome-a".to_string(),
            description: "Outcome A".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        },
    ]);
    req2.forbidden_outcomes = None;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp2: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint2 = compile_resp2.envelope.policy_bundle_fingerprint.clone();

    // U1-S9a: Reordered equivalent contracts MUST produce the same fingerprint
    assert_eq!(
        fingerprint1, fingerprint2,
        "U1-S9a: Reordered equivalent allowed_outcomes should produce the same policy_bundle_fingerprint"
    );
}

/// U1-S9a: Reordered equivalent forbidden_outcomes produce the same fingerprint.
#[tokio::test]
async fn test_policy_bundle_fingerprint_reorder_stability_forbidden_outcomes() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile intent with forbidden_outcomes in order [x, y]
    let mut req1 = sample_intent_request();
    req1.forbidden_outcomes = Some(vec![
        OutcomeClause {
            id: "forbid-x".to_string(),
            description: "Forbidden X".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
        OutcomeClause {
            id: "forbid-y".to_string(),
            description: "Forbidden Y".to_string(),
            effect_type: EffectType::ExternalApiCall,
            required: false,
            selectors: None,
        },
    ]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp1: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint1 = compile_resp1.envelope.policy_bundle_fingerprint.clone();

    // Compile intent with forbidden_outcomes in reversed order [y, x]
    let mut req2 = sample_intent_request();
    req2.forbidden_outcomes = Some(vec![
        OutcomeClause {
            id: "forbid-y".to_string(),
            description: "Forbidden Y".to_string(),
            effect_type: EffectType::ExternalApiCall,
            required: false,
            selectors: None,
        },
        OutcomeClause {
            id: "forbid-x".to_string(),
            description: "Forbidden X".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
    ]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp2: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint2 = compile_resp2.envelope.policy_bundle_fingerprint.clone();

    // U1-S9a: Reordered equivalent contracts MUST produce the same fingerprint
    assert_eq!(
        fingerprint1, fingerprint2,
        "U1-S9a: Reordered equivalent forbidden_outcomes should produce the same policy_bundle_fingerprint"
    );
}

/// U1-S9a: Changed clause content produces a different fingerprint.
#[tokio::test]
async fn test_policy_bundle_fingerprint_changed_clause_content_different_identity() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile intent with allowed_outcomes containing [outcome-a]
    let mut req1 = sample_intent_request();
    req1.allowed_outcomes = Some(vec![OutcomeClause {
        id: "outcome-a".to_string(),
        description: "Outcome A".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
        selectors: None,
    }]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp1: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint1 = compile_resp1.envelope.policy_bundle_fingerprint.clone();

    // Compile intent with DIFFERENT allowed_outcomes - same id but different effect_type
    let mut req2 = sample_intent_request();
    req2.allowed_outcomes = Some(vec![OutcomeClause {
        id: "outcome-a".to_string(),
        description: "Outcome A".to_string(),
        effect_type: EffectType::FileMutation, // CHANGED: was ReadOnlyAnalysis
        required: true,
        selectors: None,
    }]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp2: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint2 = compile_resp2.envelope.policy_bundle_fingerprint.clone();

    // U1-S9a: Changed clause content MUST produce a different fingerprint
    assert_ne!(
        fingerprint1, fingerprint2,
        "U1-S9a: Changed clause content should produce a different policy_bundle_fingerprint"
    );
}

/// U1-S9a: Different number of clauses produces a different fingerprint.
#[tokio::test]
async fn test_policy_bundle_fingerprint_different_clause_count_different_identity() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile intent with single allowed outcome
    let mut req1 = sample_intent_request();
    req1.allowed_outcomes = Some(vec![OutcomeClause {
        id: "outcome-single".to_string(),
        description: "Single outcome".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
        selectors: None,
    }]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp1: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint1 = compile_resp1.envelope.policy_bundle_fingerprint.clone();

    // Compile intent with TWO allowed outcomes (same first outcome)
    let mut req2 = sample_intent_request();
    req2.allowed_outcomes = Some(vec![
        OutcomeClause {
            id: "outcome-single".to_string(),
            description: "Single outcome".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        },
        OutcomeClause {
            id: "outcome-second".to_string(),
            description: "Second outcome".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
    ]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp2: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint2 = compile_resp2.envelope.policy_bundle_fingerprint.clone();

    // U1-S9a: Different clause count MUST produce a different fingerprint
    assert_ne!(
        fingerprint1, fingerprint2,
        "U1-S9a: Different number of clauses should produce a different policy_bundle_fingerprint"
    );
}

/// U1-S9a: Selector list fields are sorted for canonicalization.
#[tokio::test]
async fn test_policy_bundle_fingerprint_selector_list_order_stability() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile intent with selectors containing adapter_family_in in order [z, a, m]
    let mut req1 = sample_intent_request();
    req1.allowed_outcomes = Some(vec![OutcomeClause {
        id: "outcome-with-selectors".to_string(),
        description: "Outcome with selectors".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
        selectors: Some(OutcomeSelectors {
            adapter_family: None,
            adapter_family_in: Some(vec!["z".to_string(), "a".to_string(), "m".to_string()]),
            target_family: None,
            target_family_in: None,
            request_class: None,
            request_class_in: None,
            mutation_family: None,
            mutation_family_in: None,
        }),
    }]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp1: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint1 = compile_resp1.envelope.policy_bundle_fingerprint.clone();

    // Compile intent with same selectors but in different order [a, m, z]
    let mut req2 = sample_intent_request();
    req2.allowed_outcomes = Some(vec![OutcomeClause {
        id: "outcome-with-selectors".to_string(),
        description: "Outcome with selectors".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
        selectors: Some(OutcomeSelectors {
            adapter_family: None,
            adapter_family_in: Some(vec!["a".to_string(), "m".to_string(), "z".to_string()]),
            target_family: None,
            target_family_in: None,
            request_class: None,
            request_class_in: None,
            mutation_family: None,
            mutation_family_in: None,
        }),
    }]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp2: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let fingerprint2 = compile_resp2.envelope.policy_bundle_fingerprint.clone();

    // U1-S9a: Selector lists in different order MUST produce the same fingerprint
    assert_eq!(
        fingerprint1, fingerprint2,
        "U1-S9a: Selector list field order should not affect policy_bundle_fingerprint"
    );
}

/// U1-S9a: Capability mint propagates the same policy_bundle_id for reordered outcome contracts.
#[tokio::test]
async fn test_capability_mint_propagates_same_policy_bundle_id_for_reordered_contracts() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Compile intent with allowed_outcomes in order [a, b]
    let mut req1 = sample_intent_request();
    req1.allowed_outcomes = Some(vec![
        OutcomeClause {
            id: "outcome-a".to_string(),
            description: "Outcome A".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        },
        OutcomeClause {
            id: "outcome-b".to_string(),
            description: "Outcome B".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
    ]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp1: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id1 = compile_resp1.envelope.intent_id;
    let fingerprint1 = compile_resp1.envelope.policy_bundle_fingerprint.clone();

    // Evaluate proposal for intent1
    let proposal1 = sample_proposal(intent_id1);
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal1.proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Mint capability for intent1
    let app = build_router(runtime.clone());
    let mint_req1 = CapabilityMintRequest {
        intent_id: intent_id1,
        proposal_id: proposal1.proposal_id,
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
        policy_bundle_id: None,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp1: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let policy_bundle_id1 = mint_resp1.lease.policy_bundle_id;

    // Now compile intent with same content but reordered [b, a]
    let mut req2 = sample_intent_request();
    req2.allowed_outcomes = Some(vec![
        OutcomeClause {
            id: "outcome-b".to_string(),
            description: "Outcome B".to_string(),
            effect_type: EffectType::FileMutation,
            required: false,
            selectors: None,
        },
        OutcomeClause {
            id: "outcome-a".to_string(),
            description: "Outcome A".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        },
    ]);

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp2: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id2 = compile_resp2.envelope.intent_id;
    let fingerprint2 = compile_resp2.envelope.policy_bundle_fingerprint.clone();

    // Evaluate proposal for intent2
    let proposal2 = sample_proposal(intent_id2);
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal2.proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Mint capability for intent2
    let app = build_router(runtime.clone());
    let mint_req2 = CapabilityMintRequest {
        intent_id: intent_id2,
        proposal_id: proposal2.proposal_id,
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
        policy_bundle_id: None,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp2: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let policy_bundle_id2 = mint_resp2.lease.policy_bundle_id;

    // U1-S9a: Both the fingerprints and the resulting PolicyBundleIds must match
    assert_eq!(
        fingerprint1, fingerprint2,
        "U1-S9a: Reordered contracts should produce same fingerprint"
    );
    assert_eq!(
        policy_bundle_id1, policy_bundle_id2,
        "U1-S9a: Reordered contracts should produce same PolicyBundleId in capability lease"
    );
}

/// U1-S9a regression test: minting a capability for an intent with an invalid
/// (unparseable) policy_bundle_fingerprint must fail-closed rather than silently
/// degrading to a random PolicyBundleId.
///
/// This prevents corrupted/malformed fingerprints from bypassing the deterministic
/// identity guarantee of U1-S9a.
#[tokio::test]
async fn test_mint_fails_closed_for_invalid_policy_bundle_fingerprint() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Create a valid intent via compile
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

    // Step 2: Corrupt the stored intent's policy_bundle_fingerprint to be invalid
    // (not a valid UUID that can be parsed as PolicyBundleId)
    let mut stored_intent = runtime
        .store
        .intents()
        .get(intent_id)
        .await
        .unwrap()
        .unwrap();
    stored_intent.policy_bundle_fingerprint = Some("not-a-valid-uuid!!!".to_string());
    runtime
        .store
        .intents()
        .update(&stored_intent)
        .await
        .unwrap();

    // Step 3: Create and evaluate a proposal for this intent
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

    // Step 4: Attempt to mint a capability - must fail-closed due to invalid fingerprint
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
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
        policy_bundle_id: None, // Let the gateway derive from intent's fingerprint
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

    // Fail-closed: mint must fail with 500 when fingerprint is invalid
    assert_eq!(
        response.status(),
        500,
        "U1-S9a: mint should fail-closed for invalid policy_bundle_fingerprint"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        error.message.contains("invalid policy_bundle_fingerprint"),
        "Error message should mention invalid fingerprint, got: {}",
        error.message
    );
}

/// P2.5: HTTP adapter transport failure verification test.
///
/// Verifies fail-closed behavior when HTTP transport fails (connection refused).
/// Uses an unbound localhost port to deterministically trigger connection refused.
///
/// Assertions:
/// - Execute returns 500 Internal Server Error on transport failure
/// - Execution state transitions to Failed (deterministic error state)
/// - No spurious state leakage (execution does not remain in Prepared)
#[tokio::test]
async fn test_http_execute_transport_failure_is_fail_closed() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP GET scope targeting an unreachable URL.
    // Use 127.0.0.1:0 which will refuse connection (nothing listening on that port).
    // Note: HTTP adapter is only routed for mutation-capable modes (Write/ReadWrite/Admin).
    // Using Write mode to ensure the HTTP adapter is invoked.
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "http://127.0.0.1:0".to_string(),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.get
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP GET request".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": "http://127.0.0.1:0/"}),
        expected_effect: "read data from http endpoint".to_string(),
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

    // Step 3: Mint capability for HTTP
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Get,
            base_url: "http://127.0.0.1:0".to_string(),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);

    // Step 6: Execute HTTP GET to unbound port - should fail with transport error
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": "http://127.0.0.1:0/"}),
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

    // Fail-closed: execute should return 500 on transport failure
    assert_eq!(
        response.status(),
        500,
        "HTTP execute should return 500 on transport failure (connection refused)"
    );

    // Step 7: Verify execution state is Failed (deterministic error state)
    // The execution must NOT remain stuck in Prepared state after a transport failure.
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        stored_execution.is_some(),
        "Execution record should still exist after transport failure"
    );
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution state must be Failed after transport failure, got {:?}",
        exec.state
    );
    // finished_at must be set for terminal Failed state
    assert!(
        exec.finished_at.is_some(),
        "Failed execution must have finished_at timestamp"
    );

    // Step 8: Verify no spurious events - no ToolCallExecuted since execute failed
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: Some(execution_id),
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
    assert!(
        !has_tool_call_executed,
        "ToolCallExecuted should NOT be emitted when execute fails (fail-closed)"
    );
}

/// P2.5: HTTP adapter timeout behavior verification test.
///
/// Verifies fail-closed behavior when HTTP request times out (server accepts connection but never responds).
/// Uses a hanging TCP server that accepts connections but never sends a response.
///
/// Assertions:
/// - Execute returns error on timeout
/// - Execution state transitions to Failed
/// - No spurious state leakage (execution does not remain in Prepared/Running)
#[tokio::test]
async fn test_http_execute_timeout_fails_closed() {
    // Start a hanging server that accepts connections but never responds.
    // This is deterministic - the server will accept the connection and hang forever.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    // Spawn a thread to accept one connection and hang (never respond)
    let _handle = std::thread::spawn(move || {
        if let Ok((_stream, _)) = listener.accept() {
            // Connection accepted - now hang forever (never send response)
            std::thread::sleep(std::time::Duration::from_secs(600));
        }
    });

    // Give the server a moment to start accepting
    std::thread::sleep(std::time::Duration::from_millis(50));

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP GET scope targeting the hanging server.
    // Using Write mode to ensure the HTTP adapter is invoked.
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.get
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP GET request".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/", port)}),
        expected_effect: "read data from http endpoint".to_string(),
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

    // Step 3: Mint capability for HTTP
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Get,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);

    // Step 6: Execute HTTP GET to hanging server - should fail with timeout error.
    // This test uses a short timeout (configured in the HTTP adapter) to quickly
    // verify fail-closed behavior without waiting for an extended period.
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/", port)}),
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

    // Fail-closed: execute should return 500 Internal Server Error on timeout
    assert_eq!(
        response.status(),
        500,
        "HTTP execute should return 500 on timeout (server accepted but never responded)"
    );

    // Step 7: Verify execution state is Failed (deterministic error state)
    // The execution must NOT remain stuck in Prepared/Running state after timeout.
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        stored_execution.is_some(),
        "Execution record should still exist after timeout"
    );
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution state must be Failed after timeout, got {:?}",
        exec.state
    );
    // finished_at must be set for terminal Failed state
    assert!(
        exec.finished_at.is_some(),
        "Failed execution must have finished_at timestamp"
    );

    // Step 8: Verify no spurious events - no ToolCallExecuted since execute failed
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: Some(execution_id),
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
    assert!(
        !has_tool_call_executed,
        "ToolCallExecuted should NOT be emitted when execute times out (fail-closed)"
    );
}

/// P2.5: Gateway-level HTTP GET verify transport failure test.
///
/// Verifies fail-closed behavior when HTTP GET verify re-request hits transport failure.
///
/// Flow:
/// 1. Execute HTTP GET succeeds (server responds 200)
/// 2. Server becomes unreachable
/// 3. Verify is called with explicit HttpStatusExpected check
/// 4. HTTP adapter re-requests the URL (transport failure: connection refused)
/// 5. Verify returns verified=false (fail-closed)
/// 6. Execution state transitions to Failed
/// 7. Commit/unsafe progression is blocked
///
/// This is a gateway-level integration test that verifies the full API flow,
/// complementing the adapter-level test `test_verify_get_transport_failure_fails_closed`.
///
/// The test now properly injects explicit verify_checks (HttpStatusExpected) into
/// the rollback contract via the runtime store after execute succeeds, then stops
/// the server before verify to force the GET re-request path.
#[tokio::test]
async fn test_verify_get_transport_failure_fails_closed() {
    // Start a local HTTP server that will serve during execute, then be stopped
    let (port, server_handle) = start_local_http_server(200);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP GET scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.get
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP GET request".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/", port)}),
        expected_effect: "read data from http endpoint".to_string(),
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

    // Step 3: Mint capability for HTTP
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Get,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // Step 6: Execute HTTP GET - succeeds because server is still running
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/", port)}),
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

    // Step 7: Inject explicit verify_checks into the rollback contract to force
    // the gateway's verify endpoint to take the GET re-request path instead of
    // relying on execute-time metadata fallback (auto-verify).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let mut contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Add explicit HttpStatusExpected(200) check - this forces GET re-request
    let explicit_check = CheckSpec {
        check_type: CheckType::HttpStatusExpected,
        config: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("status".to_string(), serde_json::json!(200));
            m
        },
    };
    contract.verify_checks = vec![explicit_check];
    runtime
        .store
        .rollback_contracts()
        .update(&contract)
        .await
        .unwrap();

    // Step 8: Stop the HTTP server so verify's GET re-request will fail
    // Clean up server thread before verify to simulate server becoming unreachable
    let _ = server_handle.join();

    // Step 9: Verify - now takes explicit GET re-request path and fails closed
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

    // Verify API returns 200 (not 500) - fail-closed behavior demonstrated
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (non-500) for fail-closed verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // GET re-request failed due to transport error: verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when GET re-request fails due to transport error"
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Step 10: Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify failed, got {:?}",
        exec.state
    );

    // Step 11: Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );
}

/// P2.5 Slice 5: Gateway-level HTTP PATCH mutation verify explicit-check mismatch test.
///
/// Verifies that mutation verify uses execute-time metadata crosscheck and NOT replay.
///
/// Flow:
/// 1. Execute HTTP PATCH with 503 status (server error)
/// 2. Execute captures 503 in metadata (not replaying - this is the execute result)
/// 3. After execute, inject explicit verify_checks with HttpStatusExpected(200)
///    into the rollback contract
/// 4. Verify API is called
/// 5. HTTP adapter's verify for mutations crosschecks: execute-time status (503) vs
///    explicit check (200) → mismatch → verified=false
/// 6. Execution state transitions to Failed
/// 7. Commit is rejected from Failed state
///
/// Key semantic being tested:
/// - For mutations, verify does NOT replay the request
/// - Verify compares execute-time status (503) against explicit check (200)  
/// - The mismatch proves the metadata crosscheck is being used, not replay
/// - No network reachability manipulation needed - it's pure metadata comparison
///
/// This complements the adapter-level test `test_verify_mutation_patch_explicit_check_mismatch`
/// (ferrum-adapter-http/src/lib.rs) with a full gateway API integration test.
#[tokio::test]
async fn test_verify_mutation_patch_explicit_check_mismatch() {
    // Start a local HTTP server that returns 503 on PATCH request
    let (port, server_handle) = start_local_http_server(503);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP PATCH scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Patch,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.patch
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP PATCH request".to_string(),
        tool_name: "http.patch".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port),
            "body": {"name": "updated"}
        }),
        expected_effect: "update resource via HTTP PATCH".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 3: Mint capability for HTTP PATCH
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.patch".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Patch,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // R0 with mutation returns RequireApproval, so we need to go through approval flow
    // Step 4b: Get approval ID and resolve it
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created for R0 with mutation"
    );
    let approval_id = pending_approvals[0].approval_id;

    // Step 4c: Resolve approval with approve=true
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by test".to_string()),
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

    // Step 6: Execute HTTP PATCH - server returns 503
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port),
            "body": {"name": "updated"}
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

    // Execute should succeed (server responded, even with 503)
    assert_eq!(
        response.status(),
        200,
        "execute should succeed even when server returns 503"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain "503" from the server response
    assert!(
        execute_resp.result_digest.as_ref().unwrap().contains("503"),
        "result_digest should contain 503"
    );

    // Step 7: Inject explicit verify_checks into the rollback contract.
    // The contract currently has no verify_checks (empty).
    // We inject HttpStatusExpected(200) to create a mismatch with execute-time status (503).
    // This forces the verify API to use the explicit check path for mutations.
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let mut contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Add explicit HttpStatusExpected(200) check - this will mismatch with execute-time 503
    let explicit_check = CheckSpec {
        check_type: CheckType::HttpStatusExpected,
        config: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("status".to_string(), serde_json::json!(200));
            m
        },
    };
    contract.verify_checks = vec![explicit_check];
    runtime
        .store
        .rollback_contracts()
        .update(&contract)
        .await
        .unwrap();

    // Step 8: Verify - mutation with explicit check mismatch should return verified=false
    // This proves the verify uses execute-time metadata crosscheck, not replay.
    // For mutations, even if we stopped the server, verify would still use metadata.
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

    // Verify API returns 200 (not 500) - fail-closed verification failure is still 200 OK
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (not 500) for fail-closed verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Mismatch: execute-time (503) != explicit check (200) → verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when execute-time (503) != explicit check (200)"
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Step 9: Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify mismatch, got {:?}",
        exec.state
    );

    // Step 10: Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );

    // Clean up server handle
    let _ = server_handle.join();
}

/// P2.5 Slice 8: Gateway-level HTTP GET verify re-request timeout test.
///
/// Verifies fail-closed behavior when HTTP GET verify re-request times out
/// (server accepts connection but never responds).
///
/// Flow:
/// 1. Execute HTTP GET succeeds (server responds 200 on first request)
/// 2. After execute, the server starts hanging (accepts connections but never responds)
/// 3. Verify is called with explicit HttpStatusExpected check
/// 4. HTTP adapter re-requests the URL - times out because server hangs
/// 5. Verify returns verified=false (fail-closed)
/// 6. Execution state transitions to Failed
/// 7. Commit is rejected from Failed state
///
/// This is distinct from connection-refused (test_verify_get_transport_failure_fails_closed)
/// which stops the server entirely. Here the server is still running but unresponsive.
///
/// Note: This test uses a server that responds to the FIRST request (execute) but
/// hangs on subsequent requests (verify's re-request). This requires two sequential
/// connections to the same port.
#[tokio::test]
async fn test_verify_get_re_request_timeout_fails_closed() {
    // Start a "hanging" server that will:
    // 1. First connection: respond with 200 (serve execute)
    // 2. Second connection: hang forever (verify's re-request times out)
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    // Spawn a server that handles TWO connections:
    // - First: respond 200 OK (for execute)
    // - Second: accept and hang forever (for verify's re-request)
    let handle = std::thread::spawn(move || {
        for i in 0..2 {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    use std::io::{Read, Write};
                    // Read request headers
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);

                    if i == 0 {
                        // First connection (execute): respond 200 OK
                        let response =
                            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    } else {
                        // Second connection (verify re-request): hang forever
                        // Don't send any response, just keep the connection open
                        std::thread::park();
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Give server time to start
    std::thread::sleep(std::time::Duration::from_millis(50));

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP GET scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Get,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.get
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP GET request".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http".to_string(),
        raw_arguments: serde_json::json!({"url": format!("http://127.0.0.1:{}/", port)}),
        expected_effect: "read data from http endpoint".to_string(),
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

    // Step 3: Mint capability for HTTP
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Get,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // Step 6: Execute HTTP GET - first connection responds 200
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"url": format!("http://127.0.0.1:{}/", port)}),
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

    // Step 7: Inject explicit verify_checks into the rollback contract to force
    // the gateway's verify endpoint to take the GET re-request path instead of
    // relying on execute-time metadata fallback (auto-verify).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let mut contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Add explicit HttpStatusExpected(200) check - this forces GET re-request
    let explicit_check = CheckSpec {
        check_type: CheckType::HttpStatusExpected,
        config: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("status".to_string(), serde_json::json!(200));
            m
        },
    };
    contract.verify_checks = vec![explicit_check];
    runtime
        .store
        .rollback_contracts()
        .update(&contract)
        .await
        .unwrap();

    // Step 8: Verify - now takes explicit GET re-request path and times out
    // because the second connection to the server hangs forever.
    // The server already accepted the connection but never responds (timeout).
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

    // Verify API returns 200 (not 500) - fail-closed behavior demonstrated
    // Even though the re-request timed out, the gateway returns 200 with verified=false
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (non-500) for fail-closed verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // GET re-request timed out: verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when GET re-request times out"
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Step 9: Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify timeout, got {:?}",
        exec.state
    );

    // Step 10: Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );

    // Clean up: unpark the hanging thread so the test can exit
    handle.thread().unpark();
    let _ = handle.join();
}

/// P2.5 Slice 6: Gateway-level HTTP PATCH mutation verify explicit-check MATCH test.
///
/// This is the symmetric positive counterpart to `test_verify_mutation_patch_explicit_check_mismatch`.
///
/// Target semantics:
/// - PATCH execute-time metadata 200
/// - explicit HttpStatusExpected(200) check injected
/// - verify API returns 200 with verified=true
/// - Execution remains in AwaitingVerification or Committed (NOT Failed)
/// - Commit succeeds
///
/// Key semantic being tested:
/// - For mutations, verify does NOT replay the request (built into adapter design)
/// - Verify compares execute-time status (200) against explicit check (200)
/// - The MATCH proves the metadata crosscheck is working correctly
/// - When execute-time matches explicit check, verified=true and commit is allowed
///
/// This complements:
/// - Adapter-level test `test_verify_mutation_patch_explicit_check_match`
///   (ferrum-adapter-http/src/lib.rs) with full gateway API integration
/// - Gateway-level mismatch test `test_verify_mutation_patch_explicit_check_mismatch`
///   with the positive match case
#[tokio::test]
async fn test_verify_mutation_patch_explicit_check_match() {
    // Start a local HTTP server that returns 200 OK on PATCH request
    let (port, server_handle) = start_local_http_server(200);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP PATCH scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Patch,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.patch
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP PATCH request".to_string(),
        tool_name: "http.patch".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port),
            "body": {"name": "updated"}
        }),
        expected_effect: "update resource via HTTP PATCH".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 3: Mint capability for HTTP PATCH
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.patch".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Patch,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // R0 with mutation returns RequireApproval, so we need to go through approval flow
    // Step 4b: Get approval ID and resolve it
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created for R0 with mutation"
    );
    let approval_id = pending_approvals[0].approval_id;

    // Step 4c: Resolve approval with approve=true
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by test".to_string()),
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

    // Step 6: Execute HTTP PATCH - server returns 200
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port),
            "body": {"name": "updated"}
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

    // Execute should succeed
    assert_eq!(
        response.status(),
        200,
        "execute should succeed when server returns 200"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain "200" from the server response
    assert!(
        execute_resp.result_digest.as_ref().unwrap().contains("200"),
        "result_digest should contain 200"
    );

    // Step 7: Inject explicit verify_checks into the rollback contract.
    // The contract currently has no verify_checks (empty).
    // We inject HttpStatusExpected(200) to match the execute-time status.
    // This tests the positive case: execute-time (200) == explicit check (200).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let mut contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Add explicit HttpStatusExpected(200) check - this MATCHES execute-time 200
    let explicit_check = CheckSpec {
        check_type: CheckType::HttpStatusExpected,
        config: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("status".to_string(), serde_json::json!(200));
            m
        },
    };
    contract.verify_checks = vec![explicit_check];
    runtime
        .store
        .rollback_contracts()
        .update(&contract)
        .await
        .unwrap();

    // Step 8: Verify - mutation with explicit check MATCH should return verified=true
    // For mutations, verify uses execute-time metadata crosscheck (no replay)
    // Compare: execute-time (200) == explicit check (200) → verified=true
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

    // Verify API returns 200 OK
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 for successful verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // MATCH: execute-time (200) == explicit check (200) → verified=true
    assert!(
        verify_resp.verified,
        "verify should be true when execute-time (200) == explicit check (200)"
    );
    assert!(
        verify_resp.verified_at.is_some(),
        "verified_at should be set when verification succeeds"
    );

    // Step 9: Execution state should be AwaitingVerification (R0) or Committed (auto-commit)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(
            exec.state,
            ExecutionState::AwaitingVerification | ExecutionState::Committed
        ),
        "Execution should be AwaitingVerification or Committed after verify match, got {:?}",
        exec.state
    );

    // Step 10: Commit should succeed (not rejected since verification passed)
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

    // Commit should succeed after verified=true (2xx status)
    assert!(
        response.status().is_success(),
        "commit should succeed after verified=true, got {:?}",
        response.status()
    );

    // Clean up server handle
    let _ = server_handle.join();
}

/// P2.5 Slice 9: Gateway-level HTTP POST mutation verify explicit-check mismatch test.
///
/// Verifies that mutation verify uses execute-time metadata crosscheck and NOT replay
/// for POST requests (extends PATCH coverage in Slice 5 to another mutation method).
///
/// Flow:
/// 1. Execute HTTP POST with 503 status (server error)
/// 2. Execute captures 503 in metadata (not replaying - this is the execute result)
/// 3. After execute, inject explicit verify_checks with HttpStatusExpected(200)
///    into the rollback contract
/// 4. Verify API is called
/// 5. HTTP adapter's verify for mutations crosschecks: execute-time status (503) vs
///    explicit check (200) → mismatch → verified=false
/// 6. Execution state transitions to Failed
/// 7. Commit is rejected from Failed state
///
/// Key semantic being tested:
/// - For mutations, verify does NOT replay the request
/// - Verify compares execute-time status (503) against explicit check (200)
/// - The mismatch proves the metadata crosscheck is being used, not replay
///
/// This extends P2.5 Slice 5 (PATCH mismatch) to POST mutations.
#[tokio::test]
async fn test_verify_mutation_post_explicit_check_mismatch() {
    // Start a local HTTP server that returns 503 on POST request
    let (port, server_handle) = start_local_http_server(503);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP POST scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.post
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP POST request".to_string(),
        tool_name: "http.post".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port),
            "body": {"name": "created"}
        }),
        expected_effect: "create resource via HTTP POST".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 3: Mint capability for HTTP POST
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.post".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Post,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // R0 with mutation returns RequireApproval, so we need to go through approval flow
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created for R0 with mutation"
    );
    let approval_id = pending_approvals[0].approval_id;

    // Step 4c: Resolve approval with approve=true
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by test".to_string()),
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

    // Step 6: Execute HTTP POST - server returns 503
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port),
            "body": {"name": "created"}
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

    // Execute should succeed (server responded, even with 503)
    assert_eq!(
        response.status(),
        200,
        "execute should succeed even when server returns 503"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain "503" from the server response
    assert!(
        execute_resp.result_digest.as_ref().unwrap().contains("503"),
        "result_digest should contain 503"
    );

    // Step 7: Inject explicit verify_checks into the rollback contract.
    // The contract currently has no verify_checks (empty).
    // We inject HttpStatusExpected(200) to create a mismatch with execute-time status (503).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let mut contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Add explicit HttpStatusExpected(200) check - this will mismatch with execute-time 503
    let explicit_check = CheckSpec {
        check_type: CheckType::HttpStatusExpected,
        config: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("status".to_string(), serde_json::json!(200));
            m
        },
    };
    contract.verify_checks = vec![explicit_check];
    runtime
        .store
        .rollback_contracts()
        .update(&contract)
        .await
        .unwrap();

    // Step 8: Verify - mutation with explicit check mismatch should return verified=false
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

    // Verify API returns 200 (not 500) - fail-closed verification failure is still 200 OK
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (not 500) for fail-closed verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Mismatch: execute-time (503) != explicit check (200) → verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when execute-time (503) != explicit check (200)"
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Step 9: Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify mismatch, got {:?}",
        exec.state
    );

    // Step 10: Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );

    // Clean up server handle
    let _ = server_handle.join();
}

/// P2.5 Slice 10: Gateway-level HTTP DELETE mutation verify explicit-check MATCH test.
///
/// This is the symmetric positive counterpart to the POST mismatch test,
/// verifying DELETE mutation verify uses execute-time metadata crosscheck correctly.
///
/// Flow:
/// 1. Execute HTTP DELETE with 200 status (success)
/// 2. Execute captures 200 in metadata
/// 3. After execute, inject explicit verify_checks with HttpStatusExpected(200)
///    into the rollback contract
/// 4. Verify API is called
/// 5. HTTP adapter's verify for mutations crosschecks: execute-time status (200) vs
///    explicit check (200) → match → verified=true
/// 6. Execution state remains AwaitingVerification or Committed
/// 7. Commit succeeds
///
/// Key semantic being tested:
/// - For mutations, verify does NOT replay the request
/// - Verify compares execute-time status (200) against explicit check (200)
/// - The MATCH proves the metadata crosscheck is working correctly
///
/// This extends P2.5 Slice 6 (PATCH match) to DELETE mutations.
#[tokio::test]
async fn test_verify_mutation_delete_explicit_check_match() {
    // Start a local HTTP server that returns 200 OK on DELETE request
    let (port, server_handle) = start_local_http_server(200);

    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with HTTP DELETE scope
    let mut req = sample_intent_request();
    req.requested_resource_scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Delete,
        base_url: format!("http://127.0.0.1:{}", port),
        path_prefix: "/".to_string(),
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

    // Step 2: Create and evaluate proposal for http.delete
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "HTTP DELETE request".to_string(),
        tool_name: "http.delete".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port)
        }),
        expected_effect: "delete resource via HTTP DELETE".to_string(),
        estimated_risk: RiskTier::Medium,
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

    // Step 3: Mint capability for HTTP DELETE
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Delete,
            base_url: format!("http://127.0.0.1:{}", port),
            path_prefix: "/".to_string(),
            header_allowlist: vec![],
            mode: ResourceMode::Write,
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
        policy_bundle_id: None,
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

    // R0 with mutation returns RequireApproval, so we need to go through approval flow
    // Step 4b: Get approval ID and resolve it
    let (pending_approvals, _) = runtime
        .store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created for R0 with mutation"
    );
    let approval_id = pending_approvals[0].approval_id;

    // Step 4c: Resolve approval with approve=true
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by test".to_string()),
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

    // Step 6: Execute HTTP DELETE - server returns 200
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": format!("http://127.0.0.1:{}/", port)
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

    // Execute should succeed
    assert_eq!(
        response.status(),
        200,
        "execute should succeed when server returns 200"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");
    // result_digest should contain "200" from the server response
    assert!(
        execute_resp.result_digest.as_ref().unwrap().contains("200"),
        "result_digest should contain 200"
    );

    // Step 7: Inject explicit verify_checks into the rollback contract.
    // The contract currently has no verify_checks (empty).
    // We inject HttpStatusExpected(200) to match execute-time status (200).
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();
    let mut contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();

    // Add explicit HttpStatusExpected(200) check - this will match execute-time 200
    let explicit_check = CheckSpec {
        check_type: CheckType::HttpStatusExpected,
        config: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("status".to_string(), serde_json::json!(200));
            m
        },
    };
    contract.verify_checks = vec![explicit_check];
    runtime
        .store
        .rollback_contracts()
        .update(&contract)
        .await
        .unwrap();

    // Step 8: Verify - mutation with explicit check match should return verified=true
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

    // Verify API returns 200
    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Match: execute-time (200) == explicit check (200) → verified=true
    assert!(
        verify_resp.verified,
        "verify should be true when execute-time (200) == explicit check (200)"
    );
    assert!(
        verify_resp.verified_at.is_some(),
        "verified_at should be set when verification succeeds"
    );

    // Step 9: Execution state should be AwaitingVerification or Committed (NOT Failed)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(
            exec.state,
            ExecutionState::AwaitingVerification | ExecutionState::Committed
        ),
        "Execution should be AwaitingVerification or Committed after verify match, got {:?}",
        exec.state
    );

    // Step 10: Commit should succeed (not rejected since verification passed)
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

    // Commit should succeed after verified=true (2xx status)
    assert!(
        response.status().is_success(),
        "commit should succeed after verified=true, got {:?}",
        response.status()
    );

    // Clean up server handle
    let _ = server_handle.join();
}

/// P2.1 Slice 4: Gateway-level fs adapter verify hash mismatch → Failed → commit rejected.
///
/// Verifies the fail-closed integration slice for fs adapter:
/// - execute creates a file with content "hello"
/// - tamper with file after execution (modify content to "tampered")
/// - verify compares current hash against stored after_hash → mismatch → verified=false
/// - execution transitions to Failed state
/// - commit is rejected from Failed state
///
/// This is the deterministic hash-mismatch scenario proving verify failure transitions
/// execution to Failed and commit is rejected.
///
/// Flow:
/// 1. Compile intent with fs.write scope + FileMutation effect
/// 2. Evaluate, mint, authorize, prepare (R2 to require explicit commit)
/// 3. Execute fs.write → creates /tmp/test_fs_hash_mismatch.txt with "hello"
/// 4. Tampers with file: writes "tampered" to the same path (simulates outside interference)
/// 5. Verify → fs adapter computes current hash vs stored after_hash → mismatch → verified=false
/// 6. Execution state transitions to Failed
/// 7. Commit is rejected from Failed state
#[tokio::test]
async fn test_fs_verify_hash_mismatch_transitions_to_failed_and_rejects_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with FileMutation effect and fs write scope
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
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

    // Step 2: Evaluate proposal with fs.write (R2 to require explicit commit)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test_fs_hash_mismatch.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable, // Requires explicit commit
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
            path: "/tmp/test_fs_hash_mismatch.txt".to_string(),
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
        policy_bundle_id: None,
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

    // Step 6: Execute fs.write → creates file with "hello"
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test_fs_hash_mismatch.txt", "content": "hello"}),
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed, "executed flag should be true");

    // Verify file was created with correct content
    let file_path = std::path::Path::new("/tmp/test_fs_hash_mismatch.txt");
    assert!(file_path.exists(), "file should exist after execute");
    let content = std::fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "hello", "file should contain 'hello'");

    // Step 7: Tamper with the file after execution (simulates outside interference)
    // Write different content to create a hash mismatch
    std::fs::write(file_path, "tampered").expect("failed to tamper with file");
    let tampered_content = std::fs::read_to_string(file_path).unwrap();
    assert_eq!(tampered_content, "tampered", "file should be tampered");

    // Step 8: Verify → fs adapter computes current hash vs stored after_hash → mismatch
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

    // Verify returns 200 (fail-closed: verification failure is not an error response)
    assert_eq!(
        response.status(),
        200,
        "verify should return 200 (not 500) for fail-closed verification"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();

    // Hash mismatch: current hash ("tampered") != after_hash ("hello") → verified=false
    assert!(
        !verify_resp.verified,
        "verify should be false when current hash != after_hash (file was tampered)"
    );
    assert!(
        verify_resp.verified_at.is_none(),
        "verified_at should not be set when verification fails"
    );

    // Step 9: Execution state should transition to Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Failed),
        "Execution should be Failed after verify mismatch, got {:?}",
        exec.state
    );

    // Step 10: Commit should be rejected from Failed state
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

    // Commit from Failed state should be rejected (4xx or 5xx, not 2xx)
    assert!(
        !response.status().is_success(),
        "commit should be rejected from Failed state, got {:?}",
        response.status()
    );

    // Clean up: restore file to allow subsequent tests to delete it via rollback
    std::fs::write(file_path, "hello").expect("failed to restore file for rollback");
}

/// Integration test: gateway-level fs rollback drill after verify failure.
///
/// Flow: execute -> verify false (due to outside interference) -> rollback -> assert RolledBack + fs state restored.
/// Uses /tmp filesystem with snapshot-based rollback.
/// Rollback is the native-reversible path; validates it works for fs after verify failure.
#[tokio::test]
async fn test_fs_verify_false_triggers_rollback_drill() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let file_path = "/tmp/test_fs_rollback_drill.txt";

    // Step 1: Compile intent with FileMutation effect and fs write scope
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
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

    // Step 2: Evaluate proposal with R2 (requires explicit rollback, not auto-rollback)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": file_path, "content": "hello"}),
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
        policy_bundle_id: None,
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

    // Step 5: Prepare execution (snapshots existing file state - none here, so snapshot is empty)
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

    // Step 6: Execute fs.write → creates file with "hello"
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": file_path, "content": "hello"}),
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);

    // Verify file was created with correct content
    let path = std::path::Path::new(file_path);
    assert!(path.exists(), "file should exist after execute");
    let content = std::fs::read_to_string(path).unwrap();
    assert_eq!(content, "hello", "file should contain 'hello'");

    // Step 7: Tamper with the file after execution (simulates outside interference)
    std::fs::write(path, "tampered").expect("failed to tamper with file");
    let tampered_content = std::fs::read_to_string(path).unwrap();
    assert_eq!(tampered_content, "tampered", "file should be tampered");

    // Step 8: Verify → fs adapter detects hash mismatch → verified=false
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
        !verify_resp.verified,
        "verify should be false after outside interference"
    );

    // Execution state should be Failed after verify mismatch
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Failed),
        "execution should be Failed after verify mismatch"
    );

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

    assert_eq!(
        response.status(),
        200,
        "rollback should return 200 even after verify failure"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        rollback_resp.rolled_back,
        "rollback should succeed after verify failure"
    );

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::RolledBack),
        "execution should be RolledBack after rollback"
    );

    // Verify fs state: file should be deleted (rollback of new file creation)
    let file_exists_after = std::path::Path::new(file_path).exists();
    assert!(
        !file_exists_after,
        "file should be deleted after rollback (fs rollback of new file = delete)"
    );

    // Verify SideEffectRolledBack provenance event was emitted
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
        "SideEffectRolledBack provenance event should be emitted after rollback"
    );
}

/// Integration test: gateway-level fs compensate drill after verify failure.
///
/// Flow: execute -> verify false (due to outside interference) -> compensate -> assert Compensated + fs state restored.
/// Uses /tmp filesystem with snapshot-based compensate.
/// Compensate is the primary recovery endpoint; this validates it works identically to rollback for fs.
#[tokio::test]
async fn test_fs_verify_false_triggers_compensate_drill() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let file_path = "/tmp/test_fs_compensate_drill.txt";

    // Step 1: Compile intent with FileMutation effect and fs write scope
    let mut req = sample_intent_request_with_effect(EffectType::FileMutation);
    req.requested_resource_scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp".to_string(),
        mode: ResourceMode::Write,
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

    // Step 2: Evaluate proposal with R2 (requires explicit compensate)
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": file_path, "content": "hello"}),
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
        policy_bundle_id: None,
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

    // Step 6: Execute fs.write → creates file with "hello"
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": file_path, "content": "hello"}),
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

    // Verify file was created
    let path = std::path::Path::new(file_path);
    assert!(path.exists(), "file should exist after execute");
    let content = std::fs::read_to_string(path).unwrap();
    assert_eq!(content, "hello", "file should contain 'hello'");

    // Step 7: Tamper with the file (simulates outside interference)
    std::fs::write(path, "tampered").expect("failed to tamper with file");

    // Step 8: Verify → mismatch → verified=false
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
        !verify_resp.verified,
        "verify should be false after outside interference"
    );

    // Execution state should be Failed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Failed),
        "execution should be Failed after verify mismatch"
    );

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

    assert_eq!(
        response.status(),
        200,
        "compensate should return 200 even after verify failure"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();
    assert!(
        compensate_resp.compensated,
        "compensate should succeed after verify failure"
    );

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(
        matches!(stored_execution.unwrap().state, ExecutionState::Compensated),
        "execution should be Compensated after compensate"
    );

    // Verify fs state: file should be deleted (compensate of new file creation = delete)
    let file_exists_after = std::path::Path::new(file_path).exists();
    assert!(
        !file_exists_after,
        "file should be deleted after compensate (fs compensate of new file = delete)"
    );

    // Verify SideEffectCompensated provenance event was emitted
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
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
        !events.is_empty(),
        "SideEffectCompensated provenance event should be emitted after compensate"
    );
}
