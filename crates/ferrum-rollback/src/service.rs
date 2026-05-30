use anyhow::Context;
use chrono::Utc;
use ferrum_proto::{
    ExecutionId, RollbackContract, RollbackContractId, RollbackPrepareRequest,
    RollbackPrepareResponse, RollbackState,
};
use std::sync::Arc;

use crate::{AdapterError, AdapterRegistry, ExecuteReceipt, PlannableAdapter};

pub struct RollbackService {
    registry: Arc<AdapterRegistry>,
    planners: Vec<Arc<dyn PlannableAdapter>>,
}

impl RollbackService {
    pub fn new(registry: Arc<AdapterRegistry>) -> Self {
        Self {
            registry,
            planners: Vec::new(),
        }
    }

    pub fn register_planner(&mut self, planner: Arc<dyn PlannableAdapter>) {
        self.planners.push(planner);
    }

    pub async fn prepare(
        &self,
        request: RollbackPrepareRequest,
    ) -> anyhow::Result<RollbackPrepareResponse> {
        let adapter = self
            .registry
            .get(&request.adapter_key)
            .context("adapter not registered")?;

        let receipt = adapter.prepare(&request).await.map_err(map_adapter_err)?;

        // Determine final checks and plan: auto-fill from planners if manual plan is empty
        let (final_prepare_checks, final_verify_checks, final_compensation_plan, final_auto_commit) =
            if request.prepare_checks.is_empty()
                && request.verify_checks.is_empty()
                && request.compensation_plan.is_empty()
            {
                // Try auto-planning from registered planners
                let mut plan_found = None;
                for planner in &self.planners {
                    if let Ok(Some(plan)) = planner
                        .generate_plan(&request.action_type, &request.target)
                        .await
                    {
                        plan_found = Some(plan);
                        break;
                    }
                }
                match plan_found {
                    Some(plan) => (
                        plan.prepare_checks,
                        plan.verify_checks,
                        plan.compensation_plan,
                        plan.auto_commit,
                    ),
                    None => (
                        request.prepare_checks,
                        request.verify_checks,
                        request.compensation_plan,
                        request.auto_commit,
                    ),
                }
            } else {
                (
                    request.prepare_checks,
                    request.verify_checks,
                    request.compensation_plan,
                    request.auto_commit,
                )
            };

        // Invariant 9: R2 must have a compensation plan; reject if empty after planner fallback
        if matches!(
            request.rollback_class,
            ferrum_proto::RollbackClass::R2Compensatable
        ) && final_compensation_plan.is_empty()
        {
            anyhow::bail!(
                "R2Compensatable requires a non-empty compensation plan but none was provided"
            );
        }

        // Merge adapter receipt metadata (snapshot paths, created_new_file flags, etc.)
        // so compensate can find what it needs. Adapter prepare runs first and captures
        // filesystem state; we preserve that in the contract so the compensate path works.
        let mut merged_metadata = request.metadata.clone();
        for (k, v) in receipt.adapter_metadata.iter() {
            merged_metadata.insert(k.clone(), v.clone());
        }

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: request.intent_id,
            proposal_id: request.proposal_id,
            execution_id: request.execution_id,
            action_type: request.action_type,
            rollback_class: request.rollback_class,
            adapter_key: request.adapter_key,
            target: request.target,
            prepare_checks: final_prepare_checks,
            verify_checks: final_verify_checks,
            compensation_plan: final_compensation_plan,
            auto_commit: final_auto_commit,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: merged_metadata,
        };

