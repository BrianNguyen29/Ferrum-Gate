// Maildraft adapter for EmailDraft rollback/compensate evidence.
// This adapter provides draft artifact management:
// - execute: creates a draft artifact with draft_id in SQLite
// - rollback/compensate: deletes the draft artifact from SQLite
// - verify: checks that draft exists in SQLite (durable persistence)
// Fail-closed: rejects send payloads (send semantics out of scope)

use async_trait::async_trait;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(test)]
use ferrum_proto::RollbackTarget;
use ferrum_proto::{CheckSpec, CheckType, JsonMap, RollbackContract, RollbackPrepareRequest};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};

pub const ADAPTER_KIND: &str = "ferrum-adapter-maildraft";
pub const ADAPTER_KEY: &str = "maildraft";

/// Draft artifact stored in SQLite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftArtifact {
    pub draft_id: String,
    pub execution_id: String,
    pub recipients: Vec<String>,
    pub subject: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl DraftArtifact {
    pub fn new(
        draft_id: String,
        execution_id: String,
        recipients: Vec<String>,
        subject: String,
        body: String,
    ) -> Self {
        Self {
            draft_id,
            execution_id,
            recipients,
            subject,
            body,
            created_at: chrono::Utc::now(),
        }
    }
}

/// SQLite-backed store for draft artifacts
pub struct SqliteMaildraftStore {
    conn: Arc<Mutex<Connection>>,
}

/// Backwards-compatible alias for SqliteMaildraftStore.
/// The MaildraftStore name is used in integration tests and existing code.
pub type MaildraftStore = SqliteMaildraftStore;

