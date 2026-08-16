use async_trait::async_trait;
use ferrum_proto::{
    ActionType, ExecutionPlan, JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget,
};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("unsupported action: {0}")]
    Unsupported(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("duplicate adapter key: {0}")]
    DuplicateKey(String),
}

#[derive(Debug, Clone)]
pub struct PrepareReceipt {
    pub accepted: bool,
    pub adapter_metadata: JsonMap,
}

#[derive(Debug, Clone)]
pub struct ExecuteReceipt {
    pub external_id: Option<String>,
    pub result_digest: Option<String>,
    pub adapter_metadata: JsonMap,
}

#[derive(Debug, Clone)]
pub struct VerifyReceipt {
    pub verified: bool,
    pub adapter_metadata: JsonMap,
}

/// Tri-state outcome of a verification attempt.
///
/// - `Applied`: the side effect was conclusively applied.
/// - `NotApplied`: the side effect was conclusively not applied (e.g. the
///   mutation was rolled back by the target system or never reached it).
/// - `Indeterminate`: the adapter cannot determine whether the side effect
///   took place; recovery is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    Applied,
    NotApplied,
    Indeterminate,
}

/// Receipt returned by adapters that implement tri-state verification.
#[derive(Debug, Clone)]
pub struct VerifyOutcomeReceipt {
    pub outcome: VerifyOutcome,
    pub adapter_metadata: JsonMap,
}

#[derive(Debug, Clone)]
pub struct RecoveryReceipt {
    pub recovered: bool,
    pub adapter_metadata: JsonMap,
}

impl VerifyReceipt {
    /// Compatibility mapping used by the default `verify_with_outcome`:
    /// `true` means the side effect was applied, `false` means the outcome
    /// is indeterminate (not a conclusive failure).
    pub fn outcome(&self) -> VerifyOutcome {
        if self.verified {
            VerifyOutcome::Applied
        } else {
            VerifyOutcome::Indeterminate
        }
    }
}

#[async_trait]
pub trait RollbackAdapter: Send + Sync {
    fn key(&self) -> &'static str;

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError>;
    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError>;
    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError>;

    /// Tri-state verification hook. The default implementation delegates to
    /// the legacy boolean `verify` and maps `true` -> `Applied` and `false` ->
    /// `Indeterminate`, preserving existing adapters without forcing them to
    /// change constructors in this slice. Adapters that can conclusively report
    /// `NotApplied` should override this method.
    async fn verify_with_outcome(
        &self,
        contract: &RollbackContract,
    ) -> Result<VerifyOutcomeReceipt, AdapterError> {
        let receipt = self.verify(contract).await?;
        Ok(VerifyOutcomeReceipt {
            outcome: receipt.outcome(),
            adapter_metadata: receipt.adapter_metadata,
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError>;
    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError>;
}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn RollbackAdapter>>,
}

impl AdapterRegistry {
    pub fn register(&mut self, adapter: Arc<dyn RollbackAdapter>) {
        let key = adapter.key().to_string();
        if self.adapters.contains_key(&key) {
            panic!("duplicate adapter key registered: {}", key);
        }
        self.adapters.insert(key, adapter);
    }

    pub fn try_register(&mut self, adapter: Arc<dyn RollbackAdapter>) -> Result<(), AdapterError> {
        let key = adapter.key().to_string();
        if self.adapters.contains_key(&key) {
            return Err(AdapterError::DuplicateKey(key));
        }
        self.adapters.insert(key, adapter);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn RollbackAdapter>> {
        self.adapters.get(key).cloned()
    }

    pub fn keys(&self) -> Vec<&str> {
        self.adapters.keys().map(|k| k.as_str()).collect()
    }
}

pub struct NoopRollbackAdapter {
    key: &'static str,
}

impl NoopRollbackAdapter {
    pub fn new(key: &'static str) -> Self {
        Self { key }
    }
}

#[async_trait]
impl RollbackAdapter for NoopRollbackAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        _request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn execute(
        &self,
        _contract: &RollbackContract,
        _payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        Ok(ExecuteReceipt {
            external_id: None,
            result_digest: Some("noop-execution".to_string()),
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn verify(&self, _contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        _contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn rollback(
        &self,
        _contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: JsonMap::new(),
        })
    }
}

/// A no-op PlannableAdapter that generates minimal plans for any target.
pub struct PlannableNoopAdapter;

#[async_trait]
impl PlannableAdapter for PlannableNoopAdapter {
    async fn generate_plan(
        &self,
        _action_type: &ActionType,
        _target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError> {
        Ok(Some(ExecutionPlan {
            prepare_checks: Vec::new(),
            verify_checks: Vec::new(),
            compensation_plan: Vec::new(),
            auto_commit: true,
            plan_description: "noop auto-plan: no checks generated".to_string(),
        }))
    }
}

/// Extension trait for adapters that can auto-generate execution plans.
///
/// A PlannableAdapter knows how to derive prepare_checks, verify_checks,
/// and compensation_plan from an ActionType + RollbackTarget, reducing
/// the need for manual rollback contract authoring.
#[async_trait]
pub trait PlannableAdapter: Send + Sync {
    /// Generate an execution plan for the given action type and target.
    ///
    /// Returns None if the adapter cannot plan for this combination
    /// (caller should fall back to manual plan or empty plan).
    async fn generate_plan(
        &self,
        action_type: &ActionType,
        target: &RollbackTarget,
    ) -> Result<Option<ExecutionPlan>, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_plan_instantiation() {
        let plan = ExecutionPlan {
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: true,
            plan_description: "test plan".to_string(),
        };
        assert!(plan.auto_commit);
        assert!(plan.prepare_checks.is_empty());
    }

    #[test]
    fn test_plannable_adapter_is_object_safe() {
        // Verify the trait is object-safe by creating a Box<dyn PlannableAdapter>
        fn _assert_object_safe(_: Box<dyn PlannableAdapter>) {}
        // If this compiles, the trait is object-safe
    }

    #[tokio::test]
    async fn test_plannable_noop_generates_plan() {
        let adapter = PlannableNoopAdapter;
        let plan = adapter
            .generate_plan(
                &ActionType::FileWrite,
                &RollbackTarget::Generic {
                    namespace: "test".to_string(),
                    identifier: "test-id".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.auto_commit);
        assert!(plan.prepare_checks.is_empty());
    }

    #[test]
    #[should_panic(expected = "duplicate adapter key registered: noop")]
    fn test_adapter_registry_duplicate_panics() {
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    }

    #[test]
    fn test_adapter_registry_try_register_duplicate_returns_error() {
        let mut registry = AdapterRegistry::default();
        registry
            .try_register(Arc::new(NoopRollbackAdapter::new("noop")))
            .unwrap();
        let result = registry.try_register(Arc::new(NoopRollbackAdapter::new("noop")));
        assert!(matches!(result, Err(AdapterError::DuplicateKey(ref k)) if k == "noop"));
    }

    #[test]
    fn test_adapter_registry_multiple_distinct_keys() {
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        registry.register(Arc::new(NoopRollbackAdapter::new("fs")));
        registry.register(Arc::new(NoopRollbackAdapter::new("http")));

        assert!(registry.get("noop").is_some());
        assert!(registry.get("fs").is_some());
        assert!(registry.get("http").is_some());
        assert!(registry.get("missing").is_none());

        let mut keys = registry.keys();
        keys.sort_unstable();
        assert_eq!(keys, vec!["fs", "http", "noop"]);
    }
}
