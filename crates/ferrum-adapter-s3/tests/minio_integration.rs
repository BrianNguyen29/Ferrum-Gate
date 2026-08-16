// MinIO integration tests for ferrum-adapter-s3.
//
// These tests are gated with `#[ignore]` because they require a live MinIO (or S3-compatible)
// endpoint. To run them:
//
//   docker run -p 9000:9000 -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
//     minio/minio server /data
//   mc alias set local http://127.0.0.1:9000 minioadmin minioadmin
//   mc mb local/ferrum-test-bucket
//   mc version enable local/ferrum-test-bucket
//   cargo test -p ferrum-adapter-s3 --features s3-client --test minio_integration -- --ignored
//
// The WORM/Object-Lock tests create their own dedicated, uniquely-named buckets with Object Lock
// enabled at creation. They validate MinIO Object Lock governance retention and legal-hold behavior
// using test-only SDK calls (governance bypass, legal-hold clear, bucket/object cleanup). MinIO
// Object Lock behavior has been tested locally against a MinIO container (governance retention and
// legal hold); CI is configured to run the same target. It remains experimental/best-effort/
// operator-owned; these tests do not assert any compliance, immutability, or provider-certified
// guarantee.
//
// Reproduce: start MinIO on 127.0.0.1:9000, create the versioned `ferrum-test-bucket` (see Makefile),
// then run `make s3-test`.

use ferrum_adapter_s3::{PlannableS3Adapter, S3Adapter, S3Config};
use ferrum_proto::{ActionType, CheckSpec, CheckType, JsonMap, RollbackTarget};
use ferrum_rollback::{PlannableAdapter, RollbackAdapter};

