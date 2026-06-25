/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

use super::*;
use ferrum_proto::{
    CheckSpec, ExecutionId, IntentId, ProposalId, RollbackContractId, RollbackState,
};
use tempfile::tempdir;

fn create_test_contract(file_path: &str) -> RollbackContract {
    RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
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
        state: RollbackState::Prepared,
        created_at: Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(),
    }
}

fn create_test_request(file_path: &str) -> RollbackPrepareRequest {
    RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
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
        metadata: JsonMap::new(),
    }
}

#[tokio::test]
async fn test_prepare_accepts_valid_file_path() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let file_path_str = file_path.display().to_string();

    // Create the file first
    std::fs::write(&file_path, b"hello world").unwrap();

    let adapter = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);
    let receipt = adapter.prepare(&request).await.unwrap();
    assert!(receipt.accepted);
}

#[tokio::test]
async fn test_prepare_fails_on_nonexistent_file_path() {
    let adapter = FsAdapter::new("fs");
    let request = create_test_request("/nonexistent/path/to/file.txt");
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_prepare_fails_on_unsupported_action_type() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"hello").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.action_type = ActionType::SqlMutation; // Not supported for fs
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_prepare_with_file_exists_check() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("exists.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"hello").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &file_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let receipt = adapter.prepare(&request).await.unwrap();
    assert!(receipt.accepted);
}

#[tokio::test]
async fn test_prepare_with_file_hash_matches_check() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("hash_test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"hello world").unwrap();

    // Compute the expected hash
    let contents = std::fs::read(&file_path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let expected_hash = hex::encode(hasher.finalize());

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &file_path_str,
                "expected_hash": &expected_hash
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let receipt = adapter.prepare(&request).await.unwrap();
    assert!(receipt.accepted);
}

#[tokio::test]
async fn test_prepare_fails_with_wrong_hash() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("wrong_hash.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"hello world").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &file_path_str,
                "expected_hash": "wronghashvalue123"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_verify_with_file_exists_check() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_exists.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"verify me").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&file_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &file_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let receipt = adapter.verify(&contract).await.unwrap();
    assert!(receipt.verified);
}

#[tokio::test]
async fn test_verify_fails_when_file_missing() {
    let adapter = FsAdapter::new("fs");
    let contract = create_test_contract("/nonexistent/file.txt");
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_verify_with_hash_matches() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_hash.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"content to verify").unwrap();

    // Compute the expected hash
    let contents = std::fs::read(&file_path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let expected_hash = hex::encode(hasher.finalize());

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&file_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &file_path_str,
                "expected_hash": &expected_hash
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let receipt = adapter.verify(&contract).await.unwrap();
    assert!(receipt.verified);
}

#[tokio::test]
async fn test_verify_fails_on_hash_mismatch() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("hash_mismatch.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&file_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &file_path_str,
                "expected_hash": "aabbccdd"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_returns_unsupported() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("execute_test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let contract = create_test_contract(&file_path_str);
    let result = adapter.execute(&contract, &serde_json::json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rollback_returns_unsupported() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("rollback_test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let contract = create_test_contract(&file_path_str);
    let result = adapter.rollback(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_compensate_returns_unsupported() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("compensate_test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let contract = create_test_contract(&file_path_str);
    let result = adapter.compensate(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_prepare_with_invalid_target_type() {
    let adapter = FsAdapter::new("fs");
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::SqliteTxn {
            db_path: "test.db".to_string(),
            tx_id: "tx".to_string(),
        }, // Wrong target type
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_prepare_with_unsupported_check_type() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("unsupported_check.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::GitRefMatches, // Not supported for fs
        config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_verify_with_unsupported_check_type() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_unsupported.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&file_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::HttpStatusExpected, // Not supported
        config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

// =============================================================================
// FileWrite Snapshot/Recovery Slice Tests
// =============================================================================

#[tokio::test]
async fn test_prepare_captures_snapshot_for_existing_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("existing.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);
    let receipt = adapter.prepare(&request).await.unwrap();

    assert!(receipt.accepted);
    // Snapshot path should be in metadata
    let snapshot_path = receipt
        .adapter_metadata
        .get("snapshot_path")
        .expect("snapshot_path should be in metadata")
        .as_str()
        .expect("snapshot_path should be a string");
    let original_path = receipt
        .adapter_metadata
        .get("original_path")
        .expect("original_path should be in metadata")
        .as_str()
        .expect("original_path should be a string");

    assert_eq!(original_path, file_path_str);
    // Snapshot should exist and contain original content
    assert!(Path::new(snapshot_path).exists());
    let snapshot_content = std::fs::read(snapshot_path).unwrap();
    assert_eq!(snapshot_content, b"original content");
}

#[tokio::test]
async fn test_execute_writes_new_content_to_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("execute_target.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // First prepare (this will snapshot the original)
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Build contract with the prepare receipt metadata
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id, // Use same execution_id
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute with new content
    let exec_receipt = adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    assert!(exec_receipt.result_digest.is_some());
    // File should now have new content
    let new_content = std::fs::read(&file_path).unwrap();
    assert_eq!(new_content, b"new content");
}

#[tokio::test]
async fn test_rollback_restores_overwritten_contents() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("rollback_restore.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare (captures snapshot)
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Execute with different content (simulating the write operation)
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata.clone(),
    };

    // Execute changes
    adapter
        .execute(&contract, &serde_json::json!("modified content"))
        .await
        .unwrap();

    // Verify file was modified
    assert_eq!(std::fs::read(&file_path).unwrap(), b"modified content");

    // Now rollback
    let rollback_receipt = adapter.rollback(&contract).await.unwrap();
    assert!(rollback_receipt.recovered);

    // File should be restored to original content
    let restored_content = std::fs::read(&file_path).unwrap();
    assert_eq!(restored_content, b"original content");
}

#[tokio::test]
async fn test_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("compensate_alias.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata.clone(),
    };

    // Execute changes
    adapter
        .execute(&contract, &serde_json::json!("changed"))
        .await
        .unwrap();

    // Compensate should restore (alias for rollback)
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);

    // File should be restored
    assert_eq!(std::fs::read(&file_path).unwrap(), b"original");
}

#[tokio::test]
async fn test_rollback_fail_closed_when_snapshot_missing() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("missing_snapshot.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Create a contract with empty metadata (no snapshot)
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(), // Different execution_id = no snapshot
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str,
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(), // Empty metadata = no snapshot_path
    };

    let result = adapter.rollback(&contract).await;
    assert!(result.is_err());
    // Should fail with error about snapshot not found (path computed but file doesn't exist)
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(msg.contains("snapshot") || msg.contains("not found"));
        }
        _ => panic!("expected validation error for missing snapshot"),
    }
}

#[tokio::test]
async fn test_compensate_restores_deleted_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("delete_target.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare (captures snapshot)
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute deletes the file
    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!file_path.exists());

    // Compensate should restore the file
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);
    assert!(file_path.exists());
    assert_eq!(std::fs::read(&file_path).unwrap(), b"to be deleted");
}

#[tokio::test]
async fn test_rollback_restores_deleted_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("rollback_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare (captures snapshot)
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute deletes the file
    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!file_path.exists());

    // Rollback should restore the file
    let rollback_receipt = adapter.rollback(&contract).await.unwrap();
    assert!(rollback_receipt.recovered);
    assert!(file_path.exists());
    assert_eq!(std::fs::read(&file_path).unwrap(), b"to be deleted");
}

#[tokio::test]
async fn test_file_delete_rollback_fail_closed_when_snapshot_missing() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("missing_snapshot_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Contract with different execution_id = no snapshot
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(), // Different execution_id = no snapshot
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str,
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(),
    };

    // Rollback for FileDelete should fail when snapshot is missing
    let result = adapter.rollback(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(msg.contains("snapshot") || msg.contains("not found"));
        }
        _ => panic!("expected validation error for missing snapshot"),
    }
}

#[tokio::test]
async fn test_prepare_does_not_snapshot_new_file() {
    // Test that prepare on a non-existing file in a directory that exists
    // does NOT fail (new file creation is now supported when parent exists)
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("new_file.txt");
    let file_path_str = file_path.display().to_string();
    // Note: file does NOT exist, but parent directory (temp_dir) does

    let adapter = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);

    // Should succeed - new file creation with parent existing is now supported
    let result = adapter.prepare(&request).await;
    assert!(
        result.is_ok(),
        "Expected prepare to succeed for new file with parent existing"
    );
    let receipt = result.unwrap();
    assert!(receipt.accepted);
    // Should mark this as a new file creation (no snapshot needed)
    let created_new_file = receipt
        .adapter_metadata
        .get("created_new_file")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        created_new_file,
        "Expected created_new_file to be true for new file creation"
    );
}

#[tokio::test]
async fn test_execute_with_object_payload() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("object_payload.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute with object payload containing "content" field
    adapter
        .execute(
            &contract,
            &serde_json::json!({ "content": "content from object" }),
        )
        .await
        .unwrap();

    let content = std::fs::read(&file_path).unwrap();
    assert_eq!(content, b"content from object");
}

#[tokio::test]
async fn test_rollback_works_across_adapter_instances() {
    // Test that rollback works when prepare and rollback are called on different adapter instances.
    // This verifies the snapshot path is derived deterministically from contract fields,
    // not from per-instance state.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("cross_instance.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    // Instance A: prepare
    let adapter_a = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    // Build contract with the prepare receipt metadata
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id, // Same execution_id
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Instance A: execute changes
    adapter_a
        .execute(&contract, &serde_json::json!("modified content"))
        .await
        .unwrap();

    // Verify file was modified
    assert_eq!(std::fs::read(&file_path).unwrap(), b"modified content");

    // Instance B: rollback (different adapter instance)
    let adapter_b = FsAdapter::new("fs");
    let rollback_receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(rollback_receipt.recovered);

    // File should be restored to original content despite using different adapter instance
    let restored_content = std::fs::read(&file_path).unwrap();
    assert_eq!(restored_content, b"original content");
}

