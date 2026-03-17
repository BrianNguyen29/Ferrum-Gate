use anyhow::Context;
use chrono::Utc;
use ferrum_proto::{
    ExecutionId, RollbackContract, RollbackPrepareRequest, RollbackPrepareResponse, RollbackState,
    RollbackContractId,
};
use std::sync::Arc;

use crate::{AdapterError, AdapterRegistry};

pub struct RollbackService {
    registry: Arc<AdapterRegistry>,
}

impl RollbackService {
    pub fn new(registry: Arc<AdapterRegistry>) -> Self {
        Self { registry }
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
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: request.intent_id,
            proposal_id: request.proposal_id,
            execution_id: request.execution_id,
            action_type: request.action_type,
            rollback_class: request.rollback_class,
            adapter_key: request.adapter_key,
            target: request.target,
            prepare_checks: request.prepare_checks,
            verify_checks: request.verify_checks,
            compensation_plan: request.compensation_plan,
            auto_commit: request.auto_commit,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: request.metadata,
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

    pub async fn compensate(&self, contract: &RollbackContract) -> anyhow::Result<()> {
        let adapter = self
            .registry
            .get(&contract.adapter_key)
            .context("adapter not registered")?;
        adapter.compensate(contract).await.map_err(map_adapter_err)?;
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
        let auto_commit = !matches!(
            requested_rollback_class,
            ferrum_proto::RollbackClass::R3IrreversibleHighConsequence
        );

        RollbackPrepareRequest {
            intent_id,
            proposal_id,
            execution_id,
            action_type: ferrum_proto::ActionType::McpToolMutation,
            rollback_class: requested_rollback_class,
            adapter_key: "noop".to_string(),
            target: ferrum_proto::RollbackTarget::Generic {
                namespace: "mcp".to_string(),
                identifier: "tool-call".to_string(),
            },
            prepare_checks: Vec::new(),
            verify_checks: Vec::new(),
            compensation_plan: Vec::new(),
            auto_commit,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }
}

fn map_adapter_err(err: AdapterError) -> anyhow::Error {
    anyhow::anyhow!(err.to_string())
}
