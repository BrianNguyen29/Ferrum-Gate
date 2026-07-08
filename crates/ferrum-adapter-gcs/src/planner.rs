//! Plannable adapter for GCS operations.
//!
//! Generates execution plans for GCS object actions with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{ActionType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!("planner-gcs-{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Converts a serde_json::Map to a JsonMap (IndexMap).
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for GCS adapter actions.
///
/// For GCS mutating operations + GcsObject, generates an execution plan with:
///
/// - **prepare_checks**: `GcsObjectExists` (if the operation expects the object to exist)
/// - **verify_checks**: `GcsGenerationMatches` placeholder (to be filled after execute)
/// - **compensation_plan**: A placeholder `gcs.generation_rollback_v1` step with
///   `before_generation` and `after_generation` args to be filled at execution time.
///
/// Read-only operations (`GcsGetObject`) produce no compensation plan.
pub struct PlannableGcsAdapter;

#[async_trait]
impl PlannableAdapter for PlannableGcsAdapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            (
                ActionType::GcsPutObject | ActionType::GcsDeleteObject,
                RollbackTarget::GcsObject { bucket, key, .. },
            ) => {
                let mut prepare_checks = Vec::new();
                let mut verify_checks = Vec::new();
                let mut compensation_plan = Vec::new();

                // Delete expects the object to exist; Put may create it.
                if matches!(action_type, ActionType::GcsDeleteObject) {
                    prepare_checks.push(ferrum_proto::CheckSpec {
                        check_type: ferrum_proto::CheckType::GcsObjectExists,
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

                verify_checks.push(ferrum_proto::CheckSpec {
                    check_type: ferrum_proto::CheckType::GcsGenerationMatches,
                    config: json_map_from_serde_map(
                        serde_json::json!({
                            "bucket": bucket.clone(),
                            "key": key.clone(),
                            "expected_generation": "TBD"
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    ),
                });

                compensation_plan.push(CompensationStep {
                    order: 1,
                    adapter_key: "gcs".to_string(),
                    operation: "gcs.generation_rollback_v1".to_string(),
                    args: json_map_from_serde_map(
                        serde_json::json!({
                            "bucket": bucket.clone(),
                            "key": key.clone(),
                            "before_generation": "TBD",
                            "after_generation": "TBD"
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
                    plan_description: format!("gcs {:?} plan for {}/{}", action_type, bucket, key),
                }))
            }
            (ActionType::GcsGetObject, RollbackTarget::GcsObject { bucket, key, .. }) => {
                Ok(Some(ExecutionPlan {
                    prepare_checks: Vec::new(),
                    verify_checks: vec![ferrum_proto::CheckSpec {
                        check_type: ferrum_proto::CheckType::GcsObjectExists,
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
                    plan_description: format!("gcs GcsGetObject plan for {}/{}", bucket, key),
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
    async fn test_plannable_gcs_put_object_plan() {
        let adapter = PlannableGcsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GcsPutObject,
                &RollbackTarget::GcsObject {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    generation: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.prepare_checks.is_empty()); // Put may create
        assert_eq!(plan.verify_checks.len(), 1);
        assert_eq!(plan.compensation_plan.len(), 1);
        assert!(!plan.auto_commit);
        assert_eq!(plan.compensation_plan[0].adapter_key, "gcs");
        assert_eq!(
            plan.compensation_plan[0].operation,
            "gcs.generation_rollback_v1"
        );
    }

    #[tokio::test]
    async fn test_plannable_gcs_delete_object_plan() {
        let adapter = PlannableGcsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GcsDeleteObject,
                &RollbackTarget::GcsObject {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    generation: None,
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
    async fn test_plannable_gcs_get_object_plan() {
        let adapter = PlannableGcsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GcsGetObject,
                &RollbackTarget::GcsObject {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    generation: None,
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
    async fn test_plannable_gcs_unknown_action_returns_none() {
        let adapter = PlannableGcsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::FileWrite,
                &RollbackTarget::GcsObject {
                    bucket: "my-bucket".to_string(),
                    key: "path/to/file.txt".to_string(),
                    generation: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_none());
    }

    #[tokio::test]
    async fn test_plannable_gcs_unknown_target_returns_none() {
        let adapter = PlannableGcsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GcsPutObject,
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
