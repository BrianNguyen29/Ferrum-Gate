//! Integration tests for fs FileWrite round-trip lifecycle with persisted metadata.
//!
//! This file proves:
//! - prepare captures snapshot metadata (FsAdapter)
//! - contract can be persisted via SqliteRollbackRepo
//! - retrieved contract metadata can drive execute -> verify -> rollback successfully
//! - state transitions persist through the store lifecycle
//! - verify_checks are exercised during the verify phase
//! - compensate/rollback paths are exercised with non-empty compensation_plan

use ferrum_adapter_fs::FsAdapter;
use ferrum_proto::{
    ActionType, CheckSpec, CheckType, CompensationStep, ExecutionId, IntentId, ProposalId,
    RollbackClass, RollbackContract, RollbackContractId, RollbackPrepareRequest, RollbackState,
    RollbackTarget,
};
use ferrum_rollback::RollbackAdapter;
use ferrum_store::sqlite::SqliteRollbackRepo;
use ferrum_store::{RollbackRepo, SqliteStore};
use tempfile::tempdir;

/// Converts a serde_json::Map to a JsonMap (IndexMap), matching the pattern used in
/// ferrum-adapter-fs tests to avoid type mismatches between serde_json::Value and JsonMap.
fn json_map_from_serde_map(
    map: serde_json::Map<String, serde_json::Value>,
) -> ferrum_proto::JsonMap {
    map.into_iter().collect()
}