#[tokio::test]
async fn test_rollback_works_without_metadata_persistence() {
    // Test that rollback works even when contract has no adapter_metadata.
    // Snapshot path is derived from contract.execution_id + target.path.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("no_metadata.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    // Instance A: prepare
    let adapter_a = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);
    adapter_a.prepare(&request).await.unwrap();

    // Build contract with empty metadata (simulating recovered contract without metadata)
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id, // Same execution_id enables snapshot lookup
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(), // Empty metadata - no snapshot_path stored
    };

    // Instance A: execute changes
    adapter_a
        .execute(&contract, &serde_json::json!("modified content"))
        .await
        .unwrap();

    // Verify file was modified
    assert_eq!(std::fs::read(&file_path).unwrap(), b"modified content");

    // Instance B: rollback with no metadata - should still work via deterministic path
    let adapter_b = FsAdapter::new("fs");
    let rollback_receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(rollback_receipt.recovered);

    // File should be restored to original content
    let restored_content = std::fs::read(&file_path).unwrap();
    assert_eq!(restored_content, b"original content");
}

// =============================================================================
// FileDelete Snapshot/Recovery Slice Tests
// =============================================================================

#[tokio::test]
async fn test_prepare_captures_snapshot_for_file_delete() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("delete_me.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"content to delete").unwrap();

    let adapter = FsAdapter::new("fs");
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let receipt = adapter.prepare(&request).await.unwrap();

    assert!(receipt.accepted);
    // Snapshot path should be in metadata
    let snapshot_path = receipt
        .adapter_metadata
        .get("snapshot_path")
        .expect("snapshot_path should be in metadata")
        .as_str()
        .expect("snapshot_path should be a string");
    let original_path = receipt
        .adapter_metadata
        .get("original_path")
        .expect("original_path should be in metadata")
        .as_str()
        .expect("original_path should be a string");

    assert_eq!(original_path, file_path_str);
    // Snapshot should exist and contain original content
    assert!(Path::new(snapshot_path).exists());
    let snapshot_content = std::fs::read(snapshot_path).unwrap();
    assert_eq!(snapshot_content, b"content to delete");
}

#[tokio::test]
async fn test_execute_deletes_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("execute_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare (captures snapshot)
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute with empty payload (delete doesn't need content)
    let exec_receipt = adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();

    // File should be deleted
    assert!(!file_path.exists());
    assert!(exec_receipt.adapter_metadata.get("deleted_path").is_some());
}

#[tokio::test]
async fn test_prepare_fails_closed_for_nonexistent_file_delete() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("nonexistent_delete.txt");
    let file_path_str = file_path.display().to_string();
    // Note: file does NOT exist

    let adapter = FsAdapter::new("fs");
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };

    // Should fail because file doesn't exist (fail-closed for FileDelete)
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            // The error message should contain the path we tried to validate
            assert!(
                msg.contains(&file_path_str)
                    || msg.contains("not found")
                    || msg.contains("not a file")
            );
        }
        _ => panic!("expected validation error for missing file"),
    }
}

#[tokio::test]
async fn test_file_delete_rollback_works_across_adapter_instances() {
    // Test that rollback works when prepare and rollback are called on different adapter instances.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("cross_instance_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    // Instance A: prepare
    let adapter_a = FsAdapter::new("fs");
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Instance A: execute delete
    adapter_a
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!file_path.exists());

    // Instance B: rollback (different adapter instance)
    let adapter_b = FsAdapter::new("fs");
    let rollback_receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(rollback_receipt.recovered);

    // File should be restored despite using different adapter instance
    assert!(file_path.exists());
    let restored_content = std::fs::read(&file_path).unwrap();
    assert_eq!(restored_content, b"original content");
}

// =============================================================================
// FileDelete Verify Semantics Tests (P2.1 edge-case slice)
// =============================================================================

#[tokio::test]
async fn test_file_delete_verify_success_after_execute() {
    // Verify succeeds when file has been deleted (absent is correct post-execute state)
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_after_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute deletes the file
    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!file_path.exists());

    // Verify should succeed because file is correctly absent after delete
    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_file_delete_verify_fails_when_file_still_exists() {
    // Verify fails when file still exists (e.g., recreated by external process)
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_fails_if_exists.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Simulate external process recreating the file (or delete failed silently)
    // Don't execute delete - just prepare then check that verify would fail
    // because the file still exists when it shouldn't
    assert!(file_path.exists());

    // Verify should fail because file exists but should not (no execute called)
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(msg.contains("still exists") || msg.contains("not found"));
        }
        _ => panic!("expected validation error for file still existing after delete"),
    }
}

#[tokio::test]
async fn test_file_write_verify_still_checks_presence() {
    // Verify succeeds when file exists after FileWrite execute
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_write_presence.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute writes new content
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Verify should succeed because file exists after write
    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_file_write_verify_fails_if_file_missing() {
    // Verify fails when file does not exist after FileWrite (should have been written)
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_write_missing.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Note: do NOT execute - file still exists from setup
    // But the contract is for FileWrite, so before verify after execute, it should pass
    // If we call verify before execute, it should fail because file doesn't match post-execute expectation
    // Actually the issue is we're checking "verify fails if file missing" - this tests the opposite
    // The point is: FileWrite verify checks file EXISTS

    // For this test, let's manually delete the file to simulate a failed write
    std::fs::remove_file(&file_path).unwrap();

    // Verify should fail because file doesn't exist (post-write state should have file)
    let result = adapter.verify(&contract).await;
    assert!(result.is_err(), "Expected verify to fail for missing file");
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            // Validation error for missing file - msg is the path
            assert!(
                msg.contains("/tmp"),
                "Expected validation error to contain path"
            );
        }
        _ => panic!("expected validation error for missing file after write"),
    }
}

#[tokio::test]
async fn test_file_delete_verify_with_explicit_check_still_runs() {
    // Explicit verify_checks should still run and be fail-closed
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("explicit_check_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        // Explicit FileExists check - but file should be absent after delete
        // So this explicit check should FAIL
        verify_checks: vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &file_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute deletes the file
    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!file_path.exists());

    // Verify with explicit FileExists check should fail because file was deleted
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    // This is correct behavior - explicit checks are fail-closed
}

// =============================================================================
// FileWrite New-File Creation Slice Tests (P2.1)
// =============================================================================

#[tokio::test]
async fn test_prepare_accepts_new_file_when_parent_exists() {
    // Test that prepare accepts a non-existing file when parent directory exists
    let temp_dir = tempdir().unwrap();
    let sub_dir = temp_dir.path().join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();
    let file_path = sub_dir.join("new_file.txt");
    let file_path_str = file_path.display().to_string();
    // Note: file does NOT exist, but parent directory does

    let adapter = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);

    let receipt = adapter.prepare(&request).await.unwrap();
    assert!(receipt.accepted);
    // Should mark this as a new file creation
    let created_new_file = receipt
        .adapter_metadata
        .get("created_new_file")
        .expect("created_new_file should be in metadata")
        .as_bool()
        .expect("created_new_file should be a bool");
    assert!(created_new_file);
    // No snapshot_path for new file
    assert!(receipt.adapter_metadata.get("snapshot_path").is_none());
}

#[tokio::test]
async fn test_prepare_rejects_new_file_when_parent_missing() {
    // Test that prepare fails closed when parent directory is missing
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("nonexistent_parent/new_file.txt");
    let file_path_str = file_path.display().to_string();
    // Note: neither the file NOR the parent directory exists

    let adapter = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("parent") || msg.contains("does not exist"),
                "Expected parent directory error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing parent directory"),
    }
}

// =============================================================================
// FileDelete Explicit Check Path Mismatch Regression Test (P2.1)
// =============================================================================

#[tokio::test]
async fn test_file_delete_prepare_check_path_mismatch() {
    // FileDelete: explicit check with path mismatch should fail at prepare.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    std::fs::write(&target_path, b"target content").unwrap();
    std::fs::write(&wrong_path, b"wrong content").unwrap();

    let adapter = FsAdapter::new("fs");
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &wrong_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

// =============================================================================
// Explicit Check Path Mismatch Tests (P2.1 hardening)
// =============================================================================

#[tokio::test]
async fn test_prepare_fileexists_check_path_mismatch() {
    // FileExists check with path different from contract target should fail at prepare.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    std::fs::write(&target_path, b"target content").unwrap();
    std::fs::write(&wrong_path, b"wrong content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    // FileExists check targeting a DIFFERENT path should fail
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &wrong_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

#[tokio::test]
async fn test_prepare_filehashmatches_check_path_mismatch() {
    // FileHashMatches check with path different from contract target should fail at prepare.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    std::fs::write(&target_path, b"target content").unwrap();
    std::fs::write(&wrong_path, b"wrong content").unwrap();

    let mut hasher = Sha256::new();
    hasher.update(b"wrong content");
    let wrong_hash = hex::encode(hasher.finalize());

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    // FileHashMatches check targeting a DIFFERENT path should fail
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &wrong_path_str,
                "expected_hash": &wrong_hash
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

#[tokio::test]
async fn test_verify_fileexists_check_path_mismatch() {
    // FileExists check with path different from contract target should fail at verify.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    std::fs::write(&target_path, b"target content").unwrap();
    std::fs::write(&wrong_path, b"wrong content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&target_path_str);
    // FileExists check targeting a DIFFERENT path should fail
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &wrong_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

#[tokio::test]
async fn test_verify_filehashmatches_check_path_mismatch() {
    // FileHashMatches check with path different from contract target should fail at verify.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    std::fs::write(&target_path, b"target content").unwrap();
    std::fs::write(&wrong_path, b"wrong content").unwrap();

    let mut hasher = Sha256::new();
    hasher.update(b"wrong content");
    let wrong_hash = hex::encode(hasher.finalize());

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&target_path_str);
    // FileHashMatches check targeting a DIFFERENT path should fail
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &wrong_path_str,
                "expected_hash": &wrong_hash
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

// =============================================================================
// Malformed Check Config Validation Tests (P2.1 ergonomics)
// =============================================================================

#[tokio::test]
async fn test_prepare_fileexists_missing_path() {
    // FileExists check missing 'path' field should give clear error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("requires 'path'"),
                "Expected 'requires path' error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing path"),
    }
}

#[tokio::test]
async fn test_prepare_fileexists_non_string_path() {
    // FileExists check with non-string 'path' should give clear type error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": 123 })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("must be a string"),
                "Expected type error for path, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for non-string path"),
    }
}

#[tokio::test]
async fn test_prepare_filehashmatches_missing_path() {
    // FileHashMatches check missing 'path' field should give clear error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "expected_hash": "abc123" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("requires 'path'"),
                "Expected 'requires path' error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing path"),
    }
}

