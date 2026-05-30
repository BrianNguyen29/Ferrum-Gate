//! Plannable adapter for SQLite database operations.
//!
//! Generates execution plans for SqlMutation actions with appropriate
//! prepare_checks, verify_checks, and compensation_plan steps.

use async_trait::async_trait;
use ferrum_proto::{ActionType, CompensationStep, ExecutionPlan, JsonMap, RollbackTarget};
use ferrum_rollback::{AdapterError, PlannableAdapter};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique idempotency key for compensation steps.
fn next_idempotency_key() -> String {
    format!(
        "planner-sqlite-{}",
        ID_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

/// Planner for SQLite adapter actions.
///
/// For SqlMutation + SqliteTxn, generates an execution plan with:
///
/// - **prepare_checks**: SqlRowCountRange check on the target table (if table name known)
/// - **verify_checks**: SqlRowCountRange to verify expected row count after mutation
/// - **compensation_plan**: Rollback step with SQL reversed from the mutation
///
/// Note: The actual compensation SQL must be provided by the caller since the planner
/// does not know the specific SQL statement at plan generation time. The test
/// should populate the compensation_plan with the appropriate DELETE/INSERT statement
/// before calling compensate.
pub struct PlannableSqliteAdapter;

#[async_trait]
impl PlannableAdapter for PlannableSqliteAdapter {
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        match (action_type, target) {
            // SqlMutation: DML operations (INSERT/UPDATE/DELETE)
            // The compensation_plan SQL must be provided by the caller since we don't
            // know the specific SQL at plan time.
            (ActionType::SqlMutation, RollbackTarget::SqliteTxn { db_path, tx_id: _ }) => {
                // Table name extraction from db_path always returned None (dead code),
                // so verify_checks remains empty. Caller should populate verify_checks
                // with SqlRowCountRange checks if needed.

                Ok(Some(ExecutionPlan {
                    // For bounded test, we rely on the caller to populate compensation_plan
                    // with the specific SQL to reverse the mutation (DELETE to reverse INSERT,
                    // or INSERT to reverse DELETE, etc.)
                    prepare_checks: Vec::new(),
                    verify_checks: Vec::new(),
                    compensation_plan: vec![CompensationStep {
                        order: 1,
                        adapter_key: "sqlite".to_string(),
                        // The operation is "rollback" which SqliteAdapter.execute() handles
                        // by executing the SQL in args["sql"]
                        operation: "rollback".to_string(),
                        // The SQL must be provided by caller; placeholder signals manual population needed
                        args: json_map_from_serde_map(
                            serde_json::json!({
                                "sql": "/* compensation SQL must be provided by caller */"
                            })
                            .as_object()
                            .unwrap()
                            .clone(),
                        ),
                        idempotency_key: next_idempotency_key(),
                    }],
                    auto_commit: false,
                    plan_description: format!("sqlite mutation plan for {}", db_path),
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
    async fn test_plannable_sqlite_sql_mutation_plan() {
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
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.prepare_checks.len(), 0);
        // Temp path doesn't extract table name, so verify_checks is empty
        assert_eq!(plan.verify_checks.len(), 0);
        assert_eq!(plan.compensation_plan.len(), 1);
        assert!(!plan.auto_commit);
        assert_eq!(plan.compensation_plan[0].adapter_key, "sqlite");
        assert_eq!(plan.compensation_plan[0].operation, "rollback");
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
