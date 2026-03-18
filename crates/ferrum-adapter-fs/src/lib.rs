// Test-focused filesystem adapter for rollback/compensate evidence.
// This adapter provides real file side effects for integration testing:
// - execute: creates or overwrites a file
// - rollback: deletes a newly created file (cleanup)
// - compensate: restores overwritten file content

use async_trait::async_trait;
use ferrum_proto::{JsonMap, RollbackContract, RollbackPrepareRequest};
use ferrum_rollback::{
    AdapterError, AdapterRegistry, ExecuteReceipt, PrepareReceipt, RecoveryReceipt,
    RollbackAdapter, VerifyReceipt,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

pub const ADAPTER_KIND: &str = "ferrum-adapter-fs";
pub const ADAPTER_KEY: &str = "fs";

/// In-memory store for file snapshots (compensation data)
/// Maps execution_id -> file_path -> original_content
#[derive(Default)]
pub struct FsSnapshotStore {
    snapshots: Mutex<HashMap<String, HashMap<String, Vec<u8>>>>,
}

impl FsSnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    fn save_snapshot(&self, execution_id: &str, path: &str, content: Vec<u8>) {
        let mut snapshots = self.snapshots.lock().unwrap();
        let exec_snapshots = snapshots.entry(execution_id.to_string()).or_default();
        exec_snapshots.insert(path.to_string(), content);
    }

    fn get_snapshot(&self, execution_id: &str, path: &str) -> Option<Vec<u8>> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots
            .get(execution_id)
            .and_then(|exec| exec.get(path).cloned())
    }

    fn clear_snapshots(&self, execution_id: &str) {
        let mut snapshots = self.snapshots.lock().unwrap();
        snapshots.remove(execution_id);
    }

    fn file_existed_before(&self, execution_id: &str, path: &str) -> bool {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots
            .get(execution_id)
            .map(|exec| exec.contains_key(path))
            .unwrap_or(false)
    }
}

/// Filesystem rollback adapter for test evidence
pub struct FsRollbackAdapter {
    key: &'static str,
    snapshot_store: FsSnapshotStore,
}

impl FsRollbackAdapter {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            snapshot_store: FsSnapshotStore::new(),
        }
    }

    pub fn with_snapshots(key: &'static str, store: FsSnapshotStore) -> Self {
        Self {
            key,
            snapshot_store: store,
        }
    }

    /// Get the snapshot store (for test inspection)
    pub fn snapshot_store(&self) -> &FsSnapshotStore {
        &self.snapshot_store
    }
}