impl SqliteMaildraftStore {
    /// Create a new in-memory SQLite store (for testing)
    pub fn new_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS maildraft_drafts (
                draft_id TEXT PRIMARY KEY,
                execution_id TEXT NOT NULL,
                recipients TEXT NOT NULL,
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_maildraft_execution_id ON maildraft_drafts(execution_id);",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create a new SQLite store backed by a file
    pub fn new_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS maildraft_drafts (
                draft_id TEXT PRIMARY KEY,
                execution_id TEXT NOT NULL,
                recipients TEXT NOT NULL,
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_maildraft_execution_id ON maildraft_drafts(execution_id);",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create a new in-memory SQLite store (panics on error).
    /// This is the backwards-compatible API used by existing tests.
    pub fn new() -> Self {
        Self::new_in_memory().expect("failed to create maildraft store")
    }

    /// Save a draft artifact
    pub fn save_draft(&self, draft: &DraftArtifact) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let recipients_json = serde_json::to_string(&draft.recipients)?;
        conn.execute(
            "INSERT OR REPLACE INTO maildraft_drafts 
             (draft_id, execution_id, recipients, subject, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                draft.draft_id,
                draft.execution_id,
                recipients_json,
                draft.subject,
                draft.body,
                draft.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get draft by draft_id
    pub fn get_draft(&self, draft_id: &str) -> anyhow::Result<Option<DraftArtifact>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT draft_id, execution_id, recipients, subject, body, created_at 
             FROM maildraft_drafts WHERE draft_id = ?1",
        )?;
        let mut rows = stmt.query(params![draft_id])?;
        if let Some(row) = rows.next()? {
            let recipients_json: String = row.get(2)?;
            let created_at_str: String = row.get(5)?;
            Ok(Some(DraftArtifact {
                draft_id: row.get(0)?,
                execution_id: row.get(1)?,
                recipients: serde_json::from_str(&recipients_json).unwrap_or_default(),
                subject: row.get(3)?,
                body: row.get(4)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get draft_id for an execution
    pub fn get_draft_id_by_execution(&self, execution_id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT draft_id FROM maildraft_drafts WHERE execution_id = ?1")?;
        let mut rows = stmt.query(params![execution_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Check if draft exists by draft_id (backwards-compatible API returns bool)
    pub fn draft_exists(&self, draft_id: &str) -> bool {
        // For backwards compatibility, we panic on error (database errors won't happen for in-memory SQLite)
        self.draft_exists_check(draft_id)
            .expect("database error checking draft existence")
    }

    /// Internal check that returns Result (used by verify method)
    pub(crate) fn draft_exists_check(&self, draft_id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT 1 FROM maildraft_drafts WHERE draft_id = ?1")?;
        let mut rows = stmt.query(params![draft_id])?;
        Ok(rows.next()?.is_some())
    }

    /// Delete draft by draft_id
    pub fn delete_draft(&self, draft_id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "DELETE FROM maildraft_drafts WHERE draft_id = ?1",
            params![draft_id],
        )?;
        Ok(affected > 0)
    }

    /// Delete draft by execution_id
    pub fn delete_draft_by_execution(&self, execution_id: &str) -> anyhow::Result<Option<String>> {
        if let Some(draft_id) = self.get_draft_id_by_execution(execution_id)? {
            self.delete_draft(&draft_id)?;
            Ok(Some(draft_id))
        } else {
            Ok(None)
        }
    }

    /// Clear all drafts for an execution (used for rollback)
    pub fn clear(&self, execution_id: &str) -> anyhow::Result<()> {
        let _ = self.delete_draft_by_execution(execution_id);
        Ok(())
    }
}

impl Clone for SqliteMaildraftStore {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

impl Default for SqliteMaildraftStore {
    fn default() -> Self {
        Self::new_in_memory().expect("failed to create default maildraft store")
    }
}

/// Maildraft rollback adapter with durable SQLite persistence
pub struct MaildraftAdapter {
    key: &'static str,
    store: SqliteMaildraftStore,
}

impl MaildraftAdapter {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            store: SqliteMaildraftStore::new_in_memory().expect("failed to create maildraft store"),
        }
    }

    pub fn with_store(key: &'static str, store: SqliteMaildraftStore) -> Self {
        Self { key, store }
    }

    /// Get the draft store (for test inspection)
    pub fn store(&self) -> &SqliteMaildraftStore {
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
        let execution_id = contract.execution_id.to_string();

        let artifact = DraftArtifact::new(
            draft_id.clone(),
            execution_id,
            recipients.clone(),
            subject.clone(),
            body.clone(),
        );

        // Store the draft artifact durably in SQLite
        self.store
            .save_draft(&artifact)
            .map_err(|e| AdapterError::Internal(format!("failed to save draft: {}", e)))?;

        let mut metadata = JsonMap::new();
        metadata.insert("draft_id".to_string(), serde_json::json!(draft_id));

        Ok(ExecuteReceipt {
            external_id: Some(draft_id.clone()),
            result_digest: Some(format!("draft:{}", recipients.len())),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        // Verify checks that the draft was actually persisted and can be retrieved.
        // Explicit EmailDraftExists checks are honored: if provided, the check's draft_id
        // is used; otherwise fall back to metadata lookup.
        let execution_id = contract.execution_id.to_string();

        // Extract explicit draft_id from verify_checks if EmailDraftExists is present
        let explicit_draft_id =
            MaildraftAdapter::extract_expected_draft_id(&contract.verify_checks)
                .map(|s| s.to_string());

        // Track if explicit check was requested (before we consume explicit_draft_id)
        let has_explicit_check = explicit_draft_id.is_some();

        // Get draft_id: explicit check takes precedence, then metadata, then lookup
        let draft_id = explicit_draft_id
            .or_else(|| {
                contract
                    .metadata
                    .get("draft_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                // Fallback: look up by execution_id
                self.store
                    .get_draft_id_by_execution(&execution_id)
                    .ok()
                    .flatten()
            });

        let draft_id = match draft_id {
            Some(id) => id,
            None => {
                // No draft found for this execution - verification fails
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: JsonMap::new(),
                });
            }
        };

        // Check that draft exists in SQLite store
        match self.store.draft_exists_check(&draft_id) {
            Ok(true) => {
                let mut metadata = JsonMap::new();
                metadata.insert("draft_id".to_string(), serde_json::json!(draft_id));
                metadata.insert(
                    "verified_at".to_string(),
                    serde_json::json!(chrono::Utc::now().to_rfc3339()),
                );
                // Record if this was an explicit check
                if has_explicit_check {
                    metadata.insert(
                        "explicit_check".to_string(),
                        serde_json::json!("EmailDraftExists"),
                    );
                }
                Ok(VerifyReceipt {
                    verified: true,
                    adapter_metadata: metadata,
                })
            }
            Ok(false) => {
                // Draft was expected but doesn't exist - verification fails
                Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: JsonMap::new(),
                })
            }
            Err(e) => {
                // Database error - verification fails closed
                Err(AdapterError::Internal(format!(
                    "verify failed to check draft existence: {}",
                    e
                )))
            }
        }
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        let execution_id = contract.execution_id.to_string();

        // Delete the draft artifact from SQLite
        match self.store.delete_draft_by_execution(&execution_id) {
            Ok(Some(draft_id)) => {
                let mut metadata = JsonMap::new();
                metadata.insert("deleted_draft_id".to_string(), serde_json::json!(draft_id));
                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                })
            }
            Ok(None) => {
                // No draft to compensate (shouldn't happen in normal flow)
                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: JsonMap::new(),
                })
            }
            Err(e) => Err(AdapterError::Internal(format!(
                "compensate failed to delete draft: {}",
                e
            ))),
        }
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // Rollback and compensate are the same for draft creation - delete the draft
        self.compensate(contract).await
    }
}

impl MaildraftAdapter {
    /// Extract expected draft_id from verify_checks if EmailDraftExists is present.
    fn extract_expected_draft_id(checks: &[CheckSpec]) -> Option<String> {
        for check in checks {
            if matches!(check.check_type, CheckType::EmailDraftExists) {
                if let Some(draft_id) = check.config.get("draft_id") {
                    return draft_id.as_str().map(|s| s.to_string());
                }
            }
        }
        None
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
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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

        // Verify draft exists in store
        assert!(adapter.store().draft_exists(&draft_id));

        // Verify draft can be retrieved
        let draft = adapter.store().get_draft(&draft_id).unwrap().unwrap();
        assert_eq!(draft.draft_id, draft_id);
        assert_eq!(draft.recipients.len(), 2);
    }

    #[tokio::test]
    async fn test_maildraft_adapter_rejects_send_payload() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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

        let _prep_receipt = adapter.prepare(&prepare_req).await.unwrap();

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
            metadata: JsonMap::new(),
        };

        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("send semantics out of scope"));
    }