#[tokio::test]
async fn test_prepare_filehashmatches_non_string_path() {
    // FileHashMatches check with non-string 'path' should give clear type error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": ["array", "not", "string"], "expected_hash": "abc123" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("must be a string"),
                "Expected type error for path, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for non-string path"),
    }
}

#[tokio::test]
async fn test_prepare_filehashmatches_missing_expected_hash() {
    // FileHashMatches check missing 'expected_hash' field should give clear error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &target_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("requires 'expected_hash'"),
                "Expected 'requires expected_hash' error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing expected_hash"),
    }
}

#[tokio::test]
async fn test_prepare_filehashmatches_non_string_expected_hash() {
    // FileHashMatches check with non-string 'expected_hash' should give clear type error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &target_path_str, "expected_hash": 123456 })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("must be a string"),
                "Expected type error for expected_hash, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for non-string expected_hash"),
    }
}

#[tokio::test]
async fn test_verify_fileexists_non_string_path() {
    // FileExists check with non-string 'path' at verify should give clear type error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&target_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": { "object": "not string" } })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("must be a string"),
                "Expected type error for path, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for non-string path"),
    }
}

#[tokio::test]
async fn test_verify_filehashmatches_non_string_expected_hash() {
    // FileHashMatches check with non-string 'expected_hash' at verify should give clear type error.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&target_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &target_path_str, "expected_hash": true })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("must be a string"),
                "Expected type error for expected_hash, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for non-string expected_hash"),
    }
}

// =============================================================================
// Phase-Aware Error Message Tests (P2.1 validation ergonomics)
// =============================================================================

#[tokio::test]
async fn test_prepare_unsupported_check_error_mentions_prepare_phase() {
    // Unsupported check at prepare should mention "prepare" in error message.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::GitRefMatches, // Not supported for fs
        config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Unsupported(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in unsupported check error, got: {}",
                msg
            );
            assert!(
                msg.contains("GitRefMatches"),
                "Expected check type in error, got: {}",
                msg
            );
        }
        _ => panic!("expected unsupported error for unsupported check type"),
    }
}

#[tokio::test]
async fn test_verify_unsupported_check_error_mentions_verify_phase() {
    // Unsupported check at verify should mention "verify" in error message.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&file_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::HttpStatusExpected, // Not supported
        config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Unsupported(msg) => {
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in unsupported check error, got: {}",
                msg
            );
            assert!(
                msg.contains("HttpStatusExpected"),
                "Expected check type in error, got: {}",
                msg
            );
        }
        _ => panic!("expected unsupported error for unsupported check type"),
    }
}

#[tokio::test]
async fn test_prepare_malformed_check_error_mentions_prepare_phase_and_check_type() {
    // Malformed FileExists check at prepare should mention phase and check type.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": 123 }) // non-string
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileExists"),
                "Expected check type in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("must be a string"),
                "Expected type error in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for malformed check"),
    }
}

#[tokio::test]
async fn test_verify_malformed_check_error_mentions_verify_phase_and_check_type() {
    // Malformed FileHashMatches check at verify should mention phase and check type.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&target_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &target_path_str, "expected_hash": true }) // non-string hash
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileHashMatches"),
                "Expected check type in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("must be a string"),
                "Expected type error in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for malformed check"),
    }
}

#[tokio::test]
async fn test_prepare_missing_hash_field_error_mentions_phase_and_check_type() {
    // FileHashMatches missing 'expected_hash' at prepare should mention phase and check type.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &target_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileHashMatches"),
                "Expected check type in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("requires 'expected_hash'"),
                "Expected missing field error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing field"),
    }
}

#[tokio::test]
async fn test_verify_missing_path_field_error_mentions_phase_and_check_type() {
    // FileExists missing 'path' at verify should mention phase and check type.
    let temp_dir = tempdir().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let target_path_str = target_path.display().to_string();
    std::fs::write(&target_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut contract = create_test_contract(&target_path_str);
    contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
    }];

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileExists"),
                "Expected check type in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("requires 'path'"),
                "Expected missing field error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing field"),
    }
}

// =============================================================================
// New-File FileWrite with Explicit Check Path Mismatch Tests (P2.1)
// =============================================================================

#[tokio::test]
async fn test_new_file_write_prepare_check_path_mismatch() {
    // New-file FileWrite: explicit check with path mismatch should fail at prepare.
    let temp_dir = tempdir().unwrap();
    let sub_dir = temp_dir.path().join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();
    let target_path = sub_dir.join("new_file.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    // Note: target_path does NOT exist yet (new file case)

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_path_str);
    // FileExists check with WRONG path should fail
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &wrong_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

#[tokio::test]
async fn test_new_file_write_verify_check_path_mismatch_after_execute() {
    // New-file FileWrite: after execute, verify with path mismatch check should fail.
    let temp_dir = tempdir().unwrap();
    let sub_dir = temp_dir.path().join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();
    let target_path = sub_dir.join("new_file.txt");
    let target_path_str = target_path.display().to_string();
    let wrong_path = temp_dir.path().join("wrong.txt");
    let wrong_path_str = wrong_path.display().to_string();
    std::fs::write(&wrong_path, b"wrong content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare (new file case)
    let request = create_test_request(&target_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &wrong_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute creates the file
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Verify should FAIL because check targets wrong path (path mismatch)
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("path mismatch"),
                "Expected path mismatch error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for path mismatch"),
    }
}

// =============================================================================
// Phase-Aware FS/Internal Error Normalization Tests (P2.1)
// =============================================================================

#[tokio::test]
async fn test_prepare_fileexists_check_file_not_found_shows_phase() {
    // FileExists check where file is missing should show [prepare] phase context.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("nonexistent.txt");
    let file_path_str = file_path.display().to_string();
    // Note: file does NOT exist

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileExists,
        config: json_map_from_serde_map(
            serde_json::json!({ "path": &file_path_str })
                .as_object()
                .unwrap()
                .clone(),
        ),
    }];

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileExists"),
                "Expected 'FileExists' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("file not found"),
                "Expected 'file not found' in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing file"),
    }
}

#[tokio::test]
async fn test_verify_fileexists_check_file_not_found_shows_phase() {
    // FileExists check at verify where file is missing shows [verify] phase context.
    // Note: For FileWrite verify, the default validation runs FIRST (file must exist),
    // so the explicit FileExists check is secondary. If default validation passes
    // (file exists), the explicit check would also pass. This test verifies the
    // default validation error is phase-aware when file is missing after execute.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_missing.txt");
    let file_path_str = file_path.display().to_string();
    // File exists initially
    std::fs::write(&file_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &file_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute writes new content
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Delete the file to make verify fail
    std::fs::remove_file(&file_path).unwrap();

    // Verify should FAIL because file is missing (default validation runs before explicit checks)
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            // The error comes from default validation (phase-aware) since it runs first
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileWrite target file not found"),
                "Expected 'FileWrite target file not found' in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing file"),
    }
}

#[tokio::test]
async fn test_prepare_filehashmatches_io_error_shows_phase() {
    // FileHashMatches check where file becomes unreadable should show [prepare] phase context.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("hash_io_error.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"content").unwrap();

    // Compute a valid hash
    let contents = std::fs::read(&file_path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let expected_hash = hex::encode(hasher.finalize());

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&file_path_str);
    request.prepare_checks = vec![CheckSpec {
        check_type: CheckType::FileHashMatches,
        config: json_map_from_serde_map(
            serde_json::json!({
                "path": &file_path_str,
                "expected_hash": &expected_hash
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    }];

    // Remove file permissions to cause IO error on read
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    let result = adapter.prepare(&request).await;

    // Restore permissions for cleanup (in case test assertion fails)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&file_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Internal(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileHashMatches"),
                "Expected 'FileHashMatches' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains("failed to read/compute hash"),
                "Expected 'failed to read/compute hash' in internal error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected internal error for IO failure during hash compute, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_verify_filehashmatches_io_error_shows_phase() {
    // FileHashMatches check at verify where file becomes unreadable should show [verify] phase context.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_hash_io.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Compute a valid hash
    let contents = std::fs::read(&file_path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let expected_hash = hex::encode(hasher.finalize());

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &file_path_str,
                    "expected_hash": &expected_hash
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute writes new content
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Remove file permissions to cause IO error on read
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    // Verify should FAIL because file is unreadable
    let result = adapter.verify(&contract).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&file_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Internal(msg) => {
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileHashMatches"),
                "Expected 'FileHashMatches' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains("failed to read/compute hash"),
                "Expected 'failed to read/compute hash' in internal error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected internal error for IO failure during hash compute, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_prepare_default_validation_file_not_found_shows_phase() {
    // FileDelete with no prepare_checks where file is missing should show [prepare] phase.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("nonexistent_delete_phase.txt");
    let file_path_str = file_path.display().to_string();
    // Note: file does NOT exist

    let adapter = FsAdapter::new("fs");
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![], // No explicit checks - uses default validation
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileDelete target file not found"),
                "Expected 'FileDelete target file not found' in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing file"),
    }
}

#[tokio::test]
async fn test_verify_default_validation_file_not_found_shows_phase() {
    // FileWrite verify where file is missing should show [verify] phase.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_default_missing.txt");
    let file_path_str = file_path.display().to_string();
    // File exists initially
    std::fs::write(&file_path, b"content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![], // No explicit checks - uses default validation
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute writes new content
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Delete the file to make default validation fail
    std::fs::remove_file(&file_path).unwrap();

    // Verify should FAIL because file doesn't exist (default validation for FileWrite)
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileWrite target file not found"),
                "Expected 'FileWrite target file not found' in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing file"),
    }
}

#[tokio::test]
async fn test_verify_default_validation_file_still_exists_shows_phase() {
    // FileDelete verify where file still exists should show [verify] phase.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("verify_still_exists.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![], // No explicit checks - uses default validation
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // NOTE: We do NOT execute delete - file still exists

    // Verify should FAIL because file still exists after delete
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[verify]"),
                "Expected '[verify]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("file still exists after delete"),
                "Expected 'file still exists after delete' in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for file still existing"),
    }
}

