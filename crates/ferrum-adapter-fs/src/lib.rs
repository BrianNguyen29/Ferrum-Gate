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
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

pub const ADAPTER_KIND: &str = "ferrum-adapter-fs";
pub const ADAPTER_KEY: &str = "fs";

/// Compute SHA256 hash of file content at given path.
/// Returns None if file does not exist or cannot be read.
fn compute_file_hash(path: &str) -> Result<Option<String>, AdapterError> {
    match std::fs::read(path) {
        Ok(content) => {
            let mut hasher = Sha256::new();
            hasher.update(&content);
            let hash = hex::encode(hasher.finalize());
            Ok(Some(hash))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AdapterError::Internal(format!(
            "Failed to read file for hashing: {}",
            e
        ))),
    }
}

/// In-memory store for file snapshots (compensation data)
/// Maps execution_id -> file_path -> original_content
#[derive(Default)]
pub struct FsSnapshotStore {
    snapshots: Mutex<HashMap<String, HashMap<String, Vec<u8>>>>,
    /// Maps execution_id -> file_path -> before_hash (computed at prepare time)
    before_hashes: Mutex<HashMap<String, HashMap<String, String>>>,
    /// Maps execution_id -> file_path -> after_hash (computed at execute time)
    after_hashes: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl FsSnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
            before_hashes: Mutex::new(HashMap::new()),
            after_hashes: Mutex::new(HashMap::new()),
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

    /// Store the before_hash for a file at prepare time
    fn set_before_hash(&self, execution_id: &str, path: &str, hash: String) {
        let mut before_hashes = self.before_hashes.lock().unwrap();
        let exec_hashes = before_hashes.entry(execution_id.to_string()).or_default();
        exec_hashes.insert(path.to_string(), hash);
    }

    /// Get the before_hash for a file
    fn get_before_hash(&self, execution_id: &str, path: &str) -> Option<String> {
        let before_hashes = self.before_hashes.lock().unwrap();
        before_hashes
            .get(execution_id)
            .and_then(|exec| exec.get(path).cloned())
    }

    /// Store the after_hash for a file at execute time
    fn set_after_hash(&self, execution_id: &str, path: &str, hash: String) {
        let mut after_hashes = self.after_hashes.lock().unwrap();
        let exec_hashes = after_hashes.entry(execution_id.to_string()).or_default();
        exec_hashes.insert(path.to_string(), hash);
    }

    /// Get the after_hash for a file
    fn get_after_hash(&self, execution_id: &str, path: &str) -> Option<String> {
        let after_hashes = self.after_hashes.lock().unwrap();
        after_hashes
            .get(execution_id)
            .and_then(|exec| exec.get(path).cloned())
    }