        Ok(RollbackPrepareResponse {
            contract,
            accepted: receipt.accepted,
            warnings: Vec::new(),
        })
    }

    pub async fn verify(&self, contract: &RollbackContract) -> anyhow::Result<bool> {
        let adapter = self
            .registry
            .get(&contract.adapter_key)
            .context("adapter not registered")?;
        let receipt = adapter.verify(contract).await.map_err(map_adapter_err)?;
        Ok(receipt.verified)
    }

    /// Execute the contract action with the given payload.
    /// This calls the adapter's execute method with the contract and payload.
    pub async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> anyhow::Result<ExecuteReceipt> {
        let adapter = self
            .registry
            .get(&contract.adapter_key)
            .context("adapter not registered")?;
        let receipt = adapter
            .execute(contract, payload)
            .await
            .map_err(map_adapter_err)?;
        Ok(receipt)
    }

    pub async fn compensate(&self, contract: &RollbackContract) -> anyhow::Result<()> {
        let adapter = self
            .registry
            .get(&contract.adapter_key)
            .context("adapter not registered")?;
        adapter
            .compensate(contract)
            .await
            .map_err(map_adapter_err)?;
        Ok(())
    }

    pub async fn rollback(&self, contract: &RollbackContract) -> anyhow::Result<()> {
        let adapter = self
            .registry
            .get(&contract.adapter_key)
            .context("adapter not registered")?;
        adapter.rollback(contract).await.map_err(map_adapter_err)?;
        Ok(())
    }

    pub fn default_prepare_request(
        &self,
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
        execution_id: ExecutionId,
        requested_rollback_class: ferrum_proto::RollbackClass,
    ) -> RollbackPrepareRequest {
        self.build_prepare_request(
            intent_id,
            proposal_id,
            execution_id,
            requested_rollback_class,
            ferrum_proto::ActionType::McpToolMutation,
            "noop".to_string(),
        )
    }

    /// Build a prepare request with explicit action_type and adapter_key.
    /// This allows callers (e.g., the gateway) to specify the adapter directly
    /// based on their context (e.g., inferring fs adapter for FileWrite tools).
    pub fn build_prepare_request(
        &self,
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
        execution_id: ExecutionId,
        requested_rollback_class: ferrum_proto::RollbackClass,
        action_type: ferrum_proto::ActionType,
        adapter_key: String,
    ) -> RollbackPrepareRequest {
        self.build_prepare_request_with_target(
            intent_id,
            proposal_id,
            execution_id,
            requested_rollback_class,
            action_type,
            adapter_key,
            ferrum_proto::RollbackTarget::Generic {
                namespace: "mcp".to_string(),
                identifier: "tool-call".to_string(),
            },
        )
    }

    /// Build a prepare request with explicit action_type, adapter_key, and target.
    /// This allows callers (e.g., the gateway) to specify all parameters directly.
    pub fn build_prepare_request_with_target(
        &self,
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
        execution_id: ExecutionId,
        requested_rollback_class: ferrum_proto::RollbackClass,
        action_type: ferrum_proto::ActionType,
        adapter_key: String,
        target: ferrum_proto::RollbackTarget,
    ) -> RollbackPrepareRequest {
        let is_irreversible_high = matches!(
            requested_rollback_class,
            ferrum_proto::RollbackClass::R3IrreversibleHighConsequence
        );
        // R3 enforcement: irreversible-high-consequence actions must NEVER auto-commit.
        // auto_commit=false ensures verify will not silently commit; an explicit
        // commit flow is required. This is a hard invariant — do not override.
        RollbackPrepareRequest {
            intent_id,
            proposal_id,
            execution_id,
            action_type,
            rollback_class: requested_rollback_class,
            adapter_key,
            target,
            prepare_checks: Vec::new(),
            verify_checks: Vec::new(),
            compensation_plan: Vec::new(),
            auto_commit: !is_irreversible_high,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }
}

