// Test-focused maildraft adapter for EmailDraft rollback/compensate evidence.
// This adapter provides draft artifact management for integration testing:
// - execute: creates a draft artifact with draft_id
// - rollback/compensate: deletes the draft artifact
// Fail-closed: rejects send payloads (send semantics out of scope)

use async_trait::async_trait;
#[cfg(test)]
use ferrum_proto::RollbackTarget;
use ferrum_proto::{JsonMap, RollbackContract, RollbackPrepareRequest};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const ADAPTER_KIND: &str = "ferrum-adapter-maildraft";
pub const ADAPTER_KEY: &str = "maildraft";

/// In-memory store for draft artifacts
/// Maps execution_id -> draft artifact
#[derive(Clone)]
pub struct MaildraftStore {
    /// Maps draft_id -> draft content (recipients, subject, body)
    drafts: Arc<Mutex<HashMap<String, DraftArtifact>>>,
    /// Maps execution_id -> draft_id
    execution_to_draft: Arc<Mutex<HashMap<String, String>>>,
}

impl Default for MaildraftStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct DraftArtifact {
    pub draft_id: String,
    pub recipients: Vec<String>,
    pub subject: String,
    pub body: String,
}

impl MaildraftStore {
    pub fn new() -> Self {
        Self {
            drafts: Arc::new(Mutex::new(HashMap::new())),
            execution_to_draft: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Save a draft artifact, mapping execution_id to draft_id
    pub fn save_draft(&self, execution_id: &str, draft: DraftArtifact) {
        let draft_id = draft.draft_id.clone();
        let mut drafts = self.drafts.lock().unwrap();
        drafts.insert(draft_id.clone(), draft);

        let mut exec_to_draft = self.execution_to_draft.lock().unwrap();
        exec_to_draft.insert(execution_id.to_string(), draft_id);
    }

    /// Get draft_id for an execution
    pub fn get_draft_id(&self, execution_id: &str) -> Option<String> {
        let exec_to_draft = self.execution_to_draft.lock().unwrap();
        exec_to_draft.get(execution_id).cloned()
    }

    /// Get draft artifact by draft_id
    pub fn get_draft(&self, draft_id: &str) -> Option<DraftArtifact> {
        let drafts = self.drafts.lock().unwrap();
        drafts.get(draft_id).cloned()
    }

    /// Check if draft exists
    pub fn draft_exists(&self, draft_id: &str) -> bool {
        let drafts = self.drafts.lock().unwrap();
        drafts.contains_key(draft_id)
    }

    /// Delete draft artifact
    pub fn delete_draft(&self, draft_id: &str) -> bool {
        let mut drafts = self.drafts.lock().unwrap();
        drafts.remove(draft_id).is_some()
    }

    /// Delete draft by execution_id
    pub fn delete_draft_by_execution(&self, execution_id: &str) -> Option<String> {
        let exec_to_draft = self.execution_to_draft.lock().unwrap();
        let draft_id = exec_to_draft.get(execution_id).cloned();
        drop(exec_to_draft);

        if let Some(draft_id) = draft_id {
            let mut drafts = self.drafts.lock().unwrap();
            drafts.remove(&draft_id);

            let mut exec_to_draft = self.execution_to_draft.lock().unwrap();
            exec_to_draft.remove(execution_id);

            return Some(draft_id);
        }
        None
    }

    /// Clear all data for an execution
    pub fn clear(&self, execution_id: &str) {
        let mut exec_to_draft = self.execution_to_draft.lock().unwrap();
        if let Some(draft_id) = exec_to_draft.get(execution_id) {
            let mut drafts = self.drafts.lock().unwrap();
            drafts.remove(draft_id);
        }
        exec_to_draft.remove(execution_id);
    }
}

/// Maildraft rollback adapter for test evidence
pub struct MaildraftAdapter {
    key: &'static str,
    store: MaildraftStore,
}

impl MaildraftAdapter {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            store: MaildraftStore::new(),
        }
    }

    pub fn with_store(key: &'static str, store: MaildraftStore) -> Self {
        Self { key, store }
    }

    /// Get the draft store (for test inspection)
    pub fn store(&self) -> &MaildraftStore {
        &self.store
    }
}