#[tokio::test]
async fn test_prepare_parent_dir_missing_shows_phase() {
    // FileWrite prepare where parent directory is missing should show [prepare] phase.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir
        .path()
        .join("nonexistent_parent")
        .join("new_file.txt");
    let file_path_str = file_path.display().to_string();
    // Note: neither the file NOR the parent directory exists

    let adapter = FsAdapter::new("fs");
    let request = create_test_request(&file_path_str);

    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[prepare]"),
                "Expected '[prepare]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("parent directory does not exist"),
                "Expected 'parent directory does not exist' in validation error, got: {}",
                msg
            );
        }
        _ => panic!("expected validation error for missing parent directory"),
    }
}

// =============================================================================
// Execute/Rollback Phase-Aware Error Normalization Tests (P2.1)
// =============================================================================

#[tokio::test]
async fn test_execute_filewrite_io_error_shows_phase() {
    // FileWrite execute where write fails should show [execute] phase context.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("execute_write_fail.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Remove write permissions to cause IO error on write
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    let result = adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&file_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Internal(msg) => {
            assert!(
                msg.contains("[execute]"),
                "Expected '[execute]' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileWrite failed"),
                "Expected 'FileWrite failed' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains(&file_path_str),
                "Expected file path in internal error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected internal error for write failure, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_execute_filedelete_io_error_shows_phase() {
    // FileDelete execute where delete fails should show [execute] phase context.
    // We make the parent directory read-only so remove_file fails with Permission denied.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("execute_delete_fail.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"to be deleted").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Remove write permission from parent directory to cause delete to fail
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&temp_dir).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o555); // Read-only directory
        std::fs::set_permissions(&temp_dir, perms).unwrap();
    }

    let result = adapter.execute(&contract, &serde_json::json!({})).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&temp_dir) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&temp_dir, perms);
        }
    }

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Internal(msg) => {
            assert!(
                msg.contains("[execute]"),
                "Expected '[execute]' in internal error, got: {}",
                msg
            );
            assert!(
                msg.contains("FileDelete failed"),
                "Expected 'FileDelete failed' in internal error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected internal error for delete failure, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_rollback_snapshot_missing_shows_phase() {
    // Rollback where snapshot is missing should show [rollback] phase context.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("rollback_snapshot_missing.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Contract with different execution_id = no snapshot
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(), // Different execution_id = no snapshot
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str,
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(),
    };

    let result = adapter.rollback(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("[rollback]"),
                "Expected '[rollback]' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("snapshot not found"),
                "Expected 'snapshot not found' in validation error, got: {}",
                msg
            );
            assert!(
                msg.contains("cannot restore"),
                "Expected 'cannot restore' in validation error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error for missing snapshot, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_rollback_created_new_file_cleanup_shows_phase() {
    // Rollback for new-file creation where cleanup fails should return recovered=false
    // (fail-closed behavior: I/O errors during recovery return recovered=false, not errors).
    // We set up a scenario where the parent directory becomes non-writable, preventing cleanup.
    let temp_dir = tempdir().unwrap();
    let sub_dir = temp_dir.path().join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();
    let file_path = sub_dir.join("new_file.txt");
    let file_path_str = file_path.display().to_string();

    let adapter = FsAdapter::new("fs");

    // Prepare (new file case)
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute creates the file
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Now remove parent directory write permissions to make cleanup fail
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&sub_dir).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o555); // Read-only directory
        std::fs::set_permissions(&sub_dir, perms).unwrap();
    }

    let result = adapter.rollback(&contract).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&sub_dir) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&sub_dir, perms);
        }
    }

    // Fail-closed: rollback returns Ok(RecoveryReceipt { recovered: false, ... })
    // instead of propagating the I/O error
    let receipt = result.expect("rollback should succeed (return Receipt, not error)");
    assert!(
        !receipt.recovered,
        "Expected recovered=false for cleanup failure, got recovered=true"
    );
    assert!(
        receipt
            .adapter_metadata
            .get("failure_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("[rollback]") && s.contains("remove_file"))
            .unwrap_or(false),
        "Expected failure_reason to contain '[rollback]' and 'remove_file', got: {:?}",
        receipt.adapter_metadata.get("failure_reason")
    );
}

#[tokio::test]
async fn test_rollback_restore_failure_shows_phase() {
    // Rollback where restore fails should return recovered=false (fail-closed behavior).
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("restore_fail.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute changes
    adapter
        .execute(&contract, &serde_json::json!("modified content"))
        .await
        .unwrap();

    // Now remove write permissions on the file to make restore fail
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o444); // Read-only, not removable by non-root
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    let result = adapter.rollback(&contract).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&file_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    // Fail-closed: rollback returns Ok(RecoveryReceipt { recovered: false, ... })
    // instead of propagating the I/O error
    let receipt = result.expect("rollback should succeed (return Receipt, not error)");
    assert!(
        !receipt.recovered,
        "Expected recovered=false for restore failure, got recovered=true"
    );
    assert!(
        receipt
            .adapter_metadata
            .get("failure_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("[rollback]") && s.contains("copy"))
            .unwrap_or(false),
        "Expected failure_reason to contain '[rollback]' and 'copy', got: {:?}",
        receipt.adapter_metadata.get("failure_reason")
    );
}

#[tokio::test]
async fn test_compensate_error_shows_phase_from_rollback() {
    // Compensate reuses rollback, so errors should show [rollback] phase context.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("compensate_phase.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Contract with different execution_id = no snapshot
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(), // Different execution_id = no snapshot
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str,
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(),
    };

    let result = adapter.compensate(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            // compensate calls rollback, so phase should be [rollback]
            assert!(
                msg.contains("[rollback]"),
                "Expected '[rollback]' in validation error from compensate, got: {}",
                msg
            );
            assert!(
                msg.contains("snapshot not found"),
                "Expected 'snapshot not found' in validation error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error from compensate (which calls rollback), got: {:?}",
            other
        ),
    }
}

// =============================================================================
// FileMove Tests
// =============================================================================

#[tokio::test]
async fn test_file_move_happy_path_prepare_execute_verify() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"move me").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);

    let exec_contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata.clone(),
    };

    let exec_receipt = adapter
        .execute(
            &exec_contract,
            &serde_json::json!({ "destination": &dest_str }),
        )
        .await
        .unwrap();
    assert!(
        exec_receipt
            .adapter_metadata
            .get("destination_path")
            .is_some()
    );
    assert!(dest.exists());
    assert!(!source.exists());
    assert_eq!(std::fs::read(&dest).unwrap(), b"move me");

    // Build a verify contract that includes destination_path (normally added by
    // the execution pipeline to the contract metadata before verify)
    let mut verify_contract = exec_contract.clone();
    verify_contract
        .metadata
        .insert("destination_path".to_string(), serde_json::json!(&dest_str));
    let verify_receipt = adapter.verify(&verify_contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_file_move_rollback_restores() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let exec_contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta.clone(),
    };

    adapter
        .execute(
            &exec_contract,
            &serde_json::json!({ "destination": &dest_str }),
        )
        .await
        .unwrap();
    assert!(dest.exists());
    assert!(!source.exists());

    let receipt = adapter.rollback(&exec_contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(source.exists());
    assert!(!dest.exists());
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
}

#[tokio::test]
async fn test_file_move_verify_fails_if_source_still_exists() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"test").unwrap();
    // Create dest too — we want dest to exist (so that check passes), but source also
    // still exists (so the source-still-exists check fails). This simulates a failed
    // move that partially succeeded.
    std::fs::write(&dest, b"dest").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(msg.contains("source still exists"));
        }
        _ => panic!("expected validation error"),
    }
}