    /// Clear all hashes for an execution
    fn clear_hashes(&self, execution_id: &str) {
        let mut before_hashes = self.before_hashes.lock().unwrap();
        let mut after_hashes = self.after_hashes.lock().unwrap();
        before_hashes.remove(execution_id);
        after_hashes.remove(execution_id);
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
        // and compute before_hash for verification
        if let Some(ref path) = file_path {
            if Path::new(path).exists() {
                match std::fs::read(path) {
                    Ok(content) => {
                        self.snapshot_store.save_snapshot(
                            &request.execution_id.to_string(),
                            path,
                            content,
                        );
                        // Compute and store before_hash
                        if let Ok(Some(hash)) = compute_file_hash(path) {
                            self.snapshot_store.set_before_hash(
                                &request.execution_id.to_string(),
                                path,
                                hash,
                            );
                        }
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
        // Track action type for verify/compensate/rollback to know how to behave
        metadata.insert(
            "action_type".to_string(),
            serde_json::Value::String(format!("{:?}", request.action_type)),
        );

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

        match contract.action_type {
            ferrum_proto::ActionType::FileDelete => {
                // For FileDelete: snapshot was already taken in prepare if file existed
                // Check current state to determine if we should delete
                let existed_before = self
                    .snapshot_store
                    .file_existed_before(&contract.execution_id.to_string(), file_path);

                // Delete the file if it existed before
                if existed_before {
                    std::fs::remove_file(file_path).map_err(|e| {
                        AdapterError::Internal(format!("Failed to delete file: {}", e))
                    })?;
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
                metadata.insert(
                    "file_existed_before".to_string(),
                    serde_json::Value::Bool(existed_before),
                );

                Ok(ExecuteReceipt {
                    external_id: Some(file_path.to_string()),
                    result_digest: Some(format!("deleted:{}", existed_before)),
                    adapter_metadata: metadata,
                })
            }
            _ => {
                // FileWrite and other actions: create or overwrite file
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

                // Compute and store after_hash for verification
                let execution_id = contract.execution_id.to_string();
                if let Ok(Some(hash)) = compute_file_hash(file_path) {
                    self.snapshot_store
                        .set_after_hash(&execution_id, file_path, hash);
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );

                // Include hashes in metadata for observability
                if let Some(before_hash) = self
                    .snapshot_store
                    .get_before_hash(&execution_id, file_path)
                {
                    metadata.insert(
                        "before_hash".to_string(),
                        serde_json::Value::String(before_hash),
                    );
                }
                if let Some(after_hash) =
                    self.snapshot_store.get_after_hash(&execution_id, file_path)
                {
                    metadata.insert(
                        "after_hash".to_string(),
                        serde_json::Value::String(after_hash),
                    );
                }

                Ok(ExecuteReceipt {
                    external_id: Some(file_path.to_string()),
                    result_digest: Some(format!("written:{}", content.len())),
                    adapter_metadata: metadata,
                })
            }
        }
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let file_path = contract.metadata.get("file_path").and_then(|v| v.as_str());

        let Some(path) = file_path else {
            return Ok(VerifyReceipt {
                verified: false,
                adapter_metadata: JsonMap::new(),
            });
        };

        let execution_id = contract.execution_id.to_string();

        match contract.action_type {
            ferrum_proto::ActionType::FileDelete => {
                // For FileDelete: verify file does NOT exist (successful deletion)
                // This is meaningful verification - it confirms the delete actually happened
                let file_exists = Path::new(path).exists();
                let mut metadata = JsonMap::new();
                metadata.insert(
                    "verified_action".to_string(),
                    serde_json::Value::String("file_absent".to_string()),
                );
                metadata.insert(
                    "file_exists".to_string(),
                    serde_json::Value::Bool(file_exists),
                );
                Ok(VerifyReceipt {
                    verified: !file_exists,
                    adapter_metadata: metadata,
                })
            }
            _ => {
                // For FileWrite: verify file exists AND content hash matches after_hash
                // This is meaningful hash-based verification
                let file_exists = Path::new(path).exists();
                let mut metadata = JsonMap::new();
                metadata.insert(
                    "verified_action".to_string(),
                    serde_json::Value::String("content_hash_match".to_string()),
                );
                metadata.insert(
                    "file_exists".to_string(),
                    serde_json::Value::Bool(file_exists),
                );

                if !file_exists {
                    // File doesn't exist - verification fails
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: metadata,
                    });
                }

                // Compute current file hash
                let current_hash = match compute_file_hash(path) {
                    Ok(Some(h)) => h,
                    Ok(None) => {
                        // File disappeared between exists check and read
                        metadata.insert(
                            "hash_error".to_string(),
                            serde_json::Value::String("file_not_readable".to_string()),
                        );
                        return Ok(VerifyReceipt {
                            verified: false,
                            adapter_metadata: metadata,
                        });
                    }
                    Err(e) => {
                        // Fail closed: propagate I/O errors rather than treating as verification failure.
                        // An I/O error means we cannot determine the file state, so verification
                        // cannot succeed. This is distinct from a content mismatch (verified=false).
                        return Err(e);
                    }
                };

                metadata.insert(
                    "current_hash".to_string(),
                    serde_json::Value::String(current_hash.clone()),
                );

                // Compare with after_hash
                let expected_after_hash = self.snapshot_store.get_after_hash(&execution_id, path);
                let before_hash = self.snapshot_store.get_before_hash(&execution_id, path);

                if let Some(expected) = expected_after_hash {
                    metadata.insert(
                        "expected_after_hash".to_string(),
                        serde_json::Value::String(expected.clone()),
                    );
                    metadata.insert(
                        "hash_matches".to_string(),
                        serde_json::Value::Bool(current_hash == expected),
                    );
                    // Also include before_hash for completeness
                    if let Some(before) = before_hash {
                        metadata
                            .insert("before_hash".to_string(), serde_json::Value::String(before));
                    }
                    Ok(VerifyReceipt {
                        verified: current_hash == expected,
                        adapter_metadata: metadata,
                    })
                } else {
                    // No after_hash stored - fall back to basic existence check
                    // This shouldn't normally happen if execute was called properly
                    metadata.insert(
                        "hash_error".to_string(),
                        serde_json::Value::String("no_after_hash_stored".to_string()),
                    );
                    Ok(VerifyReceipt {
                        verified: file_exists,
                        adapter_metadata: metadata,
                    })
                }
            }
        }
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

        let mut metadata = JsonMap::new();
        let mut recovered = true;

        match contract.action_type {
            ferrum_proto::ActionType::FileDelete => {
                // For FileDelete: restore file if it existed before (has snapshot)
                if let Some(original_content) =
                    self.snapshot_store.get_snapshot(&execution_id, file_path)
                {
                    if let Err(e) = std::fs::write(file_path, original_content) {
                        // Fail closed: I/O error during recovery means we couldn't restore.
                        // Return recovered=false instead of propagating the error.
                        metadata.insert(
                            "recovery_error".to_string(),
                            serde_json::Value::String(format!("io_error: {}", e)),
                        );
                        recovered = false;
                    }
                }
                // If no snapshot, file didn't exist before delete - nothing to compensate
            }
            _ => {
                // For FileWrite and other: restore original content if we have a snapshot
                if let Some(original_content) =
                    self.snapshot_store.get_snapshot(&execution_id, file_path)
                {
                    if let Err(e) = std::fs::write(file_path, original_content) {
                        // Fail closed: I/O error during recovery means we couldn't restore.
                        // Return recovered=false instead of propagating the error.
                        metadata.insert(
                            "recovery_error".to_string(),
                            serde_json::Value::String(format!("io_error: {}", e)),
                        );
                        recovered = false;
                    }
                } else {
                    // No snapshot means file didn't exist before - delete it if it exists now
                    if Path::new(file_path).exists() {
                        if let Err(e) = std::fs::remove_file(file_path) {
                            // Fail closed: I/O error during recovery means we couldn't delete.
                            // Return recovered=false instead of propagating the error.
                            metadata.insert(
                                "recovery_error".to_string(),
                                serde_json::Value::String(format!("io_error: {}", e)),
                            );
                            recovered = false;
                        }
                    }
                }
            }
        }

        self.snapshot_store.clear_snapshots(&execution_id);
        self.snapshot_store.clear_hashes(&execution_id);

        Ok(RecoveryReceipt {
            recovered,
            adapter_metadata: metadata,
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

        let mut metadata = JsonMap::new();
        let mut recovered = true;

        match contract.action_type {
            ferrum_proto::ActionType::FileDelete => {
                // For FileDelete: restore file if it existed before (has snapshot)
                if let Some(original_content) =
                    self.snapshot_store.get_snapshot(&execution_id, file_path)
                {
                    if let Err(e) = std::fs::write(file_path, original_content) {
                        // Fail closed: I/O error during recovery means we couldn't restore.
                        // Return recovered=false instead of propagating the error.
                        metadata.insert(
                            "recovery_error".to_string(),
                            serde_json::Value::String(format!("io_error: {}", e)),
                        );
                        recovered = false;
                    }
                }
                // If no snapshot, file didn't exist before delete - nothing to rollback
            }
            _ => {
                // For FileWrite and other: rollback deletes the file ONLY if it was newly created (no snapshot)
                // If file existed before (has snapshot), we restore it
                if self
                    .snapshot_store
                    .file_existed_before(&execution_id, file_path)
                {
                    // File existed before - restore original content
                    if let Some(original_content) =
                        self.snapshot_store.get_snapshot(&execution_id, file_path)
                    {
                        if let Err(e) = std::fs::write(file_path, original_content) {
                            // Fail closed: I/O error during recovery means we couldn't restore.
                            // Return recovered=false instead of propagating the error.
                            metadata.insert(
                                "recovery_error".to_string(),
                                serde_json::Value::String(format!("io_error: {}", e)),
                            );
                            recovered = false;
                        }
                    }
                } else {
                    // File was newly created - delete it
                    if Path::new(file_path).exists() {
                        if let Err(e) = std::fs::remove_file(file_path) {
                            // Fail closed: I/O error during recovery means we couldn't delete.
                            // Return recovered=false instead of propagating the error.
                            metadata.insert(
                                "recovery_error".to_string(),
                                serde_json::Value::String(format!("io_error: {}", e)),
                            );
                            recovered = false;
                        }
                    }
                }
            }
        }

        self.snapshot_store.clear_snapshots(&execution_id);
        self.snapshot_store.clear_hashes(&execution_id);

        Ok(RecoveryReceipt {
            recovered,
            adapter_metadata: metadata,
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

    #[tokio::test]
    async fn test_fs_adapter_compensate_deletes_new_file() {
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

        // Execute (creates new file)
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

        // Compensate should delete the new file (no snapshot = file didn't exist before)
        adapter.compensate(&contract).await.unwrap();
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_adapter_delete_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create file to delete
        std::fs::write(&file_path, "content to delete").unwrap();
        assert!(file_path.exists());

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file exists - will snapshot)
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute FileDelete
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // File should be deleted
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_adapter_delete_verify_confirms_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create and then delete file
        std::fs::write(&file_path, "content").unwrap();
        assert!(file_path.exists());

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute delete
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });
        adapter.execute(&contract, &payload).await.unwrap();
        assert!(!file_path.exists());

        // Verify should confirm deletion
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_fs_adapter_delete_compensate_restores_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create file to delete
        std::fs::write(&file_path, "original content").unwrap();

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute delete
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });
        adapter.execute(&contract, &payload).await.unwrap();
        assert!(!file_path.exists());

        // Compensate should restore the file
        adapter.compensate(&contract).await.unwrap();
        assert!(file_path.exists());
        let restored = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, "original content");
    }

    #[tokio::test]
    async fn test_fs_adapter_delete_rollback_restores_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create file to delete
        std::fs::write(&file_path, "original content").unwrap();

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute delete
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });
        adapter.execute(&contract, &payload).await.unwrap();
        assert!(!file_path.exists());

