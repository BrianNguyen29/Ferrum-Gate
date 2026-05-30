//! Plannable adapter for filesystem operations.
//!
//! Generates execution plans for FileWrite and FileDelete actions with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{
    ActionType, CheckSpec, CheckType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget,
};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!("planner-{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for filesystem adapter actions.
pub struct PlannableFsAdapter;

#[async_trait]
impl PlannableAdapter for PlannableFsAdapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            // FileWrite: check file may or may not exist before, verify hash after
            (ActionType::FileWrite, RollbackTarget::FilePath { path, .. }) => {
                Ok(Some(ExecutionPlan {
                    prepare_checks: vec![CheckSpec {
                        check_type: CheckType::FileExists,
                        config: json_map_from_serde_map(
                            serde_json::json!({"path": path, "must_exist": false})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    }],
                    verify_checks: vec![CheckSpec {
                        check_type: CheckType::FileHashMatches,
                        config: json_map_from_serde_map(
                            serde_json::json!({"path": path})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    }],
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "fs".to_string(),
                        operation: "restore_snapshot".to_string(),
                        args: json_map_from_serde_map(
                            serde_json::json!({"path": path})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!("fs write plan for {}", path),
                }))
            }
            // FileDelete: check file exists before, verify doesn't exist after
            (ActionType::FileDelete, RollbackTarget::FilePath { path, .. }) => {
                Ok(Some(ExecutionPlan {
                    prepare_checks: vec![CheckSpec {
                        check_type: CheckType::FileExists,
                        config: json_map_from_serde_map(
                            serde_json::json!({"path": path, "must_exist": true})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    }],
                    verify_checks: vec![CheckSpec {
                        check_type: CheckType::FileExists,
                        config: json_map_from_serde_map(
                            serde_json::json!({"path": path, "must_exist": false})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    }],
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "fs".to_string(),
                        operation: "restore_snapshot".to_string(),
                        args: json_map_from_serde_map(
                            serde_json::json!({"path": path})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!("fs delete plan for {}", path),
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
    async fn test_plannable_fs_file_write_plan() {
        let adapter = PlannableFsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::FileWrite,
                &RollbackTarget::FilePath {
                    path: "/tmp/test.txt".to_string(),
                    before_hash: None,
                    after_hash: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.prepare_checks.len(), 1);
        assert_eq!(plan.verify_checks.len(), 1);
        assert_eq!(plan.compensation_plan.len(), 1);
        assert!(!plan.auto_commit);
    }

    #[tokio::test]
    async fn test_plannable_fs_file_delete_plan() {
        let adapter = PlannableFsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::FileDelete,
                &RollbackTarget::FilePath {
                    path: "/tmp/test.txt".to_string(),
                    before_hash: None,
                    after_hash: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.prepare_checks.len(), 1);
        assert!(!plan.auto_commit);
    }

    #[tokio::test]
    async fn test_plannable_fs_unknown_returns_none() {
        let adapter = PlannableFsAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GitCommit,
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