#[tokio::test]
async fn test_file_move_prepare_fails_if_source_missing() {
    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request("/nonexistent/source.txt");
    request.action_type = ActionType::FileMove;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_move_rollback_fails_if_dest_missing() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let source_str = source.display().to_string();
    std::fs::write(&source, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: JsonMap::new(),
    };

    let result = adapter.rollback(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_move_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta.clone(),
    };

    let exec_contract = RollbackContract {
        metadata: meta.clone(),
        ..contract.clone()
    };
    adapter
        .execute(
            &exec_contract,
            &serde_json::json!({ "destination": &dest_str }),
        )
        .await
        .unwrap();

    let receipt = adapter.compensate(&exec_contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(source.exists());
}

#[tokio::test]
async fn test_file_move_execute_missing_dest_payload() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let source_str = source.display().to_string();
    std::fs::write(&source, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    let result = adapter.execute(&contract, &serde_json::json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_move_verify_fails_if_dest_missing() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let source_str = source.display().to_string();
    std::fs::write(&source, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String("/nonexistent/dest.txt".to_string()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_move_verify_fails_if_dest_not_created() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileMove;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(msg.contains("destination not found") || msg.contains("source still exists"));
        }
        _ => panic!("expected validation error"),
    }
}

// =============================================================================
// FileCopy Tests
// =============================================================================

#[tokio::test]
async fn test_file_copy_happy_path_prepare_execute_verify() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"copy me").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);
    assert!(prep_receipt.adapter_metadata.get("source_hash").is_some());

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta.clone(),
    };

    let exec_receipt = adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert!(exec_receipt.adapter_metadata.get("copy_hash").is_some());
    assert!(
        exec_receipt
            .adapter_metadata
            .get("created_new_dest")
            .and_then(|v| v.as_bool())
            == Some(true)
    );
    assert!(dest.exists());
    assert!(source.exists());
    assert_eq!(std::fs::read(&dest).unwrap(), b"copy me");

    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_file_copy_rollback_new_dest_deletes() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );
    meta.insert(
        "created_new_dest".to_string(),
        serde_json::Value::Bool(true),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta.clone(),
    };

    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert!(dest.exists());

    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(!dest.exists());
    assert!(source.exists());
}

#[tokio::test]
async fn test_file_copy_rollback_existing_restores_snapshot() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"source content").unwrap();
    std::fs::write(&dest, b"original dest content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    adapter.prepare(&request).await.unwrap();

    let exec_id = request.execution_id;

    let mut meta = JsonMap::new();
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );
    meta.insert(
        "created_new_dest".to_string(),
        serde_json::Value::Bool(false),
    );
    meta.insert(
        "source_hash".to_string(),
        serde_json::Value::String(FsAdapter::compute_file_hash(&source_str).unwrap()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: exec_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta.clone(),
    };

    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"source content");

    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert_eq!(std::fs::read(&dest).unwrap(), b"original dest content");
}

#[tokio::test]
async fn test_file_copy_prepare_fails_if_source_missing() {
    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request("/nonexistent/source.txt");
    request.action_type = ActionType::FileCopy;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_copy_verify_fails_if_dest_missing() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let source_str = source.display().to_string();
    std::fs::write(&source, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String("/nonexistent/dest.txt".to_string()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_copy_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );
    meta.insert(
        "created_new_dest".to_string(),
        serde_json::Value::Bool(true),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert!(dest.exists());

    let receipt = adapter.compensate(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(!dest.exists());
}

#[tokio::test]
async fn test_file_copy_source_unaffected_after_execute() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original source").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), b"original source");
    assert_eq!(std::fs::read(&dest).unwrap(), b"original source");
}

#[tokio::test]
async fn test_file_copy_execute_missing_dest_payload() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let source_str = source.display().to_string();
    std::fs::write(&source, b"test").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    let result = adapter.execute(&contract, &serde_json::json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_copy_cross_instance_rollback() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"original").unwrap();

    let adapter_a = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );
    meta.insert(
        "created_new_dest".to_string(),
        serde_json::Value::Bool(true),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta.clone(),
    };
    adapter_a
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();

    let adapter_b = FsAdapter::new("fs");
    let receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(!dest.exists());
}

#[tokio::test]
async fn test_file_copy_existing_dest_snapshot_on_execute() {
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&dest, b"existing dest").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );
    meta.insert(
        "created_new_dest".to_string(),
        serde_json::Value::Bool(false),
    );
    meta.insert(
        "source_hash".to_string(),
        serde_json::Value::String(FsAdapter::compute_file_hash(&source_str).unwrap()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"source");

    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert_eq!(std::fs::read(&dest).unwrap(), b"existing dest");
}

// =============================================================================
// FileAppend Tests
// =============================================================================

#[tokio::test]
async fn test_file_append_prepare_rejects_missing_file() {
    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request("/nonexistent/file.txt");
    request.action_type = ActionType::FileAppend;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_append_prepare_rejects_empty_data() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;

    // Empty payload should fail in execute, not prepare
    // Prepare should succeed since it only checks file exists
    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Empty data should fail in execute
    let result = adapter.execute(&contract, &serde_json::json!("")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_append_prepare_captures_original_state() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);

    // Verify original hash and length are captured
    let meta = &prep_receipt.adapter_metadata;
    assert!(meta.get("original_hash").is_some());
    assert!(meta.get("original_length").is_some());
    assert!(meta.get("target_path").is_some());

    let original_hash = meta.get("original_hash").unwrap().as_str().unwrap();
    let original_length: u64 = meta
        .get("original_length")
        .unwrap()
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(original_length, 16); // "original content" is 16 bytes
    assert_eq!(
        FsAdapter::compute_file_hash(&target_str).unwrap(),
        original_hash
    );
}

#[tokio::test]
async fn test_file_append_execute_appends_data() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    let exec_receipt = adapter
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();

    // Verify the file content
    assert_eq!(std::fs::read(&target).unwrap(), b"original appended");

    // Verify execute receipt has the required metadata
    let meta = &exec_receipt.adapter_metadata;
    assert!(meta.get("new_hash").is_some());
    assert!(meta.get("new_length").is_some());
    assert!(meta.get("bytes_appended").is_some());
    assert!(meta.get("data_hash").is_some());

    let new_length: u64 = meta
        .get("new_length")
        .unwrap()
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(new_length, 17); // "original appended" is 17 bytes
}

#[tokio::test]
async fn test_file_append_verify_confirms_growth() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata.clone(),
    };

    adapter
        .execute(&contract, &serde_json::json!(" data"))
        .await
        .unwrap();

    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_file_append_rollback_truncates_and_restores_hash() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original content").unwrap();

    let original_hash = FsAdapter::compute_file_hash(&target_str).unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();

    // Verify file was modified
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"original content appended"
    );

    // Rollback
    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);

    // Verify file restored to original
    assert_eq!(std::fs::read(&target).unwrap(), b"original content");
    assert_eq!(
        FsAdapter::compute_file_hash(&target_str).unwrap(),
        original_hash
    );
}

#[tokio::test]
async fn test_file_append_rollback_idempotent() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();

    // First rollback
    let receipt1 = adapter.rollback(&contract).await.unwrap();
    assert!(receipt1.recovered);
    assert_eq!(std::fs::read(&target).unwrap(), b"original");

    // Second rollback should also succeed (idempotent)
    let receipt2 = adapter.rollback(&contract).await.unwrap();
    assert!(receipt2.recovered);
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
}

#[tokio::test]
async fn test_file_append_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();

    // compensate should work the same as rollback
    let receipt = adapter.compensate(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
}

#[tokio::test]
async fn test_file_append_cross_instance_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"original").unwrap();

    let adapter_a = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileAppend;
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter_a
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();

    // Different adapter instance does rollback
    let adapter_b = FsAdapter::new("fs");
    let receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
}

// =============================================================================
// FileChmod Tests
// =============================================================================

#[tokio::test]
async fn test_file_chmod_prepare_rejects_missing_file() {
    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request("/nonexistent/file.txt");
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("0o755".to_string()),
    );
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("FileChmod target file not found"));
}

#[tokio::test]
async fn test_file_chmod_prepare_rejects_empty_mode() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    // Empty mode should fail
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("".to_string()),
    );
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("FileChmod mode cannot be empty"));
}

#[tokio::test]
async fn test_file_chmod_prepare_captures_original_mode() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    // Set a specific mode for testing
    let original_mode = 0o644;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&target).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("0o755".to_string()),
    );

    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);

    // Verify original mode is captured
    let meta = &prep_receipt.adapter_metadata;
    assert!(meta.get("original_mode").is_some());
    assert!(meta.get("new_mode").is_some());
    assert!(meta.get("target_path").is_some());

    let captured_original = meta.get("original_mode").unwrap().as_str().unwrap();
    // Mode should be stored as octal string
    let captured_mode = u32::from_str_radix(captured_original, 8).unwrap();
    assert_eq!(captured_mode, original_mode);
}

#[tokio::test]
async fn test_file_chmod_execute_changes_permissions() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    let original_mode = 0o644;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&target).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("0o755".to_string()),
    );
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileChmod,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    let exec_receipt = adapter
        .execute(&contract, &serde_json::Value::Null)
        .await
        .unwrap();

    // Verify the file permissions changed
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let new_perms = std::fs::metadata(&target).unwrap().permissions();
        let new_mode = new_perms.mode() & 0o7777;
        assert_eq!(new_mode, 0o755);
    }

    // Verify execute receipt has applied_mode
    let meta = &exec_receipt.adapter_metadata;
    assert!(meta.get("applied_mode").is_some());
    let applied = meta.get("applied_mode").unwrap().as_str().unwrap();
    assert_eq!(applied, "755");
}

#[tokio::test]
async fn test_file_chmod_verify_confirms_new_mode() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    let original_mode = 0o644;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&target).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("755".to_string()),
    );
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileChmod,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata.clone(),
    };

    adapter
        .execute(&contract, &serde_json::Value::Null)
        .await
        .unwrap();

    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_file_chmod_rollback_restores_original_mode() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    let original_mode = 0o644;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&target).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("0o755".to_string()),
    );
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileChmod,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::Value::Null)
        .await
        .unwrap();

    // Verify mode was changed
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode_after_exec = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode_after_exec, 0o755);
    }

    // Rollback
    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);

    // Verify mode restored to original
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let restored_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
        assert_eq!(restored_mode, original_mode);
    }
}

