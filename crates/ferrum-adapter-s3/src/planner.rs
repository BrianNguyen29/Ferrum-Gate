//! Plannable adapter for S3 operations.
//!
//! Generates execution plans for S3 object actions with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{ActionType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!("planner-s3-{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for S3 adapter actions.
///
/// For S3 mutating operations + S3Object, generates an execution plan with:
///
/// - **prepare_checks**: `S3ObjectExists` (if the operation expects the object to exist)
/// - **verify_checks**: `S3VersionIdMatches` placeholder (to be filled after execute)
/// - **compensation_plan**: A placeholder `s3.versioning_rollback_v1` step with
///   `before_version_id` and `after_version_id` args to be filled at execution time.
///
/// Read-only operations (`S3GetObject`) produce no compensation plan.
pub struct PlannableS3Adapter;

#[async_trait]
impl PlannableAdapter for PlannableS3Adapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            (
                ActionType::S3PutObject | ActionType::S3DeleteObject | ActionType::S3CopyObject,
                RollbackTarget::S3Object { bucket, key, .. },
            ) => {
                let mut prepare_checks = Vec::new();
                let mut verify_checks = Vec::new();
                let mut compensation_plan = Vec::new();

                // Prepare: validate object exists for Delete/Copy (not Put, which may create)
                if matches!(
                    action_type,
                    ActionType::S3DeleteObject | ActionType::S3CopyObject
                ) {
                    prepare_checks.push(ferrum_proto::CheckSpec {
                        check_type: ferrum_proto::CheckType::S3ObjectExists,
                        config: json_map_from_serde_map(
                            serde_json::json!({
                                "bucket": bucket.clone(),
                                "key": key.clone()
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                    });
                }

                // Verify: placeholder version check (to be filled after execute)
                verify_checks.push(ferrum_proto::CheckSpec {
                    check_type: ferrum_proto::CheckType::S3VersionIdMatches,
                    config: json_map_from_serde_map(
                        serde_json::json!({
                            "bucket": bucket.clone(),
                            "key": key.clone(),
                            "expected_version_id": "TBD"
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    ),
                });

                // Compensation: placeholder versioning rollback step
                compensation_plan.push(CompensationStep {
                    order: 1,
                    adapter_key: "s3".to_string(),
                    operation: "s3.versioning_rollback_v1".to_string(),
                    args: json_map_from_serde_map(
                        serde_json::json!({
                            "bucket": bucket.clone(),
                            "key": key.clone(),
                            "before_version_id": "TBD",
                            "after_version_id": "TBD"
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    ),
                    idempotency_key: next_idempotency_key(),
                });

                Ok(Some(ExecutionPlan {
                    prepare_checks,
                    verify_checks,
                    compensation_plan,
                    auto_commit: false,
                    plan_description: format!("s3 {:?} plan for {}/{}", action_type, bucket, key),
                }))
            }
            (ActionType::S3GetObject, RollbackTarget::S3Object { bucket, key, .. }) => {
                // Read-only: no compensation, minimal checks
                Ok(Some(ExecutionPlan {
                    prepare_checks: Vec::new(),
                    verify_checks: vec![ferrum_proto::CheckSpec {
                        check_type: ferrum_proto::CheckType::S3ObjectExists,
                        config: json_map_from_serde_map(
                            serde_json::json!({
                                "bucket": bucket.clone(),
                                "key": key.clone()
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                    }],
                    compensation_plan: Vec::new(),
                    auto_commit: true,
                    plan_description: format!("s3 S3GetObject plan for {}/{}", bucket, key),
                }))
            }
            _ => Ok(None), // Not plannable by this adapter
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plannable_s3_put_object_plan() {
        let adapter = PlannableS3Adapter;
        let plan = adapter
            .generate_plan(
                &ActionType::S3PutObject,
                &RollbackTarget::S3Object {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    version_id: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.prepare_checks.is_empty()); // Put may create; no existence check
        assert_eq!(plan.verify_checks.len(), 1);
        assert_eq!(plan.compensation_plan.len(), 1);
        assert!(!plan.auto_commit);
        assert_eq!(plan.compensation_plan[0].adapter_key, "s3");
        assert_eq!(
            plan.compensation_plan[0].operation,
            "s3.versioning_rollback_v1"
        );
    }

    #[tokio::test]
    async fn test_plannable_s3_delete_object_plan() {
        let adapter = PlannableS3Adapter;
        let plan = adapter
            .generate_plan(
                &ActionType::S3DeleteObject,
                &RollbackTarget::S3Object {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    version_id: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.prepare_checks.len(), 1); // Delete expects object to exist
        assert_eq!(plan.verify_checks.len(), 1);
        assert_eq!(plan.compensation_plan.len(), 1);
    }

    #[tokio::test]
    async fn test_plannable_s3_get_object_plan() {
        let adapter = PlannableS3Adapter;
        let plan = adapter
            .generate_plan(
                &ActionType::S3GetObject,
                &RollbackTarget::S3Object {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    version_id: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.prepare_checks.is_empty());
        assert_eq!(plan.verify_checks.len(), 1);
        assert!(plan.compensation_plan.is_empty()); // Read-only
        assert!(plan.auto_commit);
    }

    #[tokio::test]
    async fn test_plannable_s3_unknown_action_returns_none() {
        let adapter = PlannableS3Adapter;
        let plan = adapter
            .generate_plan(
                &ActionType::FileWrite,
                &RollbackTarget::S3Object {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    version_id: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_none());
    }

    #[tokio::test]
    async fn test_plannable_s3_unknown_target_returns_none() {
        let adapter = PlannableS3Adapter;
        let plan = adapter
            .generate_plan(
                &ActionType::S3PutObject,
                &RollbackTarget::Generic {
                    namespace: "test".to_string(),
                    identifier: "test-id".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(plan.is_none());
    }
}