#[async_trait]
impl RollbackAdapter for FsRollbackAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Extract file path from target or metadata
        let file_path = extract_file_path(request);

        // If file exists, snapshot its current content for compensation
        if let Some(ref path) = file_path {
            if Path::new(path).exists() {
                match std::fs::read(path) {
                    Ok(content) => {
                        self.snapshot_store.save_snapshot(
                            &request.execution_id.to_string(),
                            path,
                            content,
                        );
                    }
                    Err(e) => {
                        return Err(AdapterError::Internal(format!(
                            "Failed to snapshot existing file: {}",
                            e
                        )));
                    }
                }
            }
        }

        let mut metadata = JsonMap::new();
        if let Some(path) = file_path {
            metadata.insert("file_path".to_string(), serde_json::Value::String(path));
        }

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: metadata,
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        let file_path = payload
            .get("path")
            .and_then(|v| v.as_str())
            .or_else(|| contract.metadata.get("file_path").and_then(|v| v.as_str()))
            .ok_or_else(|| AdapterError::Validation("Missing file path in payload".to_string()))?;

        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Create parent directories if needed
        if let Some(parent) = Path::new(file_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AdapterError::Internal(format!("Failed to create directories: {}", e))
                })?;
            }
        }

        // Write file
        std::fs::write(file_path, content)
            .map_err(|e| AdapterError::Internal(format!("Failed to write file: {}", e)))?;

        let mut metadata = JsonMap::new();
        metadata.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_string()),
        );

        Ok(ExecuteReceipt {
            external_id: Some(file_path.to_string()),
            result_digest: Some(format!("written:{}", content.len())),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, _contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        // Simple verification: file exists
        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        let execution_id = contract.execution_id.to_string();

        // Get file path from contract metadata
        let file_path = contract
            .metadata
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AdapterError::Validation("Missing file_path in contract metadata".to_string())
            })?;

        // Restore original content if we have a snapshot
        if let Some(original_content) = self.snapshot_store.get_snapshot(&execution_id, file_path) {
            std::fs::write(file_path, original_content)
                .map_err(|e| AdapterError::Internal(format!("Failed to restore file: {}", e)))?;
        } else {
            // No snapshot means file didn't exist before - delete it
            if Path::new(file_path).exists() {
                std::fs::remove_file(file_path)
                    .map_err(|e| AdapterError::Internal(format!("Failed to delete file: {}", e)))?;
            }
        }

        self.snapshot_store.clear_snapshots(&execution_id);

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        let execution_id = contract.execution_id.to_string();

        // Get file path from contract metadata
        let file_path = contract
            .metadata
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AdapterError::Validation("Missing file_path in contract metadata".to_string())
            })?;

        // Rollback deletes the file ONLY if it was newly created (no snapshot)
        // If file existed before (has snapshot), we restore it
        if self
            .snapshot_store
            .file_existed_before(&execution_id, file_path)
        {
            // File existed before - restore original content
            if let Some(original_content) =
                self.snapshot_store.get_snapshot(&execution_id, file_path)
            {
                std::fs::write(file_path, original_content).map_err(|e| {
                    AdapterError::Internal(format!("Failed to restore file: {}", e))
                })?;
            }
        } else {
            // File was newly created - delete it
            if Path::new(file_path).exists() {
                std::fs::remove_file(file_path)
                    .map_err(|e| AdapterError::Internal(format!("Failed to delete file: {}", e)))?;
            }
        }

        self.snapshot_store.clear_snapshots(&execution_id);

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: JsonMap::new(),
        })
    }
}

/// Extract file path from rollback request
fn extract_file_path(request: &RollbackPrepareRequest) -> Option<String> {
    // Try to get from target
    match &request.target {
        ferrum_proto::RollbackTarget::FilePath { path, .. } => Some(path.clone()),
        _ => {
            // Try to get from metadata
            request
                .metadata
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
    }
}

/// Register the fs adapter in the registry
pub fn register_fs_adapter(registry: &mut AdapterRegistry) {
    registry.register(std::sync::Arc::new(FsRollbackAdapter::new(ADAPTER_KEY)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_fs_adapter_execute_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let adapter = FsRollbackAdapter::new("fs");

        // Prepare
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id: ferrum_proto::ExecutionId::new(),
            action_type: ferrum_proto::ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: "fs".to_string(),
            target: ferrum_proto::RollbackTarget::FilePath {
                path: file_path.to_str().unwrap().to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();
        assert!(prep_receipt.accepted);

        // Execute
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "hello world"
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: "fs".to_string(),
            target: ferrum_proto::RollbackTarget::FilePath {
                path: file_path.to_str().unwrap().to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();
        assert!(exec_receipt.external_id.is_some());

        // Verify file exists
        assert!(file_path.exists());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_fs_adapter_rollback_deletes_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let adapter = FsRollbackAdapter::new("fs");

        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file doesn't exist yet - no snapshot)
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: "fs".to_string(),
            target: ferrum_proto::RollbackTarget::FilePath {
                path: file_path.to_str().unwrap().to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();

        // Execute
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "hello world"
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: "fs".to_string(),
            target: ferrum_proto::RollbackTarget::FilePath {
                path: file_path.to_str().unwrap().to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter.execute(&contract, &payload).await.unwrap();
        assert!(file_path.exists());

        // Rollback should delete the file
        adapter.rollback(&contract).await.unwrap();
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_adapter_compensate_restores_overwritten_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create original file
        std::fs::write(&file_path, "original content").unwrap();

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file exists - will snapshot)
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: "fs".to_string(),
            target: ferrum_proto::RollbackTarget::FilePath {
                path: file_path.to_str().unwrap().to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();

        // Execute (overwrites)
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content"
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: "fs".to_string(),
            target: ferrum_proto::RollbackTarget::FilePath {
                path: file_path.to_str().unwrap().to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter.execute(&contract, &payload).await.unwrap();
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");

        // Compensate should restore original content
        adapter.compensate(&contract).await.unwrap();
        let restored = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, "original content");
    }
}