#[tokio::test]
async fn test_file_chmod_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    let original_mode = 0o600;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&target).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("0o755".to_string()),
    );
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileChmod,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::Value::Null)
        .await
        .unwrap();

    // compensate should work the same as rollback
    let receipt = adapter.compensate(&contract).await.unwrap();
    assert!(receipt.recovered);

    // Verify mode restored
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let restored_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
        assert_eq!(restored_mode, original_mode);
    }
}

#[tokio::test]
async fn test_file_chmod_cross_instance_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("test.txt");
    let target_str = target.display().to_string();
    std::fs::write(&target, b"content").unwrap();

    let original_mode = 0o644;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&target).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    let adapter_a = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileChmod;
    request.metadata.insert(
        "mode".to_string(),
        serde_json::Value::String("0o777".to_string()),
    );
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileChmod,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter_a
        .execute(&contract, &serde_json::Value::Null)
        .await
        .unwrap();

    // Different adapter instance does rollback
    let adapter_b = FsAdapter::new("fs");
    let receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);

    // Verify mode restored
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let restored_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
        assert_eq!(restored_mode, original_mode);
    }
}

// =============================================================================
// DirCreate Tests
// =============================================================================

#[tokio::test]
async fn test_dir_create_happy_path_prepare_execute_verify() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("new_dir");
    let target_str = target.display().to_string();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirCreate;
    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);
    assert!(!target.exists());

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirCreate,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(target.exists());
    assert!(target.is_dir());

    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_dir_create_rollback_deletes_created_dir() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("new_dir");
    let target_str = target.display().to_string();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirCreate;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirCreate,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(target.exists());

    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(!target.exists());
}

#[tokio::test]
async fn test_dir_create_reject_existing_dir() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("existing_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirCreate;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dir_create_reject_missing_parent() {
    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request("/nonexistent/deeply/nested/dir_that_cant_exist");
    request.action_type = ActionType::DirCreate;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dir_create_verify_fails_if_missing() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("missing_dir");
    let target_str = target.display().to_string();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirCreate;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirCreate,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Don't execute → dir doesn't exist → verify should fail
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dir_create_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("comp_dir");
    let target_str = target.display().to_string();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirCreate;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirCreate,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();

    let receipt = adapter.compensate(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(!target.exists());
}

// =============================================================================
// DirDelete Tests
// =============================================================================

#[tokio::test]
async fn test_dir_delete_happy_path_prepare_execute_verify() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("to_delete_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirDelete;
    let prep_receipt = adapter.prepare(&request).await.unwrap();
    assert!(prep_receipt.accepted);

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!target.exists());

    let verify_receipt = adapter.verify(&contract).await.unwrap();
    assert!(verify_receipt.verified);
}

#[tokio::test]
async fn test_dir_delete_rollback_recreates_dir() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("to_delete_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirDelete;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!target.exists());

    let receipt = adapter.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(target.exists());
    assert!(target.is_dir());
}

#[tokio::test]
async fn test_dir_delete_reject_nonexistent_dir() {
    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request("/does_not_exist_dir");
    request.action_type = ActionType::DirDelete;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dir_delete_reject_nonempty_dir() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("nonempty_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("file"), b"data").unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirDelete;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dir_delete_verify_fails_if_still_exists() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("still_exists_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirDelete;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Don't execute → dir still exists → verify should fail
    let result = adapter.verify(&contract).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_dir_delete_compensate_aliases_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("comp_del_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();

    let adapter = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirDelete;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();

    let receipt = adapter.compensate(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(target.exists());
}

#[tokio::test]
async fn test_dir_create_cross_instance_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("cross_dir");
    let target_str = target.display().to_string();

    let adapter_a = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirCreate;
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirCreate,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Instance A: execute
    adapter_a
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(target.exists());

    // Instance B: rollback
    let adapter_b = FsAdapter::new("fs");
    let receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(!target.exists());
}

#[tokio::test]
async fn test_dir_delete_cross_instance_rollback() {
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("cross_del_dir");
    let target_str = target.display().to_string();
    std::fs::create_dir(&target).unwrap();

    let adapter_a = FsAdapter::new("fs");
    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::DirDelete;
    let prep_receipt = adapter_a.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::DirDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: target_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Instance A: execute
    adapter_a
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!target.exists());

    // Instance B: rollback
    let adapter_b = FsAdapter::new("fs");
    let receipt = adapter_b.rollback(&contract).await.unwrap();
    assert!(receipt.recovered);
    assert!(target.exists());
}

// =============================================================================
// Cross-Filesystem Move Tests
// =============================================================================

#[tokio::test]
async fn test_fs_file_move_cross_filesystem_fallback() {
    // Test that FileMove handles cross-filesystem scenario by falling back to copy+delete.
    // We simulate this by using a helper that wraps the move operation.
    // Since we can't easily create actual cross-filesystem scenarios in tests,
    // we test the cross_filesystem_move helper directly.
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();

    std::fs::write(&source, b"cross-fs content").unwrap();

    // Set the original permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&source).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&source, perms).unwrap();
    }

    // Use the cross_filesystem_move helper directly (simulating EXDEV fallback)
    FsAdapter::cross_filesystem_move(&source_str, &dest_str).unwrap();

    // Verify source was deleted and dest was created with content
    assert!(
        !source.exists(),
        "source should be deleted after cross-fs move"
    );
    assert!(dest.exists(), "dest should exist after cross-fs move");
    assert_eq!(std::fs::read(&dest).unwrap(), b"cross-fs content");

    // Verify permissions were preserved (on Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dest_perms = std::fs::metadata(&dest).unwrap().permissions();
        let dest_mode = dest_perms.mode() & 0o7777;
        assert_eq!(
            dest_mode, 0o755,
            "dest should have same permissions as source"
        );
    }
}

#[tokio::test]
async fn test_fs_file_size_limit_enforcement() {
    // Test that file size limits are enforced during prepare.
    let temp_dir = tempdir().unwrap();
    let target = temp_dir.path().join("large_file.txt");
    let target_str = target.display().to_string();

    // Create a file that exceeds the default 100MB limit
    // We use a smaller test size to make the test practical
    let small_limit: u64 = 100; // 100 bytes
    let bounds = FsBoundsConfig {
        max_file_size: small_limit,
        ..Default::default()
    };
    let adapter = FsAdapter::new_with_bounds("fs", bounds);

    // Create a file larger than the limit
    let content = vec![0u8; 150]; // 150 bytes, exceeds 100 byte limit
    std::fs::write(&target, &content).unwrap();

    let mut request = create_test_request(&target_str);
    request.action_type = ActionType::FileWrite;

    let result = adapter.prepare(&request).await;
    assert!(
        result.is_err(),
        "prepare should fail when file exceeds size limit"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("exceeds"),
                "Expected error about exceeding limit, got: {}",
                msg
            );
        }
        other => panic!("expected validation error for file size, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_fs_path_depth_limit() {
    // Test that path depth limits are enforced during prepare.
    let temp_dir = tempdir().unwrap();

    // Create a deeply nested path structure
    let deep_dir = temp_dir
        .path()
        .join("a")
        .join("b")
        .join("c")
        .join("d")
        .join("e");
    std::fs::create_dir_all(&deep_dir).unwrap();

    // Create a file at depth 6 (exceeds our limit of 5)
    let deep_path = deep_dir.join("file.txt");
    std::fs::write(&deep_path, b"deep content").unwrap();

    let deep_path_str = deep_path.display().to_string();

    // Use a very low depth limit for testing
    let bounds = FsBoundsConfig {
        max_path_depth: 5,
        ..Default::default()
    };
    let adapter = FsAdapter::new_with_bounds("fs", bounds);

    let mut request = create_test_request(&deep_path_str);
    request.action_type = ActionType::FileWrite;

    let result = adapter.prepare(&request).await;
    assert!(
        result.is_err(),
        "prepare should fail when path exceeds depth limit"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("depth") && msg.contains("exceeds"),
                "Expected error about path depth, got: {}",
                msg
            );
        }
        other => panic!("expected validation error for path depth, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_fs_symlink_escape_rejection() {
    // Test that symlinks escaping the sandbox are rejected.
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("work");
    let escape_dir = temp_dir.path().join("escape");
    std::fs::create_dir(&work_dir).unwrap();
    std::fs::create_dir(&escape_dir).unwrap();

    // Create a file in the escape directory
    let target_file = escape_dir.join("secret.txt");
    std::fs::write(&target_file, b"secret").unwrap();

    // Create a symlink in work_dir that points to the escape directory
    let symlink_path = work_dir.join("link_to_escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

    // On non-Unix, skip this test part
    #[cfg(not(unix))]
    {
        // For non-Unix, we just return without testing symlinks
        return;
    }

    let symlink_str = symlink_path.display().to_string();

    // Use bounds that reject symlinks
    let bounds = FsBoundsConfig {
        allow_symlinks: false,
        sandbox_to_workdir: true,
        ..Default::default()
    };
    let adapter = FsAdapter::new_with_bounds("fs", bounds);

    let mut request = create_test_request(&symlink_str);
    request.action_type = ActionType::FileWrite;

    let result = adapter.prepare(&request).await;
    assert!(
        result.is_err(),
        "prepare should fail when symlink is not allowed"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink") || msg.contains("escape"),
                "Expected error about symlink, got: {}",
                msg
            );
        }
        other => panic!("expected validation error for symlink, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_fs_bounded_copy_preserves_permissions() {
    // Test that FileCopy preserves file permissions.
    let temp_dir = tempdir().unwrap();
    let source = temp_dir.path().join("source.txt");
    let dest = temp_dir.path().join("dest.txt");
    let source_str = source.display().to_string();
    let dest_str = dest.display().to_string();

    std::fs::write(&source, b"content with permissions").unwrap();

    // Set specific permissions on the source file
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&source).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&source, perms).unwrap();
    }

    let adapter = FsAdapter::new("fs");

    // Prepare the copy operation
    let mut request = create_test_request(&source_str);
    request.action_type = ActionType::FileCopy;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    // Execute the copy
    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();

    // Verify the destination file exists and has the same permissions
    assert!(dest.exists(), "dest should exist after copy");
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        b"content with permissions",
        "dest should have same content as source"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let source_perms = std::fs::metadata(&source).unwrap().permissions().mode() & 0o7777;
        let dest_perms = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o7777;
        assert_eq!(
            dest_perms, source_perms,
            "dest permissions should match source permissions"
        );
    }
}

