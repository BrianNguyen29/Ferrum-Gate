//! Plannable adapter for SQLite database operations.
//!
//! The SQLite planner deliberately refuses to generate auto-compensation plans
//! for SqlMutation actions. SQLite DML mutations cannot be safely compensated
//! with a generic rollback step, so R2Compensatable is not supported for this
//! action type.

use async_trait::async_trait;
use ferrum_proto::{ActionType, ExecutionPlan, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};

/// Planner for SQLite adapter actions.
///
/// For SqlMutation + SqliteTxn, this planner deliberately returns no execution
/// plan. SQLite DML mutations cannot be safely compensated with a generic
/// rollback step, so R2Compensatable is not supported for this action type. The
/// rollback service rejects such requests at prepare time, and the gateway
/// execute path rejects any legacy persisted R2 HTTP/SQLite contracts before
/// the adapter is invoked.
pub struct PlannableSqliteAdapter;

#[async_trait]
impl PlannableAdapter for PlannableSqliteAdapter {
    async fn generate_plan(
        &self,
        _action_type: &ActionType,
        _target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        // SQLite DML mutations are not safely compensatable generically. Do not
        // emit any auto-compensation plan; the rollback service rejects R2 for
        // this action type at prepare time.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plannable_sqlite_sql_mutation_returns_none() {
        let adapter = PlannableSqliteAdapter;
        // Use a path that doesn't trigger table extraction (temp path)
        let plan = adapter
            .generate_plan(
                &ActionType::SqlMutation,
                &RollbackTarget::SqliteTxn {
                    db_path: "/tmp/test.db".to_string(),
                    tx_id: "test-tx".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(
            plan.is_none(),
            "SqlMutation must not produce an auto-compensation plan"
        );
    }

    #[tokio::test]
    async fn test_plannable_sqlite_unknown_action_returns_none() {
        let adapter = PlannableSqliteAdapter;
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
    async fn test_plannable_sqlite_unknown_target_returns_none() {
        let adapter = PlannableSqliteAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::SqlMutation,
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