fn minio_config() -> S3Config {
    S3Config {
        allowed_bucket: "ferrum-test-bucket".to_string(),
        max_object_size: 10 * 1024 * 1024,
        require_versioning: true,
        endpoint_url: Some("http://127.0.0.1:9000".to_string()),
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
#[ignore = "requires MinIO Docker container at 127.0.0.1:9000"]
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
#[ignore = "requires MinIO Docker container at 127.0.0.1:9000"]
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
#[ignore = "requires MinIO Docker container at 127.0.0.1:9000"]
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

#[cfg(feature = "s3-client")]
mod worm_minio_tests {
    use aws_credential_types::Credentials;
    use aws_sdk_s3::Client;
    use aws_sdk_s3::error::ProvideErrorMetadata;
    use aws_sdk_s3::types::{
        BucketVersioningStatus, ObjectLockLegalHold, ObjectLockLegalHoldStatus,
    };
    use ferrum_adapter_s3::{ObjectLockMode, WormUploadConfig, WormUploader};
    use std::future::Future;
    use std::sync::Arc;

    const ENDPOINT: &str = "http://127.0.0.1:9000";
    const REGION: &str = "us-east-1";
    const KEY_ID: &str = "minioadmin";
    const SECRET: &str = "minioadmin";

    fn unique_bucket() -> String {
        format!("fg-worm-{}", uuid::Uuid::new_v4().simple())
    }

    fn worm_config(bucket: &str) -> WormUploadConfig {
        WormUploadConfig {
            bucket: bucket.to_string(),
            endpoint_url: Some(ENDPOINT.to_string()),
            region: REGION.to_string(),
            access_key_id: Some(KEY_ID.to_string()),
            secret_access_key: Some(SECRET.to_string()),
            object_lock_mode: ObjectLockMode::Governance,
            retention_days: 1,
            legal_hold: false,
            live: true,
        }
    }

    async fn raw_client() -> Client {
        let aws_cfg = aws_config::from_env()
            .region(aws_sdk_s3::config::Region::new(REGION))
            .endpoint_url(ENDPOINT)
            .credentials_provider(Credentials::new(
                KEY_ID,
                SECRET,
                None,
                None,
                "ferrum-adapter-s3-worm-test",
            ))
            .load()
            .await;
        let mut builder = aws_sdk_s3::config::Builder::from(&aws_cfg);
        builder = builder.force_path_style(true);
        Client::from_conf(builder.build())
    }

    async fn create_bucket_with_object_lock(client: &Client, bucket: &str) -> Result<(), String> {
        client
            .create_bucket()
            .bucket(bucket)
            .object_lock_enabled_for_bucket(true)
            .send()
            .await
            .map_err(|e| format!("create bucket failed: {e}"))
            .map(|_| ())
    }

    async fn enable_versioning(client: &Client, bucket: &str) -> Result<(), String> {
        client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(
                aws_sdk_s3::types::VersioningConfiguration::builder()
                    .status(BucketVersioningStatus::Enabled)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| format!("enable versioning failed: {e}"))
            .map(|_| ())
    }

    async fn cleanup_version(
        client: &Client,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<(), String> {
        let mut parts = Vec::new();

        match get_legal_hold(client, bucket, key, version_id).await {
            Ok(status) => {
                if status == ObjectLockLegalHoldStatus::On {
                    if let Err(e) = put_legal_hold(
                        client,
                        bucket,
                        key,
                        version_id,
                        ObjectLockLegalHoldStatus::Off,
                    )
                    .await
                    {
                        parts.push(format!("clear legal hold failed: {e}"));
                    }
                }
            }
            Err(e) => parts.push(format!("read legal hold failed: {e}")),
        }

        if let Err(e) = delete_version(client, bucket, key, version_id, true).await {
            parts.push(format!("delete version failed: {e}"));
        }

        if parts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "cleanup version {key}/{version_id}: {}",
                parts.join("; ")
            ))
        }
    }

    async fn cleanup_bucket(client: &Client, bucket: &str) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;
        loop {
            let mut req = client.list_object_versions().bucket(bucket);
            if let Some(km) = key_marker.as_deref() {
                req = req.key_marker(km);
            }
            if let Some(vm) = version_id_marker.as_deref() {
                req = req.version_id_marker(vm);
            }
            match req.send().await {
                Ok(output) => {
                    for v in output.versions() {
                        let key = v.key().unwrap_or_default();
                        let vid = v.version_id().unwrap_or_default();
                        if let Err(e) = cleanup_version(client, bucket, key, vid).await {
                            errors.push(e);
                        }
                    }
                    for dm in output.delete_markers() {
                        let key = dm.key().unwrap_or_default();
                        let vid = dm.version_id().unwrap_or_default();
                        if let Err(e) = delete_version(client, bucket, key, vid, false).await {
                            errors.push(format!("delete marker {key}/{vid} failed: {e}"));
                        }
                    }
                    if !output.is_truncated().unwrap_or(false) {
                        break;
                    }
                    key_marker = output.next_key_marker().map(String::from);
                    version_id_marker = output.next_version_id_marker().map(String::from);
                }
                Err(e) => {
                    errors.push(format!("list_object_versions failed: {e}"));
                    break;
                }
            }
        }

        if let Err(e) = client.delete_bucket().bucket(bucket).send().await {
            errors.push(format!("delete bucket failed: {e}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn combine_run_and_cleanup(
        run_result: Result<(), String>,
        cleanup_errors: Vec<String>,
    ) -> Result<(), String> {
        match (run_result, cleanup_errors.is_empty()) {
            (Ok(()), true) => Ok(()),
            (Err(e), true) => Err(e),
            (Ok(()), false) => Err(cleanup_errors.join("; ")),
            (Err(body), false) => Err(format!(
                "test/setup failed: {body}; cleanup failed: {}",
                cleanup_errors.join("; ")
            )),
        }
    }

    async fn with_worm_bucket<F, Fut>(test: F) -> Result<(), String>
    where
        F: FnOnce(Arc<Client>, String) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let bucket = unique_bucket();
        let client = Arc::new(raw_client().await);
        create_bucket_with_object_lock(client.as_ref(), &bucket).await?;
        // Cleanup is registered immediately after bucket creation so partial setup
        // (e.g., versioning enable failure) does not leak buckets.
        let client_for_run = client.clone();
        let bucket_for_run = bucket.clone();
        let run_result = async move {
            enable_versioning(client_for_run.as_ref(), &bucket_for_run).await?;
            test(client_for_run, bucket_for_run).await
        }
        .await;
        let cleanup_result = cleanup_bucket(client.as_ref(), &bucket).await;
        let cleanup_errors = match cleanup_result {
            Ok(()) => Vec::new(),
            Err(errs) => errs,
        };
        combine_run_and_cleanup(run_result, cleanup_errors)
    }

    #[cfg(test)]
    mod cleanup_unit_tests {
        use super::combine_run_and_cleanup;

        #[test]
        fn test_combine_run_and_cleanup_all_ok() {
            assert!(combine_run_and_cleanup(Ok(()), Vec::new()).is_ok());
        }

        #[test]
        fn test_combine_run_and_cleanup_run_error_only() {
            let result = combine_run_and_cleanup(Err("body failed".into()), Vec::new());
            assert_eq!(result.unwrap_err(), "body failed");
        }

        #[test]
        fn test_combine_run_and_cleanup_cleanup_error_only() {
            let result = combine_run_and_cleanup(Ok(()), vec!["bucket delete failed".into()]);
            assert_eq!(result.unwrap_err(), "bucket delete failed");
        }

        #[test]
        fn test_combine_run_and_cleanup_both_errors() {
            let result = combine_run_and_cleanup(
                Err("hold assertion failed".into()),
                vec!["version delete failed".into()],
            );
            let err = result.unwrap_err();
            assert!(err.contains("hold assertion failed"));
            assert!(err.contains("version delete failed"));
        }
    }

    async fn head_version_exists(
        client: &Client,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<bool, String> {
        match client
            .head_object()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => match e.code() {
                Some("NoSuchKey") | Some("NoSuchVersion") | Some("NotFound") => Ok(false),
                _ => Err(format!("unexpected head_object error: {e:?}")),
            },
        }
    }

    async fn delete_version(
        client: &Client,
        bucket: &str,
        key: &str,
        version_id: &str,
        bypass_governance: bool,
    ) -> Result<
        (),
        aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::delete_object::DeleteObjectError>,
    > {
        let mut req = client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .version_id(version_id);
        if bypass_governance {
            req = req.bypass_governance_retention(true);
        }
        req.send().await.map(|_| ())
    }

    async fn get_legal_hold(
        client: &Client,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<ObjectLockLegalHoldStatus, String> {
        let output = client
            .get_object_legal_hold()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .send()
            .await
            .map_err(|e| format!("get_object_legal_hold failed: {e}"))?;
        Ok(output
            .legal_hold()
            .and_then(|lh| lh.status())
            .cloned()
            .unwrap_or(ObjectLockLegalHoldStatus::Off))
    }

    async fn put_legal_hold(
        client: &Client,
        bucket: &str,
        key: &str,
        version_id: &str,
        status: ObjectLockLegalHoldStatus,
    ) -> Result<(), String> {
        client
            .put_object_legal_hold()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .legal_hold(ObjectLockLegalHold::builder().status(status).build())
            .send()
            .await
            .map_err(|e| format!("put_object_legal_hold failed: {e}"))?;
        Ok(())
    }

    fn assert_minio_object_lock_denial(
        err: &aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::delete_object::DeleteObjectError>,
    ) -> Result<(), String> {
        let code = err.code().unwrap_or("");
        if code != "InvalidRequest" {
            return Err(format!(
                "expected Object-Lock denial code InvalidRequest, got {code}"
            ));
        }
        let msg = err.message().unwrap_or("");
        if !msg.contains("Object is WORM protected") {
            return Err(format!("unexpected MinIO denial message: {msg}"));
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires MinIO Docker container at 127.0.0.1:9000"]
    async fn test_minio_worm_governance_retention() {
        with_worm_bucket(|client, bucket| async move {
            let uploader = WormUploader::new(worm_config(&bucket));
            uploader
                .validate_bucket()
                .await
                .map_err(|e| format!("validate bucket failed: {e}"))?;

            let key = "governance-bundle.txt";
            let receipt = uploader
                .upload_object(key, b"hello governance".to_vec())
                .await
                .map_err(|e| format!("upload failed: {e}"))?;
            let version_id = receipt
                .version_id
                .ok_or("version_id should be present for versioned object-lock bucket")?;

            if !head_version_exists(&client, &bucket, key, &version_id).await? {
                return Err("uploaded version should be present".into());
            }

            let err = delete_version(&client, &bucket, key, &version_id, false)
                .await
                .err()
                .ok_or("delete without governance bypass should be denied")?;
            assert_minio_object_lock_denial(&err)?;

            delete_version(&client, &bucket, key, &version_id, true)
                .await
                .map_err(|e| format!("delete with governance bypass should succeed: {e}"))?;

            if head_version_exists(&client, &bucket, key, &version_id).await? {
                return Err("version should be absent after bypass delete".into());
            }

            Ok(())
        })
        .await
        .expect("scenario failed");
    }

    #[tokio::test]
    #[ignore = "requires MinIO Docker container at 127.0.0.1:9000"]
    async fn test_minio_worm_legal_hold() {
        with_worm_bucket(|client, bucket| async move {
            let mut config = worm_config(&bucket);
            config.legal_hold = true;
            let uploader = WormUploader::new(config);
            uploader
                .validate_bucket()
                .await
                .map_err(|e| format!("validate bucket failed: {e}"))?;

            let key = "legal-hold-bundle.txt";
            let receipt = uploader
                .upload_object(key, b"hello legal hold".to_vec())
                .await
                .map_err(|e| format!("upload failed: {e}"))?;
            let version_id = receipt
                .version_id
                .ok_or("version_id should be present for versioned object-lock bucket")?;

            if get_legal_hold(&client, &bucket, key, &version_id).await?
                != ObjectLockLegalHoldStatus::On
            {
                return Err("legal hold should be ON".into());
            }

            // Legal hold must prevent deletion even when governance retention is bypassed.
            let err = delete_version(&client, &bucket, key, &version_id, true)
                .await
                .err()
                .ok_or("delete with governance bypass and active legal hold should be denied")?;
            assert_minio_object_lock_denial(&err)?;

            // Clear legal hold.
            put_legal_hold(
                &client,
                &bucket,
                key,
                &version_id,
                ObjectLockLegalHoldStatus::Off,
            )
            .await?;
            if get_legal_hold(&client, &bucket, key, &version_id).await?
                != ObjectLockLegalHoldStatus::Off
            {
                return Err("legal hold should be OFF".into());
            }

            // Governance retention still denies deletion without bypass.
            let err = delete_version(&client, &bucket, key, &version_id, false)
                .await
                .err()
                .ok_or("delete without governance bypass should be denied")?;
            assert_minio_object_lock_denial(&err)?;

            // With governance bypass and legal hold cleared, deletion succeeds.
            delete_version(&client, &bucket, key, &version_id, true)
                .await
                .map_err(|e| {
                    format!("delete with governance bypass after clearing hold should succeed: {e}")
                })?;

            if head_version_exists(&client, &bucket, key, &version_id).await? {
                return Err("version should be absent after bypass delete".into());
            }

            Ok(())
        })
        .await
        .expect("scenario failed");
    }
}