// =============================================================================
// P2.1 Fail-Closed Rollback Tests
// =============================================================================

#[tokio::test]
async fn test_rollback_fail_closed_on_permission_denied_file_write() {
    // Test that permission denied during FileWrite rollback returns recovered=false
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("perm_denied_write.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute changes
    adapter
        .execute(&contract, &serde_json::json!("modified content"))
        .await
        .unwrap();

    // Make file read-only to trigger permission denied on restore
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o444); // Read-only
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    let result = adapter.rollback(&contract).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&file_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    // Should return recovered=false (fail-closed)
    let receipt = result.expect("rollback returns Receipt, not error");
    assert!(!receipt.recovered, "Expected recovered=false on I/O error");
    assert!(
        receipt
            .adapter_metadata
            .get("rollback_failed")
            .and_then(|v| v.as_bool())
            == Some(true),
        "Expected rollback_failed=true in metadata"
    );
    let reason = receipt
        .adapter_metadata
        .get("failure_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        reason.contains("[rollback]") && reason.contains("copy"),
        "Expected failure_reason to contain '[rollback]' and 'copy', got: {}",
        reason
    );
}

#[tokio::test]
async fn test_rollback_fail_closed_on_permission_denied_delete() {
    // Test that permission denied during FileDelete (new file) rollback returns recovered=false
    let temp_dir = tempdir().unwrap();
    let sub_dir = temp_dir.path().join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();
    let file_path = sub_dir.join("new_file.txt");
    let file_path_str = file_path.display().to_string();

    let adapter = FsAdapter::new("fs");

    // Prepare (new file case)
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute creates the file
    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    // Make parent directory read-only to prevent file deletion
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&sub_dir).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o555); // Read-only directory
        std::fs::set_permissions(&sub_dir, perms).unwrap();
    }

    let result = adapter.rollback(&contract).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&sub_dir) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&sub_dir, perms);
        }
        // Also try to clean up the file if it still exists
        let _ = std::fs::remove_file(&file_path);
    }

    // Should return recovered=false (fail-closed)
    let receipt = result.expect("rollback returns Receipt, not error");
    assert!(!receipt.recovered, "Expected recovered=false on I/O error");
    assert!(
        receipt
            .adapter_metadata
            .get("rollback_failed")
            .and_then(|v| v.as_bool())
            == Some(true),
        "Expected rollback_failed=true in metadata"
    );
    let reason = receipt
        .adapter_metadata
        .get("failure_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        reason.contains("[rollback]") && reason.contains("remove_file"),
        "Expected failure_reason to contain '[rollback]' and 'remove_file', got: {}",
        reason
    );
}

#[tokio::test]
async fn test_compensate_inherits_fail_closed_behavior() {
    // Test that compensate() (which delegates to rollback) also returns recovered=false on I/O errors
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("compensate_test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute changes
    adapter
        .execute(&contract, &serde_json::json!("modified"))
        .await
        .unwrap();

    // Make file read-only to trigger permission denied on compensate
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mut perms = metadata.permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    let result = adapter.compensate(&contract).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&file_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    // compensate() should also return recovered=false (fail-closed)
    let receipt = result.expect("compensate returns Receipt, not error");
    assert!(!receipt.recovered, "Expected recovered=false on I/O error");
    assert!(
        receipt
            .adapter_metadata
            .get("rollback_failed")
            .and_then(|v| v.as_bool())
            == Some(true),
        "Expected rollback_failed=true in metadata"
    );
}

// =============================================================================
// Symlink Hardening: Final-Path Symlink Rejection Tests
// =============================================================================

#[tokio::test]
async fn test_symlink_final_path_rejected_at_prepare() {
    // Test that a symlink as the final path is rejected at prepare.
    let temp_dir = tempdir().unwrap();
    let target_file = temp_dir.path().join("target.txt");
    let symlink_path = temp_dir.path().join("link");
    std::fs::write(&target_file, b"target").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

    #[cfg(not(unix))]
    return; // Skip on non-Unix

    let symlink_str = symlink_path.display().to_string();
    let adapter = FsAdapter::new("fs");

    let mut request = create_test_request(&symlink_str);
    request.action_type = ActionType::FileWrite;

    let result = adapter.prepare(&request).await;
    assert!(
        result.is_err(),
        "prepare should reject symlink as final path"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink"),
                "Expected error about symlink, got: {}",
                msg
            );
        }
        other => panic!("expected validation error for symlink, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_symlink_intermediate_path_rejected_at_prepare() {
    // Test that a symlink in an intermediate path component is rejected at prepare.
    let temp_dir = tempdir().unwrap();
    let real_dir = temp_dir.path().join("real_dir");
    std::fs::create_dir(&real_dir).unwrap();
    let target_file = real_dir.join("target.txt");
    std::fs::write(&target_file, b"target").unwrap();

    // Create a symlink to the real directory
    let symlink_dir = temp_dir.path().join("link_dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, &symlink_dir).unwrap();

    #[cfg(not(unix))]
    return; // Skip on non-Unix

    // Path that goes through the symlink: link_dir/target.txt
    let path_through_symlink = symlink_dir.join("target.txt");
    let path_str = path_through_symlink.display().to_string();

    let adapter = FsAdapter::new("fs");

    let mut request = create_test_request(&path_str);
    request.action_type = ActionType::FileWrite;

    let result = adapter.prepare(&request).await;
    assert!(
        result.is_err(),
        "prepare should reject path with intermediate symlink"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink"),
                "Expected error about symlink, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error for intermediate symlink, got: {:?}",
            other
        ),
    }
}

// =============================================================================
// Symlink Hardening: Execute-Phase Revalidation Tests
// =============================================================================

#[tokio::test]
async fn test_symlink_swap_between_prepare_and_execute_fails_execute() {
    // Test that if a symlink is swapped between prepare and execute,
    // execute fails with validation error (fail-closed).
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("work");
    let escape_dir = temp_dir.path().join("escape");
    std::fs::create_dir(&work_dir).unwrap();
    std::fs::create_dir(&escape_dir).unwrap();

    // Create target file in escape directory
    let target_file = escape_dir.join("secret.txt");
    std::fs::write(&target_file, b"secret").unwrap();

    // Create initial file in work directory
    let initial_file = work_dir.join("file.txt");
    std::fs::write(&initial_file, b"initial").unwrap();

    let initial_str = initial_file.display().to_string();

    let adapter = FsAdapter::new("fs");

    // Prepare with the initial file
    let mut request = create_test_request(&initial_str);
    request.action_type = ActionType::FileWrite;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Build contract
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: initial_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Swap the initial file to a symlink pointing to escape
    #[cfg(unix)]
    {
        std::fs::remove_file(&initial_file).unwrap();
        std::os::unix::fs::symlink(&target_file, &initial_file).unwrap();
    }

    #[cfg(not(unix))]
    return; // Skip on non-Unix

    // Execute should fail because the path is now a symlink
    let result = adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await;
    assert!(
        result.is_err(),
        "execute should fail when path becomes a symlink"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink") || msg.contains("[execute]"),
                "Expected symlink or execute-phase error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error for symlink at execute, got: {:?}",
            other
        ),
    }
}

// =============================================================================
// Symlink Hardening: Rollback-Phase Revalidation Tests
// =============================================================================

#[tokio::test]
async fn test_symlink_swap_between_execute_and_rollback_fails_rollback() {
    // Test that if a symlink is introduced between execute and rollback,
    // rollback fails with validation error (fail-closed).
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("work");
    let escape_dir = temp_dir.path().join("escape");
    std::fs::create_dir(&work_dir).unwrap();
    std::fs::create_dir(&escape_dir).unwrap();

    // Create target file in escape directory
    let target_file = escape_dir.join("secret.txt");
    std::fs::write(&target_file, b"secret").unwrap();

    // Create initial file in work directory
    let initial_file = work_dir.join("file.txt");
    std::fs::write(&initial_file, b"initial").unwrap();

    let initial_str = initial_file.display().to_string();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let mut request = create_test_request(&initial_str);
    request.action_type = ActionType::FileWrite;
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Build contract
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: initial_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata.clone(),
    };

    // Execute succeeds
    adapter
        .execute(&contract, &serde_json::json!("modified"))
        .await
        .unwrap();

    // Swap the file to a symlink after execute
    #[cfg(unix)]
    {
        std::fs::remove_file(&initial_file).unwrap();
        std::os::unix::fs::symlink(&target_file, &initial_file).unwrap();
    }

    #[cfg(not(unix))]
    return; // Skip on non-Unix

    // Rollback should fail because the path is now a symlink
    let result = adapter.rollback(&contract).await;
    assert!(
        result.is_err(),
        "rollback should fail when path becomes a symlink"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink") || msg.contains("[rollback]"),
                "Expected symlink or rollback-phase error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error for symlink at rollback, got: {:?}",
            other
        ),
    }
}

// =============================================================================
// O_NOFOLLOW Hardening: Defense-in-Depth Tests
// =============================================================================