// Helper to insert required parent records for rollback_contract foreign keys
async fn insert_parent_records_via_sql(
    pool: &sqlx::SqlitePool,
    intent_id: &str,
    proposal_id: &str,
    execution_id: &str,
    capability_id: &str,
) -> ferrum_store::Result<()> {
    use chrono::Utc;

    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO intents (intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode, default_rollback_class, created_at, expires_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    )
    .bind(intent_id)
    .bind("test-principal")
    .bind("test goal")
    .bind("Active")
    .bind("Low")
    .bind("Auto")
    .bind("R1SnapshotRecoverable")
    .bind(&now)
    .bind(&now)
    .bind(r#"{}"#)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO proposals (proposal_id, intent_id, step_index, server_name, tool_name, estimated_risk, requested_rollback_class, created_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )
    .bind(proposal_id)
    .bind(intent_id)
    .bind(0)
    .bind("test-server")
    .bind("test-tool")
    .bind("Low")
    .bind("R1SnapshotRecoverable")
    .bind(&now)
    .bind(r#"{}"#)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO capabilities (capability_id, intent_id, proposal_id, server_name, tool_name, status, issued_at, expires_at, revoked_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    )
    .bind(capability_id)
    .bind(intent_id)
    .bind(proposal_id)
    .bind("test-server")
    .bind("test-tool")
    .bind("Granted")
    .bind(&now)
    .bind(&now)
    .bind(Option::<String>::None)
    .bind(r#"{}"#)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO executions (execution_id, intent_id, proposal_id, capability_id, rollback_contract_id, decision, state, started_at, finished_at, result_digest, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    )
    .bind(execution_id)
    .bind(intent_id)
    .bind(proposal_id)
    .bind(capability_id)
    .bind(Option::<String>::None)
    .bind("Approved")
    .bind("Completed")
    .bind(&now)
    .bind(&now)
    .bind(Option::<String>::None)
    .bind(r#"{}"#)
    .execute(pool)
    .await?;

    Ok(())
}

/// Helper to create a RollbackPrepareRequest for FileWrite
fn create_prepare_request(file_path: &str, execution_id: ExecutionId) -> RollbackPrepareRequest {
    RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path.to_string(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

/// Helper to build a RollbackContract from prepare receipt metadata
fn build_contract_from_prepare(
    request: &RollbackPrepareRequest,
    adapter_metadata: ferrum_proto::JsonMap,
) -> RollbackContract {
    RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: request.intent_id,
        proposal_id: request.proposal_id,
        execution_id: request.execution_id,
        action_type: request.action_type.clone(),
        rollback_class: request.rollback_class.clone(),
        adapter_key: request.adapter_key.clone(),
        target: request.target.clone(),
        prepare_checks: request.prepare_checks.clone(),
        verify_checks: request.verify_checks.clone(),
        compensation_plan: request.compensation_plan.clone(),
        auto_commit: request.auto_commit,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: adapter_metadata,
    }
}

// =============================================================================
// Q2 FileWrite fs-first slice: prepare -> persist -> retrieve -> execute -> verify -> rollback
// =============================================================================

#[tokio::test]
async fn test_fs_filewrite_prepare_persist_retrieve_execute_verify_rollback() {
    // This test proves the Q2 fs-first slice:
    // 1. FsAdapter::prepare captures snapshot metadata
    // 2. Contract is persisted via SqliteRollbackRepo
    // 3. Retrieved contract metadata can drive execute -> verify -> rollback

    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("roundtrip_test.txt");
    let file_path_str = file_path.display().to_string();

    // Create original file content
    std::fs::write(&file_path, b"original content").unwrap();
    let original_content = b"original content".as_slice();

    // --- Step 1: Prepare (FsAdapter captures snapshot) ---
    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();
    let request = create_prepare_request(&file_path_str, execution_id);

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");
    assert!(prep_receipt.accepted, "prepare should be accepted");

    // Verify snapshot_path is in metadata (proves prepare captured the snapshot)
    let snapshot_path = prep_receipt
        .adapter_metadata
        .get("snapshot_path")
        .expect("snapshot_path should be in prepare metadata")
        .as_str()
        .expect("snapshot_path should be a string");
    assert!(
        std::path::Path::new(snapshot_path).exists(),
        "snapshot should exist at {}",
        snapshot_path
    );

    // Verify snapshot contains original content
    let snapshot_content = std::fs::read(snapshot_path).expect("read snapshot");
    assert_eq!(
        snapshot_content.as_slice(),
        original_content,
        "snapshot should contain original content"
    );

    // --- Step 2: Build contract and persist to store ---
    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    let intent_id = request.intent_id;
    let proposal_id = request.proposal_id;
    let capability_id = ferrum_proto::CapabilityId::new();

    insert_parent_records_via_sql(
        store.pool(),
        &intent_id.to_string(),
        &proposal_id.to_string(),
        &execution_id.to_string(),
        &capability_id.to_string(),
    )
    .await
    .expect("insert parent records");

    let contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata.clone());
    let contract_id = contract.contract_id;

    // Persist contract to store
    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    // --- Step 3: Retrieve contract from store ---
    let retrieved = repo
        .get(contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");

    // Verify metadata survived the round-trip through SQLite
    let retrieved_snapshot_path = retrieved
        .metadata
        .get("snapshot_path")
        .expect("snapshot_path should be in retrieved metadata")
        .as_str()
        .expect("snapshot_path should be a string");
    assert_eq!(
        retrieved_snapshot_path, snapshot_path,
        "snapshot_path should match after store round-trip"
    );

    // --- Step 4: Execute (write new content) ---
    let _exec_receipt = adapter
        .execute(&retrieved, &serde_json::json!("new content"))
        .await
        .expect("execute should succeed");

    // Verify new content was written
    let new_file_content = std::fs::read(&file_path).expect("read file after execute");
    assert_eq!(new_file_content.as_slice(), b"new content");

    // --- Step 5: Verify ---
    let verify_receipt = adapter
        .verify(&retrieved)
        .await
        .expect("verify should succeed");
    assert!(verify_receipt.verified, "verify should pass");

    // --- Step 6: Rollback (restore from snapshot) ---
    let rollback_receipt = adapter
        .rollback(&retrieved)
        .await
        .expect("rollback should succeed");
    assert!(rollback_receipt.recovered, "rollback should succeed");

    // Verify file content was restored to original
    let restored_content = std::fs::read(&file_path).expect("read file after rollback");
    assert_eq!(
        restored_content.as_slice(),
        original_content,
        "file should be restored to original content after rollback"
    );
}

/// Test that verifies the metadata survives store persistence for a NEW file create (no snapshot).
#[tokio::test]
async fn test_fs_filewrite_newfile_metadata_persists_through_store() {
    // For new file creation, prepare() sets created_new_file=true instead of snapshot_path.
    // This test proves that metadata round-trips correctly for the new-file case.

    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("newfile_test.txt");
    let file_path_str = file_path.display().to_string();

    // Ensure file does NOT exist (new file case)
    assert!(!file_path.exists(), "test file should not exist initially");

    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();
    let request = create_prepare_request(&file_path_str, execution_id);

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");
    assert!(prep_receipt.accepted, "prepare should be accepted");

    // For new file, metadata should have created_new_file=true (no snapshot)
    let created_new_file = prep_receipt
        .adapter_metadata
        .get("created_new_file")
        .expect("created_new_file should be in metadata for new file")
        .as_bool()
        .expect("created_new_file should be a bool");
    assert!(
        created_new_file,
        "created_new_file should be true for non-existent file"
    );

    // --- Persist to store and retrieve ---
    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    let intent_id = request.intent_id;
    let proposal_id = request.proposal_id;
    let capability_id = ferrum_proto::CapabilityId::new();

    insert_parent_records_via_sql(
        store.pool(),
        &intent_id.to_string(),
        &proposal_id.to_string(),
        &execution_id.to_string(),
        &capability_id.to_string(),
    )
    .await
    .expect("insert parent records");

    let contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata.clone());
    let contract_id = contract.contract_id;

    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    let retrieved = repo
        .get(contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");

    // Verify created_new_file survived the round-trip
    let retrieved_created_new_file = retrieved
        .metadata
        .get("created_new_file")
        .expect("created_new_file should be in retrieved metadata")
        .as_bool()
        .expect("created_new_file should be a bool");
    assert!(
        retrieved_created_new_file,
        "created_new_file should be true after store round-trip"
    );

    // --- Execute creates the file ---
    let _exec_receipt = adapter
        .execute(&retrieved, &serde_json::json!("new file content"))
        .await
        .expect("execute should succeed for new file");

    // Verify file was created
    assert!(file_path.exists(), "file should exist after execute");
    let file_content = std::fs::read(&file_path).expect("read new file");
    assert_eq!(file_content.as_slice(), b"new file content");

    // --- Verify ---
    let verify_receipt = adapter
        .verify(&retrieved)
        .await
        .expect("verify should succeed");
    assert!(verify_receipt.verified);

    // --- Rollback should delete the new file (idempotent delete) ---
    let rollback_receipt = adapter
        .rollback(&retrieved)
        .await
        .expect("rollback should succeed");
    assert!(
        rollback_receipt.recovered,
        "rollback should succeed for new file delete"
    );

    // File should no longer exist after rollback
    assert!(
        !file_path.exists(),
        "file should be deleted after rollback for new file"
    );
}

// =============================================================================
// Q2 pre-gateway verification coverage:
// - State transitions via update_state on persisted contracts
// - verify_checks exercised during the verify phase
// - compensation_plan exercised via compensate() path
// =============================================================================

/// Helper: compute a hex SHA-256 hash of file contents for verify_checks.
/// Replicates the logic from FsAdapter::compute_file_hash to avoid adding sha2 dep.
fn compute_file_hash(path: &std::path::Path) -> String {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path).expect("open file");
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).expect("read file");
    let mut hasher = sha2::Sha256::new();
    Digest::update(&mut hasher, &contents);
    hex::encode(hasher.finalize())
}

/// Helper: build a request with verify_checks and compensation_plan pre-loaded.
fn create_prepare_request_with_checks(
    file_path: &str,
    execution_id: ExecutionId,
    verify_checks: Vec<CheckSpec>,
    compensation_plan: Vec<CompensationStep>,
) -> RollbackPrepareRequest {
    RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path.to_string(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks,
        compensation_plan,
        auto_commit: false,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

/// Test that verify_checks are exercised during verify phase.
///
/// We construct a contract with a FileHashMatches verify_check targeting the
/// post-execute content, execute writes new content, then verify runs the check.
#[tokio::test]
async fn test_fs_filewrite_verify_checks_are_exercised() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_checks_test.txt");
    let file_path_str = file_path.display().to_string();

    // Create original file
    std::fs::write(&file_path, b"original content").unwrap();

    // Build a FileHashMatches check expecting the post-execute content
    let post_execute_hash = {
        std::fs::write(&file_path, b"new content").unwrap();
        compute_file_hash(&file_path)
    };

    // Restore original for prepare
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();

    let verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": file_path_str,
                "expected_hash": post_execute_hash,
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let request = create_prepare_request_with_checks(
        &file_path_str,
        execution_id,
        verify_checks,
        vec![], // no compensation plan needed here
    );

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");
    assert!(prep_receipt.accepted, "prepare should be accepted");

    // Persist contract
    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    insert_parent_records_via_sql(
        store.pool(),
        &request.intent_id.to_string(),
        &request.proposal_id.to_string(),
        &execution_id.to_string(),
        &ferrum_proto::CapabilityId::new().to_string(),
    )
    .await
    .expect("insert parent records");

    let contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata);
    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    // Retrieve and execute (write new content matching the verify_check hash)
    let retrieved = repo
        .get(contract.contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");

    let _exec_receipt = adapter
        .execute(&retrieved, &serde_json::json!("new content"))
        .await
        .expect("execute should succeed");

    // Verify runs the FileHashMatches check against post-execute content
    let verify_receipt = adapter
        .verify(&retrieved)
        .await
        .expect("verify should succeed");
    assert!(
        verify_receipt.verified,
        "verify should pass with matching hash"
    );
}

/// Test that state transitions survive through the store lifecycle.
///
/// We insert a contract in Prepared state, then update its state to
/// ExecutedAwaitingVerify via the repo, retrieve it, and verify the state persisted.
#[tokio::test]
async fn test_fs_filewrite_state_transitions_persist_through_store() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("state_transition_test.txt");
    let file_path_str = file_path.display().to_string();

    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();
    let request = create_prepare_request(&file_path_str, execution_id);

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");
    assert!(prep_receipt.accepted);

    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    insert_parent_records_via_sql(
        store.pool(),
        &request.intent_id.to_string(),
        &request.proposal_id.to_string(),
        &execution_id.to_string(),
        &ferrum_proto::CapabilityId::new().to_string(),
    )
    .await
    .expect("insert parent records");

    let contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata);
    let contract_id = contract.contract_id;

    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    // Transition to ExecutedAwaitingVerify using update() which properly persists raw_json
    let mut contract_v2 = contract.clone();
    contract_v2.state = RollbackState::ExecutedAwaitingVerify;
    repo.update(&contract_v2)
        .await
        .expect("update should succeed");

    // Retrieve and confirm state is ExecutedAwaitingVerify
    let retrieved = repo
        .get(contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");
    assert_eq!(
        retrieved.state,
        RollbackState::ExecutedAwaitingVerify,
        "state should be ExecutedAwaitingVerify after transition"
    );

    // Transition again to Verified using update()
    let mut contract_v3 = retrieved.clone();
    contract_v3.state = RollbackState::Verified;
    repo.update(&contract_v3)
        .await
        .expect("update should succeed");

    let retrieved2 = repo
        .get(contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");
    assert_eq!(
        retrieved2.state,
        RollbackState::Verified,
        "state should be Verified after second transition"
    );
}

/// Test that a meaningful compensation_plan is exercised via the compensate path.
///
/// We build a compensation_plan with a single step targeting the adapter key "fs"
/// and operation "rollback", then invoke compensate() and verify the file is restored.
#[tokio::test]
async fn test_fs_filewrite_compensation_plan_exercises_rollback() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("compensation_plan_test.txt");
    let file_path_str = file_path.display().to_string();

    // Create original file content
    std::fs::write(&file_path, b"original content").unwrap();
    let original_content = b"original content".as_slice();

    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();
    let request = create_prepare_request(&file_path_str, execution_id);

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");
    assert!(prep_receipt.accepted);

    // Build a meaningful compensation_plan
    let compensation_plan = vec![CompensationStep {
        order: 1,
        adapter_key: "fs".to_string(),
        operation: "rollback".to_string(),
        args: json_map_from_serde_map(
            serde_json::json!({
                "target_path": file_path_str,
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        idempotency_key: "compensation-1".to_string(),
    }];

    let request_with_plan =
        create_prepare_request_with_checks(&file_path_str, execution_id, vec![], compensation_plan);
    // Build contract with the compensation_plan (create_prepare_request_with_checks
    // uses request.verify_checks/compensation_plan which we override below)

    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    insert_parent_records_via_sql(
        store.pool(),
        &request.intent_id.to_string(),
        &request.proposal_id.to_string(),
        &execution_id.to_string(),
        &ferrum_proto::CapabilityId::new().to_string(),
    )
    .await
    .expect("insert parent records");

    // Build contract with verify_checks and compensation_plan
    let mut contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata.clone());
    contract.verify_checks = request_with_plan.verify_checks.clone();
    contract.compensation_plan = request_with_plan.compensation_plan.clone();

    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    let retrieved = repo
        .get(contract.contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");

    // Execute writes new content
    adapter
        .execute(&retrieved, &serde_json::json!("new content"))
        .await
        .expect("execute should succeed");

    let new_content = std::fs::read(&file_path).expect("read file after execute");
    assert_eq!(new_content.as_slice(), b"new content");

    // compensate() should invoke rollback (since compensate delegates to rollback)
    let compensate_receipt = adapter
        .compensate(&retrieved)
        .await
        .expect("compensate should succeed");
    assert!(
        compensate_receipt.recovered,
        "compensate should succeed and restore file"
    );

    let restored_content = std::fs::read(&file_path).expect("read file after compensate");
    assert_eq!(
        restored_content.as_slice(),
        original_content,
        "file should be restored to original content after compensate"
    );
}

/// Test that compensation_plan is persisted and retrieved correctly from the store.
#[tokio::test]
async fn test_fs_filewrite_compensation_plan_persists_through_store() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("comp_plan_persist_test.txt");
    let file_path_str = file_path.display().to_string();

    std::fs::write(&file_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();

    let compensation_plan = vec![
        CompensationStep {
            order: 1,
            adapter_key: "fs".to_string(),
            operation: "rollback".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({
                    "target_path": file_path_str,
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            idempotency_key: "step-1".to_string(),
        },
        CompensationStep {
            order: 2,
            adapter_key: "fs".to_string(),
            operation: "rollback".to_string(),
            args: json_map_from_serde_map(
                serde_json::json!({
                    "target_path": file_path_str,
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            idempotency_key: "step-2".to_string(),
        },
    ];

    let request =
        create_prepare_request_with_checks(&file_path_str, execution_id, vec![], compensation_plan);

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");

    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    insert_parent_records_via_sql(
        store.pool(),
        &request.intent_id.to_string(),
        &request.proposal_id.to_string(),
        &execution_id.to_string(),
        &ferrum_proto::CapabilityId::new().to_string(),
    )
    .await
    .expect("insert parent records");

    let contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata);
    let contract_id = contract.contract_id;

    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    let retrieved = repo
        .get(contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");

    assert_eq!(
        retrieved.compensation_plan.len(),
        2,
        "compensation_plan should have 2 steps after round-trip"
    );
    assert_eq!(retrieved.compensation_plan[0].order, 1);
    assert_eq!(retrieved.compensation_plan[1].order, 2);
    assert_eq!(retrieved.compensation_plan[0].idempotency_key, "step-1");
}

/// Test that verify_checks survive through store round-trip and are exercised post-retrieval.
#[tokio::test]
async fn test_fs_filewrite_verify_checks_persist_and_are_exercised() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_checks_persist_test.txt");
    let file_path_str = file_path.display().to_string();

    std::fs::write(&file_path, b"original").unwrap();

    // Write the expected post-execute content and compute its hash
    std::fs::write(&file_path, b"verified content").unwrap();
    let expected_hash = compute_file_hash(&file_path);

    // Restore original for prepare
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let execution_id = ExecutionId::new();

    let verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": file_path_str,
                "expected_hash": expected_hash,
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let request =
        create_prepare_request_with_checks(&file_path_str, execution_id, verify_checks, vec![]);

    let prep_receipt = adapter
        .prepare(&request)
        .await
        .expect("prepare should succeed");

    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    insert_parent_records_via_sql(
        store.pool(),
        &request.intent_id.to_string(),
        &request.proposal_id.to_string(),
        &execution_id.to_string(),
        &ferrum_proto::CapabilityId::new().to_string(),
    )
    .await
    .expect("insert parent records");

    let contract = build_contract_from_prepare(&request, prep_receipt.adapter_metadata);
    let contract_id = contract.contract_id;

    let repo: SqliteRollbackRepo = store.rollback_contracts();
    repo.insert(&contract).await.expect("insert contract");

    // Retrieve, execute, then verify
    let retrieved = repo
        .get(contract_id)
        .await
        .expect("get contract")
        .expect("contract should exist");

    adapter
        .execute(&retrieved, &serde_json::json!("verified content"))
        .await
        .expect("execute should succeed");

    let verify_receipt = adapter
        .verify(&retrieved)
        .await
        .expect("verify should succeed");
    assert!(
        verify_receipt.verified,
        "verify with persisted FileHashMatches check should pass"
    );
}
