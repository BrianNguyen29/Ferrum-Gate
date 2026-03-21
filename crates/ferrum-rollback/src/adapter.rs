use async_trait::async_trait;
use ferrum_proto::{JsonMap, RollbackContract, RollbackPrepareRequest};
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

#[derive(Debug, Clone)]
pub struct RecoveryReceipt {
    pub recovered: bool,
    pub adapter_metadata: JsonMap,
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
        self.adapters.insert(adapter.key().to_string(), adapter);
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn RollbackAdapter>> {
        self.adapters.get(key).cloned()
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