        // Rollback should restore the file
        adapter.rollback(&contract).await.unwrap();
        assert!(file_path.exists());
        let restored = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, "original content");
    }

    #[tokio::test]
    async fn test_fs_adapter_delete_nonexistent_file_noop() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        // File doesn't exist
        assert!(!file_path.exists());

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file doesn't exist - no snapshot)
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute delete on non-existent file
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });
        adapter.execute(&contract, &payload).await.unwrap();

        // File should still not exist
        assert!(!file_path.exists());

        // Compensate should be a no-op (file didn't exist before)
        adapter.compensate(&contract).await.unwrap();
        assert!(!file_path.exists());

        // Rollback should be a no-op
        adapter.rollback(&contract).await.unwrap();
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_adapter_delete_verify_fails_if_file_still_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create file
        std::fs::write(&file_path, "content").unwrap();

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute delete
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });
        adapter.execute(&contract, &payload).await.unwrap();

        // Manually recreate file to simulate failed delete
        std::fs::write(&file_path, "recreated").unwrap();
        assert!(file_path.exists());

        // Verify should fail because file still exists
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(!verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_fs_adapter_hash_tracking_before_hash_stored_on_prepare() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create original file
        std::fs::write(&file_path, "original content").unwrap();

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file exists - should compute and store before_hash)
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

        adapter.prepare(&prepare_req).await.unwrap();

        // before_hash should be stored in snapshot store
        let before_hash = adapter
            .snapshot_store()
            .get_before_hash(&execution_id.to_string(), file_path.to_str().unwrap());
        assert!(
            before_hash.is_some(),
            "before_hash should be computed for existing file"
        );

        // Verify the hash is a valid hex string (SHA256 = 64 hex chars)
        let hash = before_hash.unwrap();
        assert_eq!(hash.len(), 64, "SHA256 hash should be 64 hex characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hex"
        );
    }

    #[tokio::test]
    async fn test_fs_adapter_hash_tracking_after_hash_stored_on_execute() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // File doesn't exist initially
        assert!(!file_path.exists());

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Execute (creates file)
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

        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();

        // after_hash should be stored in snapshot store
        let after_hash = adapter
            .snapshot_store()
            .get_after_hash(&execution_id.to_string(), file_path.to_str().unwrap());
        assert!(
            after_hash.is_some(),
            "after_hash should be computed after write"
        );

        // Verify the hash is a valid hex string
        let hash = after_hash.unwrap();
        assert_eq!(hash.len(), 64, "SHA256 hash should be 64 hex characters");

        // after_hash should be in execute receipt metadata for observability
        let after_hash_in_metadata = exec_receipt
            .adapter_metadata
            .get("after_hash")
            .and_then(|v| v.as_str());
        assert!(
            after_hash_in_metadata.is_some(),
            "after_hash should be in execute receipt metadata"
        );
    }

    #[tokio::test]
    async fn test_fs_adapter_verify_uses_hash_for_file_write() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // File doesn't exist initially
        assert!(!file_path.exists());

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Execute (creates file with "hello world")
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

        // Verify should pass because file content matches after_hash
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            verify_receipt.verified,
            "verify should pass when content matches after_hash"
        );

        // Verify metadata contains hash information
        let verified_action = verify_receipt
            .adapter_metadata
            .get("verified_action")
            .and_then(|v| v.as_str());
        assert_eq!(
            verified_action,
            Some("content_hash_match"),
            "verified_action should indicate hash-based verification"
        );

        let current_hash = verify_receipt
            .adapter_metadata
            .get("current_hash")
            .and_then(|v| v.as_str());
        assert!(current_hash.is_some(), "current_hash should be in metadata");

        let hash_matches = verify_receipt
            .adapter_metadata
            .get("hash_matches")
            .and_then(|v| v.as_bool());
        assert_eq!(hash_matches, Some(true), "hash_matches should be true");
    }

    #[tokio::test]
    async fn test_fs_adapter_verify_fails_when_content_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Execute (creates file with "hello world")
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

        // Manually modify the file to simulate content mismatch
        std::fs::write(&file_path, "tampered content").unwrap();

        // Verify should fail because content no longer matches after_hash
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            !verify_receipt.verified,
            "verify should fail when content doesn't match"
        );

        let hash_matches = verify_receipt
            .adapter_metadata
            .get("hash_matches")
            .and_then(|v| v.as_bool());
        assert_eq!(hash_matches, Some(false), "hash_matches should be false");
    }

    #[tokio::test]
    async fn test_fs_adapter_verify_fails_when_file_deleted_after_write() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Manually delete the file
        std::fs::remove_file(&file_path).unwrap();

        // Verify should fail because file doesn't exist
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            !verify_receipt.verified,
            "verify should fail when file doesn't exist"
        );

        let file_exists = verify_receipt
            .adapter_metadata
            .get("file_exists")
            .and_then(|v| v.as_bool());
        assert_eq!(file_exists, Some(false), "file_exists should be false");
    }

    #[tokio::test]
    async fn test_fs_adapter_verify_for_file_delete_checks_absence() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create file
        std::fs::write(&file_path, "content to delete").unwrap();
        assert!(file_path.exists());

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::FileDelete,
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

        // Execute delete
        let payload = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });
        adapter.execute(&contract, &payload).await.unwrap();
        assert!(!file_path.exists());

        // Verify should pass (file is absent as expected)
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            verify_receipt.verified,
            "verify should pass when file is absent after delete"
        );

        let verified_action = verify_receipt
            .adapter_metadata
            .get("verified_action")
            .and_then(|v| v.as_str());
        assert_eq!(
            verified_action,
            Some("file_absent"),
            "verified_action should indicate absence verification for delete"
        );
    }

    // === Fail-closed I/O error tests ===

    #[tokio::test]
    async fn test_fs_adapter_verify_fail_closed_on_io_error_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Execute (creates file)
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

        // Make file unreadable (remove read permission)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        // Verify should FAIL CLOSED: propagate I/O error rather than returning verified=false
        // This ensures that permission denied (or other I/O errors) are treated as failures
        // that prevent commit, not as "verification passed but content mismatch"
        let verify_result = adapter.verify(&contract).await;
        assert!(
            verify_result.is_err(),
            "verify should return Err on I/O error (fail closed), got: {:?}",
            verify_result
        );

        // Cleanup: restore permissions so TempDir can clean up
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    #[tokio::test]
    async fn test_fs_adapter_verify_hash_mismatch_is_verified_false_not_error() {
        // This test confirms the distinction:
        // - Content mismatch (hash doesn't match) -> verified=false (can commit)
        // - I/O error (can't read file) -> Err (fail closed, prevent commit)
        // Both return different results to allow proper handling upstream.
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Execute (creates file with "hello world")
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

        // Manually modify the file to create content mismatch
        std::fs::write(&file_path, "different content").unwrap();

        // Verify should return Ok with verified=false (NOT Err)
        // This is because we CAN read the file - the content just doesn't match
        let verify_result = adapter.verify(&contract).await;
        assert!(
            verify_result.is_ok(),
            "verify should return Ok even on content mismatch, got: {:?}",
            verify_result
        );
        let verify_receipt = verify_result.unwrap();
        assert!(
            !verify_receipt.verified,
            "verified should be false on content mismatch"
        );
    }

    #[tokio::test]
    async fn test_fs_adapter_verify_file_deleted_is_verified_false_not_error() {
        // File deletion (NotFound) should return verified=false, NOT an error.
        // This is because NotFound is a determinate state - we know the file doesn't exist.
        // Contrast with I/O errors (permission denied, disk error) which return Err (fail closed).
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
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

        // Delete the file
        std::fs::remove_file(&file_path).unwrap();

        // Verify should return Ok with verified=false (NOT Err)
        // File deletion is a determinate state we can verify - the file is gone.
        let verify_result = adapter.verify(&contract).await;
        assert!(
            verify_result.is_ok(),
            "verify should return Ok when file is deleted (NotFound), got: {:?}",
            verify_result
        );
        let verify_receipt = verify_result.unwrap();
        assert!(
            !verify_receipt.verified,
            "verified should be false when file is deleted"
        );
    }

    // === Fail-closed compensate/rollback I/O error tests ===

    #[tokio::test]
    async fn test_fs_adapter_compensate_fail_closed_on_permission_denied() {
        // Test that compensate returns recovered=false (not Err) when I/O fails
        // due to permission denied. This is fail-closed: we cannot confirm recovery
        // succeeded, so we report recovery failure.
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

        // Remove all permissions to make file unwritable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        // Compensate should return Ok with recovered=false (fail closed)
        // NOT Err - we report recovery failure, not propagate the I/O error
        let result = adapter.compensate(&contract).await;
        assert!(
            result.is_ok(),
            "compensate should return Ok even on I/O error, got: {:?}",
            result
        );
        let receipt = result.unwrap();
        assert!(
            !receipt.recovered,
            "compensate should report recovered=false on I/O failure"
        );
        // Should have error metadata
        assert!(
            receipt.adapter_metadata.contains_key("recovery_error"),
            "compensate should include recovery_error in metadata"
        );

        // Cleanup: restore permissions so TempDir can clean up
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    #[tokio::test]
    async fn test_fs_adapter_rollback_fail_closed_on_permission_denied() {
        // Test that rollback returns recovered=false (not Err) when I/O fails
        // due to permission denied. This is fail-closed: we cannot confirm recovery
        // succeeded, so we report recovery failure.
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

        // Remove all permissions to make file unwritable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        // Rollback should return Ok with recovered=false (fail closed)
        // NOT Err - we report recovery failure, not propagate the I/O error
        let result = adapter.rollback(&contract).await;
        assert!(
            result.is_ok(),
            "rollback should return Ok even on I/O error, got: {:?}",
            result
        );
        let receipt = result.unwrap();
        assert!(
            !receipt.recovered,
            "rollback should report recovered=false on I/O failure"
        );
        // Should have error metadata
        assert!(
            receipt.adapter_metadata.contains_key("recovery_error"),
            "rollback should include recovery_error in metadata"
        );

        // Cleanup: restore permissions so TempDir can clean up
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&file_path, perms);
        }
    }

    #[tokio::test]
    async fn test_fs_adapter_compensate_fail_closed_on_delete_permission_denied() {
        // Test compensate when trying to delete a newly-created file but permission denied.
        // Should return recovered=false, not Err.
        // Note: This test is environment-dependent (root can bypass permissions).
        // The primary fail-closed tests are the write/restore permission denied tests.
        // This test verifies the behavior when deletion fails.
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file doesn't exist - no snapshot)
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

        // Execute (creates new file)
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

        // Make file read-only (no write permission) - this doesn't prevent deletion
        // but on some systems, removing write permission from parent can fail deletion.
        // The key is: if deletion fails for ANY reason (permission or otherwise),
        // we should return recovered=false.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let parent = temp_dir.path();
            let mut perms = std::fs::metadata(&parent).unwrap().permissions();
            // Remove write permission from parent - deletion should fail
            let mode = perms.mode();
            perms.set_mode(mode & !0o200); // Remove write permission
            std::fs::set_permissions(&parent, perms).unwrap();
        }

        // Compensate should return Ok with recovered=false when delete fails
        let result = adapter.compensate(&contract).await;
        assert!(
            result.is_ok(),
            "compensate should return Ok even on I/O error, got: {:?}",
            result
        );
        let receipt = result.unwrap();
        // If we're running as root or in a permissive environment, deletion might succeed.
        // In that case, recovered=true is correct. Otherwise, it should be false.
        if !receipt.recovered {
            assert!(
                receipt.adapter_metadata.contains_key("recovery_error"),
                "compensate should include recovery_error in metadata when recovery fails"
            );
        }

        // Cleanup: restore permissions so TempDir can clean up
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&temp_dir.path()).unwrap().permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&temp_dir.path(), perms);
        }
    }

    #[tokio::test]
    async fn test_fs_adapter_rollback_fail_closed_on_delete_permission_denied() {
        // Test rollback when trying to delete a newly-created file but permission denied.
        // Should return recovered=false, not Err.
        // Note: This test is environment-dependent (root can bypass permissions).
        // The primary fail-closed tests are the write/restore permission denied tests.
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let adapter = FsRollbackAdapter::new("fs");
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare (file doesn't exist - no snapshot)
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

        // Execute (creates new file)
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

        // Make parent dir read-only (remove write permission)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let parent = temp_dir.path();
            let mut perms = std::fs::metadata(&parent).unwrap().permissions();
            let mode = perms.mode();
            perms.set_mode(mode & !0o200); // Remove write permission
            std::fs::set_permissions(&parent, perms).unwrap();
        }

        // Rollback should return Ok with recovered=false when delete fails
        let result = adapter.rollback(&contract).await;
        assert!(
            result.is_ok(),
            "rollback should return Ok even on I/O error, got: {:?}",
            result
        );
        let receipt = result.unwrap();
        // If we're running as root or in a permissive environment, deletion might succeed.
        // In that case, recovered=true is correct. Otherwise, it should be false.
        if !receipt.recovered {
            assert!(
                receipt.adapter_metadata.contains_key("recovery_error"),
                "rollback should include recovery_error in metadata when recovery fails"
            );
        }

        // Cleanup: restore permissions so TempDir can clean up
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&temp_dir.path()).unwrap().permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&temp_dir.path(), perms);
        }
    }
}
