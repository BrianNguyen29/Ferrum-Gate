//! Plannable adapter for MailDraft email operations.
//!
//! Generates execution plans for MailDraft actions with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{ActionType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!(
        "planner-maildraft-{}",
        ID_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for MailDraft adapter actions.
///
/// For MailDraft + EmailDraft, generates an execution plan with:
///
/// - **prepare_checks**: None (MailDraft prepare validates draft existence/uniqueness internally)
/// - **verify_checks**: None (MailDraft verify confirms draft state internally)
/// - **compensation_plan**: Operation is set to the metadata["operation"] value ("delete" to reverse create,
///   "recreate_delete" to reverse delete). The caller should set proper operation in metadata
///   before compensate.
pub struct PlannableMailDraftAdapter;

#[async_trait]
impl PlannableAdapter for PlannableMailDraftAdapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            (ActionType::MailDraft, RollbackTarget::EmailDraft { draft_id, .. }) => {
                // MailDraft adapter handles rollback internally based on operation in metadata.
                // The compensation_plan has a placeholder operation that caller should override.
                Ok(Some(ExecutionPlan {
                    prepare_checks: Vec::new(),
                    verify_checks: Vec::new(),
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "maildraft".to_string(),
                        // Operation should be "delete" (to reverse create) or "recreate_delete"
                        // (to reverse delete). Caller sets this in metadata before compensate.
                        operation: "rollback".to_string(),
                        args: json_map_from_serde_map(
                            serde_json::json!({
                                "draft_id": draft_id.clone().unwrap_or_else(|| "unknown".to_string())
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!(
                        "maildraft plan for draft {}",
                        draft_id.clone().unwrap_or_else(|| "unknown".to_string())
                    ),
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
    async fn test_plannable_maildraft_email_draft_plan() {
        let adapter = PlannableMailDraftAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::MailDraft,
                &RollbackTarget::EmailDraft {
                    draft_id: Some("test-draft".to_string()),
                    recipients: vec!["recipient@example.com".to_string()],
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
        assert_eq!(plan.compensation_plan[0].adapter_key, "maildraft");
        assert_eq!(plan.compensation_plan[0].operation, "rollback");
    }

    #[tokio::test]
    async fn test_plannable_maildraft_unknown_action_returns_none() {
        let adapter = PlannableMailDraftAdapter;
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
    async fn test_plannable_maildraft_unknown_target_returns_none() {
        let adapter = PlannableMailDraftAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::MailDraft,
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