#[tokio::test]
async fn test_nofollow_blocks_symlink_write_on_unix() {
    // Test that O_NOFOLLOW blocks write operations on symlinks (Unix only).
    let temp_dir = tempdir().unwrap();
    let target_file = temp_dir.path().join("target.txt");
    let symlink_path = temp_dir.path().join("link");
    std::fs::write(&target_file, b"target content").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

    #[cfg(not(unix))]
    return; // O_NOFOLLOW is Unix-only

    let symlink_str = symlink_path.display().to_string();
    let adapter = FsAdapter::new("fs");

    // Prepare on the symlink should fail because prepare validates path
    let mut request = create_test_request(&symlink_str);
    request.action_type = ActionType::FileWrite;
    let result = adapter.prepare(&request).await;
    assert!(result.is_err(), "prepare should reject symlink path");

    // Verify the error is about symlinks
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink"),
                "Expected symlink error, got: {}",
                msg
            );
        }
        other => panic!("expected validation error for symlink, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_nofollow_normal_file_write_still_works() {
    // Test that normal file operations still work with O_NOFOLLOW.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("normal.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare should succeed
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Build contract
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute should succeed with normal file
    let exec_receipt = adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    assert!(exec_receipt.result_digest.is_some());
    // Verify file has new content
    let content = std::fs::read(&file_path).unwrap();
    assert_eq!(content, b"new content");
}

#[tokio::test]
async fn test_nofollow_file_append_rollback_truncation() {
    // Test that FileAppend rollback correctly truncates using O_NOFOLLOW helpers.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("append_rollback.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Build contract
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute append
    adapter
        .execute(&contract, &serde_json::json!(" appended data"))
        .await
        .unwrap();

    // Verify content was appended
    let content = std::fs::read(&file_path).unwrap();
    assert_eq!(content, b"original content appended data");

    // Now rollback
    let rollback_receipt = adapter.rollback(&contract).await.unwrap();
    assert!(rollback_receipt.recovered);

    // File should be truncated to original length (rollback uses write_file_nofollow)
    let content_after = std::fs::read(&file_path).unwrap();
    assert_eq!(content_after, b"original content");
}

#[tokio::test]
async fn test_nofollow_file_append_normal_still_works() {
    // Test that FileAppend still works on normal files.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("append_test.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Build contract
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute append
    let exec_receipt = adapter
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();

    assert!(exec_receipt.result_digest.is_some());

    // Verify content was appended
    let content = std::fs::read(&file_path).unwrap();
    assert_eq!(content, b"original appended");
}

// =============================================================================
// Workdir Sandbox: Constructor and Enforcement Tests
// =============================================================================

#[tokio::test]
async fn test_workdir_sandbox_enforcement_via_constructor() {
    // Test that workdir sandbox is enforced when set via constructor.
    // We use a file OUTSIDE the workdir to test the escape detection.
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("work");
    std::fs::create_dir(&work_dir).unwrap();

    // Create a file in a DIFFERENT temp directory (outside workdir)
    let outside_dir = tempdir().unwrap();
    let outside_file = outside_dir.path().join("outside.txt");
    std::fs::write(&outside_file, b"outside").unwrap();

    // Path to file outside workdir
    let outside_str = outside_file.display().to_string();

    // Create adapter with workdir set to temp_dir (which contains 'work' subdir)
    // The outside_file is in a completely different temp directory
    let bounds = FsBoundsConfig {
        allow_symlinks: false,
        sandbox_to_workdir: true,
        ..Default::default()
    };
    let adapter = FsAdapter::new_with_workdir("fs", bounds, temp_dir.path().to_path_buf());

    let mut request = create_test_request(&outside_str);
    request.action_type = ActionType::FileWrite;

    // File outside workdir should be rejected because it escapes the sandbox
    let result = adapter.prepare(&request).await;
    assert!(
        result.is_err(),
        "prepare should reject file outside workdir"
    );
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("escape") || msg.contains("workdir"),
                "Expected workdir escape error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error for workdir escape, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_workdir_with_symlink_escape_rejected() {
    // Test that workdir sandbox catches symlink escape.
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("work");
    let escape_dir = temp_dir.path().join("escape");
    std::fs::create_dir(&work_dir).unwrap();
    std::fs::create_dir(&escape_dir).unwrap();

    // Create target file in escape directory
    let target_file = escape_dir.join("secret.txt");
    std::fs::write(&target_file, b"secret").unwrap();

    // Create a symlink in work_dir pointing to escape directory
    let symlink_path = work_dir.join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

    #[cfg(not(unix))]
    return; // Skip on non-Unix

    let symlink_str = symlink_path.display().to_string();

    // Create adapter with workdir set to temp_dir (which includes both work and escape)
    // But the sandbox_to_workdir=true should catch the escape via symlink
    let bounds = FsBoundsConfig {
        allow_symlinks: false,
        sandbox_to_workdir: true,
        ..Default::default()
    };
    let adapter = FsAdapter::new_with_workdir("fs", bounds, temp_dir.path().to_path_buf());

    let mut request = create_test_request(&symlink_str);
    request.action_type = ActionType::FileWrite;

    let result = adapter.prepare(&request).await;
    assert!(result.is_err(), "prepare should reject symlink escape");
    match result.unwrap_err() {
        AdapterError::Validation(msg) => {
            assert!(
                msg.contains("symlink") || msg.contains("escape"),
                "Expected symlink or escape error, got: {}",
                msg
            );
        }
        other => panic!(
            "expected validation error for symlink escape, got: {:?}",
            other
        ),
    }
}

// =============================================================================
// FS Compensation Audit: Real-Undo Behavior Tests
// =============================================================================

#[tokio::test]
async fn test_compensation_audit_file_write_real_undo() {
    // Audit test: FileWrite rollback actually restores original content.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("audit_write.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare (captures snapshot)
    let request = create_test_request(&file_path_str);
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    // Execute with new content
    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    adapter
        .execute(&contract, &serde_json::json!("new content"))
        .await
        .unwrap();

    assert_eq!(std::fs::read(&file_path).unwrap(), b"new content");

    // Compensate should restore original content
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);
    assert_eq!(
        std::fs::read(&file_path).unwrap(),
        b"original content",
        "FileWrite compensate should restore original content"
    );
}

#[tokio::test]
async fn test_compensation_audit_file_delete_real_undo() {
    // Audit test: FileDelete rollback actually restores deleted file.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("audit_delete.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"content to delete").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileDelete,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute deletes the file
    adapter
        .execute(&contract, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(!file_path.exists(), "File should be deleted after execute");

    // Compensate should restore the file
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);
    assert!(
        file_path.exists(),
        "FileDelete compensate should restore file"
    );
    assert_eq!(
        std::fs::read(&file_path).unwrap(),
        b"content to delete",
        "FileDelete compensate should restore original content"
    );
}

#[tokio::test]
async fn test_compensation_audit_file_move_real_undo() {
    // Audit test: FileMove rollback actually moves destination back to source.
    let temp_dir = tempdir().unwrap();
    let source_file = temp_dir.path().join("source.txt");
    let dest_file = temp_dir.path().join("dest.txt");
    let source_str = source_file.display().to_string();
    let dest_str = dest_file.display().to_string();
    std::fs::write(&source_file, b"content to move").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileMove,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    // Execute moves the file
    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert!(!source_file.exists(), "Source should be moved");
    assert!(dest_file.exists(), "Destination should exist");

    // Compensate should move the file back
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);
    assert!(
        source_file.exists(),
        "Source should be restored after compensate"
    );
    assert!(
        !dest_file.exists(),
        "Destination should be removed after compensate"
    );
    assert_eq!(
        std::fs::read(&source_file).unwrap(),
        b"content to move",
        "FileMove compensate should restore original content"
    );
}

#[tokio::test]
async fn test_compensation_audit_file_copy_real_undo() {
    // Audit test: FileCopy rollback of new destination deletes it (idempotent).
    let temp_dir = tempdir().unwrap();
    let source_file = temp_dir.path().join("source.txt");
    let dest_file = temp_dir.path().join("dest.txt");
    let source_str = source_file.display().to_string();
    let dest_str = dest_file.display().to_string();
    std::fs::write(&source_file, b"source content").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let mut meta = prep_receipt.adapter_metadata;
    meta.insert(
        "destination_path".to_string(),
        serde_json::Value::String(dest_str.clone()),
    );

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileCopy,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: source_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: meta,
    };

    // Execute copies the file
    adapter
        .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
        .await
        .unwrap();
    assert!(source_file.exists(), "Source should still exist");
    assert!(dest_file.exists(), "Destination should exist after copy");

    // Compensate should delete the new destination (since it was new)
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);
    assert!(
        source_file.exists(),
        "Source should still exist after compensate"
    );
    assert!(
        !dest_file.exists(),
        "New destination should be deleted after compensate"
    );
}

#[tokio::test]
async fn test_compensation_audit_file_append_real_undo() {
    // Audit test: FileAppend rollback truncates to original length.
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("audit_append.txt");
    let file_path_str = file_path.display().to_string();
    std::fs::write(&file_path, b"original").unwrap();

    let adapter = FsAdapter::new("fs");

    // Prepare
    let request = RollbackPrepareRequest {
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: ExecutionId::new(),
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: JsonMap::new(),
    };
    let prep_receipt = adapter.prepare(&request).await.unwrap();

    let contract = RollbackContract {
        contract_id: RollbackContractId::new(),
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id: request.execution_id,
        action_type: ActionType::FileAppend,
        rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: file_path_str.clone(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prep_receipt.adapter_metadata,
    };

    // Execute appends data
    adapter
        .execute(&contract, &serde_json::json!(" appended"))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&file_path).unwrap(), b"original appended");

    // Compensate should truncate to original length
    let compensate_receipt = adapter.compensate(&contract).await.unwrap();
    assert!(compensate_receipt.recovered);
    assert_eq!(
        std::fs::read(&file_path).unwrap(),
        b"original",
        "FileAppend compensate should restore original content"
    );
}
