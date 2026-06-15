// MinIO integration tests for ferrum-adapter-s3.
//
// These tests are gated with `#[ignore]` because they require a live MinIO (or S3-compatible)
// endpoint. To run them:
//
//   docker run -p 9000:9000 -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
//     minio/minio server /data
//   mc alias set local http://localhost:9000 minioadmin minioadmin
//   mc mb local/ferrum-test-bucket
//   mc version enable local/ferrum-test-bucket
//   cargo test -p ferrum-adapter-s3 -- --ignored

use ferrum_adapter_s3::{PlannableS3Adapter, S3Adapter, S3Config};
use ferrum_proto::{ActionType, CheckSpec, CheckType, JsonMap, RollbackTarget};
use ferrum_rollback::{PlannableAdapter, RollbackAdapter};

fn minio_config() -> S3Config {
    S3Config {
        allowed_bucket: "ferrum-test-bucket".to_string(),
        max_object_size: 10 * 1024 * 1024,
        require_versioning: true,
        endpoint_url: Some("http://localhost:9000".to_string()),
        region: "us-east-1".to_string(),
        live: true,
        access_key_id: Some("minioadmin".to_string()),
        secret_access_key: Some("minioadmin".to_string()),
    }
}

fn make_prepare_request(
    action_type: ActionType,
    key: &str,
) -> ferrum_proto::RollbackPrepareRequest {
    ferrum_proto::RollbackPrepareRequest {
        intent_id: ferrum_proto::IntentId::new(),
        proposal_id: ferrum_proto::ProposalId::new(),
        execution_id: ferrum_proto::ExecutionId::new(),
        action_type,
        rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
        adapter_key: "s3".to_string(),
        target: RollbackTarget::S3Object {
            bucket: "ferrum-test-bucket".to_string(),
            key: key.to_string(),
            version_id: None,
        },
        prepare_checks: Vec::new(),
        verify_checks: Vec::new(),
        compensation_plan: Vec::new(),
        auto_commit: false,
        metadata: JsonMap::new(),
    }
}

#[tokio::test]
#[ignore = "requires MinIO Docker container at localhost:9000"]
async fn test_minio_put_object_lifecycle() {
    let adapter = S3Adapter::new_with_config("s3", minio_config());
    let key = "test-put-lifecycle.txt";
    let request = make_prepare_request(ActionType::S3PutObject, key);

    // Prepare
    let prepare_receipt: ferrum_rollback::PrepareReceipt = adapter.prepare(&request).await.unwrap();
    assert!(prepare_receipt.accepted);

    // Execute
    let payload = serde_json::json!({ "content": "hello minio" });
    let contract = ferrum_proto::RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id: request.intent_id,
        proposal_id: request.proposal_id,
        execution_id: request.execution_id,
        action_type: request.action_type,
        rollback_class: request.rollback_class,
        adapter_key: request.adapter_key,
        target: request.target.clone(),
        prepare_checks: Vec::new(),
        verify_checks: Vec::new(),
        compensation_plan: Vec::new(),
        auto_commit: false,
        state: ferrum_proto::RollbackState::PendingPrepare,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: prepare_receipt.adapter_metadata,
    };
    let execute_receipt: ferrum_rollback::ExecuteReceipt =
        adapter.execute(&contract, &payload).await.unwrap();
    assert!(
        !execute_receipt
            .adapter_metadata
            .get("execution_groundwork")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    let after_version_id = execute_receipt
        .adapter_metadata
        .get("after_version_id")
        .unwrap()
        .as_str()
        .map(String::from);
    assert!(
        after_version_id.is_some(),
        "after_version_id should be set for versioned bucket"
    );

    // Verify
    let mut verify_contract = contract.clone();
    verify_contract.verify_checks = vec![CheckSpec {
        check_type: CheckType::S3ObjectExists,
        config: {
            let mut m = JsonMap::new();
            m.insert(
                "bucket".to_string(),
                serde_json::Value::String("ferrum-test-bucket".to_string()),
            );
            m.insert(
                "key".to_string(),
                serde_json::Value::String(key.to_string()),
            );
            m
        },
    }];
    verify_contract.metadata = execute_receipt.adapter_metadata;
    let verify_receipt: ferrum_rollback::VerifyReceipt =
        adapter.verify(&verify_contract).await.unwrap();
    assert!(verify_receipt.verified);

    // Rollback
    let mut rollback_contract = contract.clone();
    rollback_contract.metadata = verify_contract.metadata;
    let rollback_receipt: ferrum_rollback::RecoveryReceipt =
        adapter.rollback(&rollback_contract).await.unwrap();
    assert!(rollback_receipt.recovered);
}