#[async_trait]
impl RollbackAdapter for MaildraftAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        _request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // No special preparation needed for draft creation
        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        // Fail-closed: reject send payloads (send semantics out of scope)
        if let Some(send) = payload.get("send").and_then(|v| v.as_bool()) {
            if send {
                return Err(AdapterError::Validation(
                    "maildraft adapter: send semantics out of scope, rejecting send payload"
                        .to_string(),
                ));
            }
        }

        // Extract draft creation fields
        let recipients: Vec<String> = payload
            .get("to")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let subject = payload
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let body = payload
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Generate draft_id
        let draft_id = format!("draft-{}", uuid::Uuid::new_v4());

        let artifact = DraftArtifact {
            draft_id: draft_id.clone(),
            recipients: recipients.clone(),
            subject: subject.clone(),
            body: body.clone(),
        };

        // Store the draft artifact
        self.store
            .save_draft(&contract.execution_id.to_string(), artifact);

        let mut metadata = JsonMap::new();
        metadata.insert("draft_id".to_string(), serde_json::json!(draft_id));

        Ok(ExecuteReceipt {
            external_id: Some(draft_id.clone()),
            result_digest: Some(format!("draft:{}", recipients.len())),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, _contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        // Drafts are immediately created, no verification needed
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

        // Delete the draft artifact
        if let Some(draft_id) = self.store.delete_draft_by_execution(&execution_id) {
            let mut metadata = JsonMap::new();
            metadata.insert("deleted_draft_id".to_string(), serde_json::json!(draft_id));

            Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            })
        } else {
            // No draft to compensate (shouldn't happen in normal flow)
            Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: JsonMap::new(),
            })
        }
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // Rollback and compensate are the same for draft creation - delete the draft
        self.compensate(contract).await
    }
}

/// Register the maildraft adapter in the registry
pub fn register_maildraft_adapter(registry: &mut ferrum_rollback::AdapterRegistry) {
    registry.register(std::sync::Arc::new(MaildraftAdapter::new(ADAPTER_KEY)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_maildraft_adapter_execute_creates_draft() {
        let adapter = MaildraftAdapter::new(ADAPTER_KEY);
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare
        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::EmailDraft {
                draft_id: None,
                recipients: vec!["alice@example.com".to_string()],
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();
        assert!(prep_receipt.accepted);

        // Execute (draft creation, not send)
        let payload = serde_json::json!({
            "to": ["alice@example.com", "bob@example.com"],
            "subject": "Test email",
            "body": "Hello!"
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: prepare_req.target.clone(),
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
        let draft_id = exec_receipt.external_id.unwrap();

        // Verify draft exists
        assert!(adapter.store.draft_exists(&draft_id));
    }

    #[tokio::test]
    async fn test_maildraft_adapter_rejects_send_payload() {
        let adapter = MaildraftAdapter::new(ADAPTER_KEY);
        let execution_id = ferrum_proto::ExecutionId::new();

        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::EmailDraft {
                draft_id: None,
                recipients: vec!["alice@example.com".to_string()],
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();

        // Execute with send=true (should fail)
        let payload = serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test",
            "body": "Hello!",
            "send": true
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: prepare_req.target.clone(),
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("send semantics out of scope"));
    }

    #[tokio::test]
    async fn test_maildraft_adapter_rollback_deletes_draft() {
        let adapter = MaildraftAdapter::new(ADAPTER_KEY);
        let execution_id = ferrum_proto::ExecutionId::new();

        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::EmailDraft {
                draft_id: None,
                recipients: vec!["alice@example.com".to_string()],
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();

        let payload = serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test",
            "body": "Hello!"
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: prepare_req.target.clone(),
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
        let draft_id = exec_receipt.external_id.unwrap();
        assert!(adapter.store.draft_exists(&draft_id));

        // Rollback should delete the draft
        adapter.rollback(&contract).await.unwrap();
        assert!(!adapter.store.draft_exists(&draft_id));
    }

    #[tokio::test]
    async fn test_maildraft_adapter_compensate_deletes_draft() {
        let adapter = MaildraftAdapter::new(ADAPTER_KEY);
        let execution_id = ferrum_proto::ExecutionId::new();

        let prepare_req = RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::EmailDraft {
                draft_id: None,
                recipients: vec!["alice@example.com".to_string()],
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let prep_receipt = adapter.prepare(&prepare_req).await.unwrap();

        let payload = serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test",
            "body": "Hello!"
        });

        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: prepare_req.target.clone(),
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
        let draft_id = exec_receipt.external_id.unwrap();
        assert!(adapter.store.draft_exists(&draft_id));

        // Compensate should delete the draft
        adapter.compensate(&contract).await.unwrap();
        assert!(!adapter.store.draft_exists(&draft_id));
    }
}