    #[tokio::test]
    async fn test_maildraft_adapter_rollback_deletes_draft() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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
        assert!(adapter.store().draft_exists(&draft_id));

        // Rollback should delete the draft
        adapter.rollback(&contract).await.unwrap();
        assert!(!adapter.store().draft_exists(&draft_id));
    }

    #[tokio::test]
    async fn test_maildraft_adapter_compensate_deletes_draft() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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
        assert!(adapter.store().draft_exists(&draft_id));

        // Compensate should delete the draft
        adapter.compensate(&contract).await.unwrap();
        assert!(!adapter.store().draft_exists(&draft_id));
    }

    #[tokio::test]
    async fn test_maildraft_adapter_verify_returns_true_for_existing_draft() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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

        adapter.execute(&contract, &payload).await.unwrap();

        // Verify should return true for existing draft
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_maildraft_adapter_verify_returns_false_for_missing_draft() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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

        adapter.execute(&contract, &payload).await.unwrap();

        // Delete the draft manually to simulate missing draft
        let draft_id = adapter
            .store()
            .get_draft_id_by_execution(&execution_id.to_string())
            .unwrap()
            .unwrap();
        adapter.store().delete_draft(&draft_id).unwrap();

        // Verify should return false for missing draft
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(!verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_maildraft_adapter_persistence_across_adapter_restart() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_drafts.sqlite");

        // Create store and adapter with file-backed SQLite
        let store1 = SqliteMaildraftStore::new_from_file(&db_path).unwrap();
        let adapter1 = MaildraftAdapter::with_store(ADAPTER_KEY, store1);
        let execution_id = ferrum_proto::ExecutionId::new();

        // Prepare and execute
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

        let prep_receipt = adapter1.prepare(&prepare_req).await.unwrap();

        let payload = serde_json::json!({
            "to": ["alice@example.com", "bob@example.com"],
            "subject": "Persistence test",
            "body": "This should persist!"
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

        let exec_receipt = adapter1.execute(&contract, &payload).await.unwrap();
        let draft_id = exec_receipt.external_id.unwrap();

        // Verify draft exists in first adapter
        assert!(adapter1.store().draft_exists(&draft_id));

        // Drop first adapter
        drop(adapter1);

        // Create new adapter with same file-backed store
        let store2 = SqliteMaildraftStore::new_from_file(&db_path).unwrap();
        let adapter2 = MaildraftAdapter::with_store(ADAPTER_KEY, store2);

        // Draft should still exist in persisted store
        assert!(adapter2.store().draft_exists(&draft_id));

        // Should be able to retrieve the draft
        let draft = adapter2.store().get_draft(&draft_id).unwrap().unwrap();
        assert_eq!(draft.subject, "Persistence test");
        assert_eq!(draft.recipients.len(), 2);
    }

    #[tokio::test]
    async fn test_maildraft_adapter_verify_returns_false_for_nonexistent_execution() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
        let execution_id = ferrum_proto::ExecutionId::new();

        // Contract with an execution that was never executed
        let contract = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
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
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        };

        // Verify should return false for non-existent execution
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(!verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_maildraft_adapter_verify_with_explicit_email_draft_exists_check_passes() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        let exec_receipt = adapter.execute(&contract, &payload).await.unwrap();
        let draft_id = exec_receipt.external_id.unwrap();

        // Now verify with an explicit EmailDraftExists check
        let explicit_check = CheckSpec {
            check_type: CheckType::EmailDraftExists,
            config: {
                let mut m = JsonMap::new();
                m.insert("draft_id".to_string(), serde_json::json!(draft_id));
                m
            },
        };

        let contract_with_explicit_check = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: prepare_req.target.clone(),
            prepare_checks: vec![],
            verify_checks: vec![explicit_check],
            compensation_plan: vec![],
            auto_commit: false,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let verify_receipt = adapter.verify(&contract_with_explicit_check).await.unwrap();
        assert!(verify_receipt.verified);
        // Verify that explicit_check metadata was recorded
        assert_eq!(
            verify_receipt
                .adapter_metadata
                .get("explicit_check")
                .and_then(|v| v.as_str()),
            Some("EmailDraftExists")
        );
    }

    #[tokio::test]
    async fn test_maildraft_adapter_verify_with_explicit_email_draft_exists_check_fails() {
        let store = SqliteMaildraftStore::new_in_memory().unwrap();
        let adapter = MaildraftAdapter::with_store(ADAPTER_KEY, store);
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
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        adapter.execute(&contract, &payload).await.unwrap();

        // Verify with an explicit EmailDraftExists check for a DIFFERENT draft_id
        let nonexistent_draft_id = "nonexistent-draft-12345";
        let explicit_check = CheckSpec {
            check_type: CheckType::EmailDraftExists,
            config: {
                let mut m = JsonMap::new();
                m.insert(
                    "draft_id".to_string(),
                    serde_json::json!(nonexistent_draft_id),
                );
                m
            },
        };

        let contract_with_explicit_check = RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: prepare_req.intent_id,
            proposal_id: prepare_req.proposal_id,
            execution_id: prepare_req.execution_id,
            action_type: ferrum_proto::ActionType::EmailDraftCreate,
            rollback_class: ferrum_proto::RollbackClass::R2Compensatable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: prepare_req.target.clone(),
            prepare_checks: vec![],
            verify_checks: vec![explicit_check],
            compensation_plan: vec![],
            auto_commit: false,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Verify should return false because the explicit draft_id doesn't exist
        let verify_receipt = adapter.verify(&contract_with_explicit_check).await.unwrap();
        assert!(!verify_receipt.verified);
    }
}