#[tokio::test]
#[ignore = "requires MinIO Docker container at localhost:9000"]
async fn test_minio_delete_object_rollback() {
    let adapter = S3Adapter::new_with_config("s3", minio_config());
    let key = "test-delete-rollback.txt";

    // First put an object so we can delete it
    let put_request = make_prepare_request(ActionType::S3PutObject, key);
    let put_prepare: ferrum_rollback::PrepareReceipt = adapter.prepare(&put_request).await.unwrap();
    let put_payload = serde_json::json!({ "content": "delete me" });
    let put_contract = ferrum_proto::RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id: put_request.intent_id,
        proposal_id: put_request.proposal_id,
        execution_id: put_request.execution_id,
        action_type: put_request.action_type,
        rollback_class: put_request.rollback_class,
        adapter_key: put_request.adapter_key,
        target: put_request.target.clone(),
        prepare_checks: Vec::new(),
        verify_checks: Vec::new(),
        compensation_plan: Vec::new(),
        auto_commit: false,
        state: ferrum_proto::RollbackState::PendingPrepare,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: put_prepare.adapter_metadata,
    };
    let _put_execute: ferrum_rollback::ExecuteReceipt =
        adapter.execute(&put_contract, &put_payload).await.unwrap();

    // Now delete the object
    let delete_request = make_prepare_request(ActionType::S3DeleteObject, key);
    let delete_prepare: ferrum_rollback::PrepareReceipt =
        adapter.prepare(&delete_request).await.unwrap();
    let delete_contract = ferrum_proto::RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id: delete_request.intent_id,
        proposal_id: delete_request.proposal_id,
        execution_id: delete_request.execution_id,
        action_type: delete_request.action_type,
        rollback_class: delete_request.rollback_class,
        adapter_key: delete_request.adapter_key,
        target: delete_request.target.clone(),
        prepare_checks: Vec::new(),
        verify_checks: Vec::new(),
        compensation_plan: Vec::new(),
        auto_commit: false,
        state: ferrum_proto::RollbackState::PendingPrepare,
        created_at: chrono::Utc::now(),
        expires_at: None,
        metadata: delete_prepare.adapter_metadata,
    };
    let delete_execute: ferrum_rollback::ExecuteReceipt = adapter
        .execute(&delete_contract, &serde_json::Value::Null)
        .await
        .unwrap();
    let delete_marker_version_id = delete_execute
        .adapter_metadata
        .get("delete_marker_version_id")
        .unwrap()
        .as_str()
        .map(String::from);
    assert!(
        delete_marker_version_id.is_some(),
        "delete_marker_version_id should be set"
    );

    // Rollback (compensate) the delete
    let mut rollback_contract = delete_contract.clone();
    rollback_contract.metadata = delete_execute.adapter_metadata;
    let rollback_receipt: ferrum_rollback::RecoveryReceipt =
        adapter.rollback(&rollback_contract).await.unwrap();
    assert!(rollback_receipt.recovered);
}

#[tokio::test]
#[ignore = "requires MinIO Docker container at localhost:9000"]
async fn test_plannable_s3_adapter_plan() {
    let adapter = PlannableS3Adapter;
    let plan = adapter
        .generate_plan(
            &ActionType::S3PutObject,
            &RollbackTarget::S3Object {
                bucket: "ferrum-test-bucket".to_string(),
                key: "test-plan.txt".to_string(),
                version_id: None,
            },
        )
        .await
        .unwrap();
    assert!(plan.is_some());
    let plan = plan.unwrap();
    assert_eq!(plan.compensation_plan.len(), 1);
    assert_eq!(
        plan.compensation_plan[0].operation,
        "s3.versioning_rollback_v1"
    );
}