fn map_adapter_err(err: AdapterError) -> anyhow::Error {
    anyhow::anyhow!(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ferrum_proto::{
        ActionType, CheckSpec, CheckType, ExecutionPlan, JsonMap, RollbackClass, RollbackTarget,
    };
    use std::sync::Arc;

    struct FakePlannableAdapter {
        should_plan: bool,
    }

    #[async_trait]
    impl PlannableAdapter for FakePlannableAdapter {
        async fn generate_plan(
            &self,
            _action_type: &ActionType,
            _target: &RollbackTarget,
        ) -> Result<Option<ExecutionPlan>, AdapterError> {
            if self.should_plan {
                Ok(Some(ExecutionPlan {
                    prepare_checks: vec![CheckSpec {
                        check_type: CheckType::FileExists,
                        config: JsonMap::new(),
                    }],
                    verify_checks: vec![],
                    compensation_plan: vec![],
                    auto_commit: true,
                    plan_description: "fake plan".to_string(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    fn make_request_with_empty_plan() -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileWrite,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "noop".to_string(),
            target: RollbackTarget::FilePath {
                path: "/test.txt".to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: Vec::new(),
            verify_checks: Vec::new(),
            compensation_plan: Vec::new(),
            auto_commit: true,
            metadata: JsonMap::new(),
        }
    }

    fn make_request_with_manual_plan() -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileWrite,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "noop".to_string(),
            target: RollbackTarget::FilePath {
                path: "/test.txt".to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![CheckSpec {
                check_type: CheckType::FileHashMatches,
                config: JsonMap::new(),
            }],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        }
    }

    fn make_test_service_with_adapter() -> (RollbackService, Arc<AdapterRegistry>) {
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(crate::NoopRollbackAdapter::new("noop")));
        let registry = Arc::new(registry);
        let service = RollbackService::new(registry.clone());
        (service, registry)
    }

    #[tokio::test]
    async fn test_prepare_uses_manual_plan_when_provided() {
        let (mut service, _registry) = make_test_service_with_adapter();

        // Register a planner that would generate a plan
        service.register_planner(Arc::new(FakePlannableAdapter { should_plan: true }));

        let request = make_request_with_manual_plan();
        let response = service.prepare(request).await.unwrap();

        // Manual plan should be used, not the planner's plan
        assert_eq!(response.contract.prepare_checks.len(), 1);
        assert!(matches!(
            response.contract.prepare_checks[0].check_type,
            CheckType::FileHashMatches
        ));
        assert!(!response.contract.auto_commit);
    }

    #[tokio::test]
    async fn test_prepare_auto_fills_from_planner_when_empty() {
        let (mut service, _registry) = make_test_service_with_adapter();

        // Register a planner that generates a plan
        service.register_planner(Arc::new(FakePlannableAdapter { should_plan: true }));

        let request = make_request_with_empty_plan();
        let response = service.prepare(request).await.unwrap();

        // Planner's plan should be used since manual plan was empty
        assert_eq!(response.contract.prepare_checks.len(), 1);
        assert!(matches!(
            response.contract.prepare_checks[0].check_type,
            CheckType::FileExists
        ));
        assert!(response.contract.auto_commit);
    }

    #[tokio::test]
    async fn test_prepare_no_planner_falls_back_to_empty() {
        let (service, _registry) = make_test_service_with_adapter();

        // No planners registered - service has empty planners list

        let request = make_request_with_empty_plan();
        let response = service.prepare(request).await.unwrap();

        // Should fall back to empty plan (preserve empty checks)
        assert!(response.contract.prepare_checks.is_empty());
        assert!(response.contract.verify_checks.is_empty());
        assert!(response.contract.compensation_plan.is_empty());
        assert!(response.contract.auto_commit); // from original request
    }

    #[tokio::test]
    async fn test_register_planner_adds_to_list() {
        let mut service = RollbackService::new(Arc::new(AdapterRegistry::default()));

        assert_eq!(service.planners.len(), 0);

        service.register_planner(Arc::new(FakePlannableAdapter { should_plan: false }));
        assert_eq!(service.planners.len(), 1);

        service.register_planner(Arc::new(FakePlannableAdapter { should_plan: true }));
        assert_eq!(service.planners.len(), 2);
    }

    // -------------------------------------------------------------------------
    // Invariant 9: R2 has compensation plan
    // -------------------------------------------------------------------------

    /// Invariant 9: R2 without a planner should fail with empty compensation plan.
    #[tokio::test]
    async fn test_invariant9_r2_empty_plan_without_planner_fails() {
        let (service, _registry) = make_test_service_with_adapter();
        // No planners registered - service has empty planners list

        let mut request = make_request_with_empty_plan();
        request.rollback_class = ferrum_proto::RollbackClass::R2Compensatable;
        request.compensation_plan = Vec::new(); // explicit empty plan
        request.auto_commit = false; // R2 typically requires manual commit

        let result = service.prepare(request).await;
        // Invariant 9: R2 must have compensation plan, so empty should fail
        assert!(
            result.is_err(),
            "R2 without planner should fail with empty compensation plan"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("compensation plan"),
            "Error should mention compensation plan: {}",
            err
        );
    }

    /// Invariant 9: R2 with a planner that provides a compensation plan should succeed.
    #[tokio::test]
    async fn test_invariant9_r2_with_planner_succeeds() {
        let (mut service, _registry) = make_test_service_with_adapter();

        // Register a planner that generates a plan with compensation steps
        struct FakeR2Planner;
        #[async_trait]
        impl PlannableAdapter for FakeR2Planner {
            async fn generate_plan(
                &self,
                _action_type: &ActionType,
                _target: &RollbackTarget,
            ) -> Result<Option<ExecutionPlan>, AdapterError> {
                let mut args = JsonMap::new();
                args.insert("path".to_string(), serde_json::json!("/tmp/test.txt"));
                Ok(Some(ExecutionPlan {
                    prepare_checks: vec![CheckSpec {
                        check_type: CheckType::FileExists,
                        config: JsonMap::new(),
                    }],
                    verify_checks: vec![],
                    compensation_plan: vec![ferrum_proto::CompensationStep {
                        order: 1,
                        adapter_key: "noop".to_string(),
                        operation: "delete".to_string(),
                        args,
                        idempotency_key: "step-1".to_string(),
                    }],
                    auto_commit: false,
                    plan_description: "R2 plan with compensation".to_string(),
                }))
            }
        }

        service.register_planner(Arc::new(FakeR2Planner));

        let request = make_request_with_empty_plan();
        let request = RollbackPrepareRequest {
            intent_id: request.intent_id,
            proposal_id: request.proposal_id,
            execution_id: request.execution_id,
            action_type: request.action_type,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: request.adapter_key,
            target: request.target,
            prepare_checks: request.prepare_checks,
            verify_checks: request.verify_checks,
            compensation_plan: request.compensation_plan,
            auto_commit: request.auto_commit,
            metadata: request.metadata,
        };

        let response = service.prepare(request).await.unwrap();

        // With a planner that generates a plan, the contract should have it
        assert!(
            !response.contract.compensation_plan.is_empty(),
            "R2 with planner should have non-empty compensation_plan"
        );
        assert!(
            !response.contract.auto_commit,
            "R2 should have auto_commit=false"
        );
    }

    /// Invariant 9: R0 should not be affected by compensation plan requirement.
    #[tokio::test]
    async fn test_invariant9_r0_not_affected() {
        let (service, _registry) = make_test_service_with_adapter();

        let mut request = make_request_with_empty_plan();
        request.rollback_class = ferrum_proto::RollbackClass::R0NativeReversible;
        request.compensation_plan = Vec::new(); // empty, but R0 doesn't need compensation

        let response = service.prepare(request).await.unwrap();
        // R0 should succeed even with empty compensation plan
        assert!(response.contract.compensation_plan.is_empty());
        assert!(response.contract.auto_commit); // R0 typically auto-commits
    }

    /// Invariant 9: R3 should not be affected by compensation plan requirement
    /// (R3 is about irreversibility, not compensation). R3 with empty compensation
    /// plan should succeed since Invariant 9 only applies to R2.
    #[tokio::test]
    async fn test_invariant9_r3_not_affected() {
        let (service, _registry) = make_test_service_with_adapter();

        let mut request = make_request_with_empty_plan();
        request.rollback_class = ferrum_proto::RollbackClass::R3IrreversibleHighConsequence;
        request.compensation_plan = Vec::new(); // empty, R3 doesn't use compensation
        request.auto_commit = false; // R3 must not auto-commit (enforced at prepare)

        let response = service.prepare(request).await.unwrap();
        // R3 should succeed even with empty compensation plan (Invariant 9 applies to R2 only)
        assert!(response.contract.compensation_plan.is_empty());
        // R3 prepare sets auto_commit=false; verify must honor it and suppress SideEffectCommitted.
        assert!(!response.contract.auto_commit);
    }
}
