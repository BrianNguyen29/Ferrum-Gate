//! Plannable adapter for Git operations.
//!
//! Generates execution plans for GitBranchCreate action with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{ActionType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!("planner-git-{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for Git adapter actions.
///
/// For GitBranchCreate + GitRef, generates an execution plan with:
/// - **prepare_checks**: None (GitBranchCreate prepare validates branch doesn't exist,
///   repo is valid, and base_ref is resolvable internally)
/// - **verify_checks**: None (GitBranchCreate verify checks branch exists and optionally
///   that branch tip matches expected SHA, done internally by adapter)
/// - **compensation_plan**: Delete the created branch using "rollback" operation.
///   The GitRollbackAdapter handles the actual branch deletion on rollback.
pub struct PlannableGitAdapter;

#[async_trait]
impl PlannableAdapter for PlannableGitAdapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            // GitBranchCreate: create a branch at a given ref
            // Compensation is branch deletion via "rollback" operation
            (
                ActionType::GitBranchCreate,
                RollbackTarget::GitRef {
                    repo_path,
                    before_ref: _,
                    after_ref: _,
                },
            ) => {
                // The branch_name comes from request metadata (set during prepare by caller).
                // The compensation_plan uses "rollback" operation which GitRollbackAdapter
                // interprets as branch deletion for GitBranchCreate action type.
                Ok(Some(ExecutionPlan {
                    prepare_checks: Vec::new(),
                    verify_checks: Vec::new(),
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "git".to_string(),
                        // GitRollbackAdapter.rollback() handles GitBranchCreate by deleting
                        // the branch whose name is in contract.metadata["branch_name"]
                        operation: "rollback".to_string(),
                        args: json_map_from_serde_map(
                            serde_json::json!({
                                "repo_path": repo_path
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!("git branch create plan for repo {}", repo_path),
                }))
            }
            // GitTagCreate: create a lightweight tag at a given commit
            (
                ActionType::GitTagCreate,
                RollbackTarget::GitRef {
                    repo_path,
                    before_ref: _,
                    after_ref: _,
                },
            ) => {
                Ok(Some(ExecutionPlan {
                    prepare_checks: Vec::new(),
                    verify_checks: Vec::new(),
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "git".to_string(),
                        // GitRollbackAdapter.rollback() handles GitTagCreate by deleting
                        // the tag whose name is in contract.metadata["tag_name"]
                        operation: "rollback".to_string(),
                        args: json_map_from_serde_map(
                            serde_json::json!({
                                "repo_path": repo_path
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!("git tag create plan for repo {}", repo_path),
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
    async fn test_plannable_git_git_branch_create_plan() {
        let adapter = PlannableGitAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GitBranchCreate,
                &RollbackTarget::GitRef {
                    repo_path: "/tmp/test-repo".to_string(),
                    before_ref: None,
                    after_ref: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.prepare_checks.len(), 0);
        assert_eq!(plan.verify_checks.len(), 0);
        assert_eq!(plan.compensation_plan.len(), 1);
        assert!(!plan.auto_commit);
        assert_eq!(plan.compensation_plan[0].adapter_key, "git");
        assert_eq!(plan.compensation_plan[0].operation, "rollback");
    }

    #[tokio::test]
    async fn test_plannable_git_git_tag_create_plan() {
        let adapter = PlannableGitAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GitTagCreate,
                &RollbackTarget::GitRef {
                    repo_path: "/tmp/test-repo".to_string(),
                    before_ref: None,
                    after_ref: None,
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.prepare_checks.len(), 0);
        assert_eq!(plan.verify_checks.len(), 0);
        assert_eq!(plan.compensation_plan.len(), 1);
        assert!(!plan.auto_commit);
        assert_eq!(plan.compensation_plan[0].adapter_key, "git");
    }

    #[tokio::test]
    async fn test_plannable_git_unknown_action_returns_none() {
        let adapter = PlannableGitAdapter;
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
        assert!(plan.is_none());
    }

    #[tokio::test]
    async fn test_plannable_git_unknown_target_returns_none() {
        let adapter = PlannableGitAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::GitBranchCreate,
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
