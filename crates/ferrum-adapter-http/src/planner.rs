//! Plannable adapter for HTTP operations.
//!
//! The HTTP planner deliberately refuses to generate auto-compensation plans
//! for HttpMutation actions. HTTP mutations cannot be safely compensated with a
//! generic replay step, so R2Compensatable is not supported for this action type.

use async_trait::async_trait;
use ferrum_proto::{ActionType, ExecutionPlan, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};

/// Planner for HTTP adapter actions.
///
/// For HttpMutation + HttpRequest, this planner deliberately returns no
/// execution plan. HTTP mutations cannot be safely compensated with a generic
/// replay step, so R2Compensatable is not supported for this action type. The
/// rollback service rejects such requests at prepare time, and the gateway
/// execute path rejects any legacy persisted R2 HTTP/SQLite contracts before
/// the adapter is invoked.
pub struct PlannableHttpAdapter;

#[async_trait]
impl PlannableAdapter for PlannableHttpAdapter {
    async fn generate_plan(
        &self,
        _action_type: &ActionType,
        _target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        // HTTP mutations are not safely compensatable generically. Do not emit
        // any auto-compensation plan; the rollback service rejects R2 for this
        // action type at prepare time.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plannable_http_http_mutation_returns_none() {
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
        assert!(
            plan.is_none(),
            "HttpMutation must not produce an auto-compensation plan"
        );
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
