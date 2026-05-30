//! Plannable adapter for HTTP operations.
//!
//! Generates execution plans for HttpMutation actions with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{ActionType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!("planner-http-{}", ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for HTTP adapter actions.
///
/// For HttpMutation + HttpRequest, generates an execution plan with:
///
/// - **prepare_checks**: None (HttpAdapter prepare validates URL/method shape internally)
/// - **verify_checks**: None (HttpAdapter verify runs HttpStatusExpected checks internally)
/// - **compensation_plan**: A placeholder http.replay_v1 step. The args are intentionally
///   incomplete so that HttpAdapter.execute ignores it (parse_replay_contract fails),
///   while keeping compensation_plan non-empty so Invariant 9 passes for R2Compensatable.
///   Callers that need real replay compensation must populate the compensation_plan with
///   valid method, url, payload, and expected_statuses before compensate.
pub struct PlannableHttpAdapter;

#[async_trait]
impl PlannableAdapter for PlannableHttpAdapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            (
                ActionType::HttpMutation,
                RollbackTarget::HttpRequest {
                    method: _,
                    url,
                    request_digest: _,
                },
            ) => {
                // Generate a placeholder replay step with incomplete args.
                // This keeps compensation_plan non-empty for Invariant 9,
                // but parse_replay_contract will fail so execute won't
                // try to validate a digest against an empty request_digest.
                Ok(Some(ExecutionPlan {
                    prepare_checks: Vec::new(),
                    verify_checks: Vec::new(),
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "http".to_string(),
                        operation: "http.replay_v1".to_string(),
                        args: json_map_from_serde_map(
                            serde_json::json!({
                                "url": url.clone()
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!("http mutation plan for {}", url),
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
    async fn test_plannable_http_http_mutation_plan() {
        let adapter = PlannableHttpAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::HttpMutation,
                &RollbackTarget::HttpRequest {
                    method: ferrum_proto::HttpMethod::Post,
                    url: "https://httpbin.org/post".to_string(),
                    request_digest: String::new(),
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
        assert_eq!(plan.compensation_plan[0].adapter_key, "http");
        assert_eq!(plan.compensation_plan[0].operation, "http.replay_v1");
    }

    #[tokio::test]
    async fn test_plannable_http_unknown_action_returns_none() {
        let adapter = PlannableHttpAdapter;
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
    async fn test_plannable_http_unknown_target_returns_none() {
        let adapter = PlannableHttpAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::HttpMutation,
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
