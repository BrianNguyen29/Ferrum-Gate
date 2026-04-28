//! MailDraft adapter for email draft management with rollback support.
//!
//! This adapter implements the `RollbackAdapter` trait for email draft operations,
//! supporting create, update, and delete operations on drafts stored in memory.
//!
//! # Operations
//!
//! - **Create**: prepare validates draft_id is new, execute stores, verify confirms exists, rollback deletes
//! - **Update**: prepare captures original draft, execute overwrites, verify confirms new content, rollback restores original
//! - **Delete**: prepare captures draft content, execute removes, verify confirms gone, rollback recreates
//!
//! The operation type is determined by `request.metadata["operation"]` ("create", "update", "delete").

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrum_proto::{ActionType, JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub const ADAPTER_KIND: &str = "ferrum-adapter-maildraft";

/// Phase context for error normalization.
const PHASE_PREPARE: &str = "prepare";
const PHASE_VERIFY: &str = "verify";
const PHASE_EXECUTE: &str = "execute";
const PHASE_ROLLBACK: &str = "rollback";
#[allow(dead_code)]
const PHASE_COMPENSATE: &str = "compensate";

/// Supported operation types for MailDraft adapter.
const OP_CREATE: &str = "create";
const OP_UPDATE: &str = "update";
const OP_DELETE: &str = "delete";

#[derive(Debug, Error)]
pub enum MailDraftAdapterError {
    #[error("invalid target: expected EmailDraft, got {0}")]
    InvalidTarget(String),
    #[error("draft not found: {0}")]
    DraftNotFound(String),
    #[error("draft already exists: {0}")]
    DraftAlreadyExists(String),
    #[error("unsupported action type: {0}")]
    UnsupportedAction(String),
    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<MailDraftAdapterError> for AdapterError {
    fn from(err: MailDraftAdapterError) -> Self {
        match err {
            MailDraftAdapterError::InvalidTarget(msg) => AdapterError::Validation(msg),
            MailDraftAdapterError::DraftNotFound(msg) => AdapterError::Validation(msg),
            MailDraftAdapterError::DraftAlreadyExists(msg) => AdapterError::Validation(msg),
            MailDraftAdapterError::UnsupportedAction(msg) => AdapterError::Unsupported(msg),
            MailDraftAdapterError::UnsupportedOperation(msg) => AdapterError::Unsupported(msg),
            MailDraftAdapterError::Validation(msg) => AdapterError::Validation(msg),
            MailDraftAdapterError::Internal(msg) => AdapterError::Internal(msg),
        }
    }
}

/// Email draft data structure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmailDraft {
    pub id: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Thread-safe in-memory store for email drafts.
#[derive(Default)]
pub struct DraftStore {
    drafts: HashMap<String, EmailDraft>,
}

impl DraftStore {
    fn new() -> Self {
        Self {
            drafts: HashMap::new(),
        }
    }

    fn get(&self, id: &str) -> Option<EmailDraft> {
        self.drafts.get(id).cloned()
    }

    fn insert(&mut self, draft: EmailDraft) {
        self.drafts.insert(draft.id.clone(), draft);
    }

    fn remove(&mut self, id: &str) -> Option<EmailDraft> {
        self.drafts.remove(id)
    }

    fn contains(&self, id: &str) -> bool {
        self.drafts.contains_key(id)
    }
}

/// MailDraft adapter implementing the RollbackAdapter trait.
///
/// Uses in-memory storage to provide prepare→verify lifecycle testing
/// with snapshot-based recovery for bounded Create, Update, and Delete operations.
pub struct MailDraftAdapter {
    key: &'static str,
    store: Arc<Mutex<DraftStore>>,
}

impl MailDraftAdapter {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            store: Arc::new(Mutex::new(DraftStore::new())),
        }
    }

    /// Creates a new MailDraftAdapter with a shared store (useful for testing).
    pub fn with_store(key: &'static str, store: Arc<Mutex<DraftStore>>) -> Self {
        Self { key, store }
    }

    /// Extracts the draft_id from a RollbackTarget::EmailDraft variant.
    fn extract_draft_id(target: &RollbackTarget) -> Result<String, AdapterError> {
        match target {
            RollbackTarget::EmailDraft { draft_id, .. } => draft_id.clone().ok_or_else(|| {
                AdapterError::Validation("draft_id is required in EmailDraft target".into())
            }),
            _ => Err(AdapterError::Validation(format!(
                "invalid target: expected EmailDraft, got {:?}",
                target
            ))),
        }
    }

    /// Gets the operation type from metadata.
    fn get_operation(metadata: &JsonMap) -> Result<&str, AdapterError> {
        metadata
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AdapterError::Validation(
                    "operation is required in metadata (create, update, delete)".into(),
                )
            })
    }

    /// Validates required draft fields.
    fn validate_draft_fields(
        draft_id: &str,
        from: &str,
        to: &[String],
        subject: &str,
        body: &str,
    ) -> Result<(), AdapterError> {
        if draft_id.is_empty() {
            return Err(AdapterError::Validation("draft_id cannot be empty".into()));
        }
        if from.is_empty() {
            return Err(AdapterError::Validation("from cannot be empty".into()));
        }
        if to.is_empty() {
            return Err(AdapterError::Validation("to cannot be empty".into()));
        }
        for (i, recipient) in to.iter().enumerate() {
            if recipient.is_empty() {
                return Err(AdapterError::Validation(format!(
                    "to[{}] cannot be empty",
                    i
                )));
            }
        }
        if subject.is_empty() {
            return Err(AdapterError::Validation("subject cannot be empty".into()));
        }
        if body.is_empty() {
            return Err(AdapterError::Validation("body cannot be empty".into()));
        }
        Ok(())
    }

    /// Normalizes an internal error with phase context.
    fn phase_wrap_internal(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Internal(format!("[{}] {}", phase, msg))
    }

    /// Normalizes a validation error with phase context.
    fn phase_wrap_validation(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Validation(format!("[{}] {}", phase, msg))
    }
}

#[async_trait]
impl RollbackAdapter for MailDraftAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate that target is EmailDraft
        let draft_id = Self::extract_draft_id(&request.target)?;

        // Validate that action_type is MailDraft
        if !matches!(request.action_type, ActionType::MailDraft) {
            return Err(AdapterError::Unsupported(format!(
                "unsupported action type: {:?}",
                request.action_type
            )));
        }

        // Get operation type from metadata
        let operation = Self::get_operation(&request.metadata)?;

        // Validate operation is supported
        if !matches!(operation, OP_CREATE | OP_UPDATE | OP_DELETE) {
            return Err(AdapterError::Unsupported(format!(
                "unsupported operation: {} (expected create, update, or delete)",
                operation
            )));
        }

        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "prepared_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "operation".to_string(),
            serde_json::Value::String(operation.to_string()),
        );
        metadata.insert(
            "draft_id".to_string(),
            serde_json::Value::String(draft_id.clone()),
        );

        let store = self.store.lock().map_err(|e| {
            Self::phase_wrap_internal(
                PHASE_PREPARE,
                format!("failed to acquire store lock: {}", e),
            )
        })?;

        match operation {
            OP_CREATE => {
                // Fail-closed: draft must NOT exist for create
                if store.contains(&draft_id) {
                    return Err(Self::phase_wrap_validation(
                        PHASE_PREPARE,
                        format!("draft already exists: {}", draft_id),
                    ));
                }

                // Validate required fields from metadata
                let from = request
                    .metadata
                    .get("from")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let to_values = request
                    .metadata
                    .get("to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let subject = request
                    .metadata
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let body = request
                    .metadata
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                // Validate fields are present for create
                Self::validate_draft_fields(&draft_id, from, &to_values, subject, body)?;

                // Store validated fields in metadata for execute phase
                metadata.insert(
                    "from".to_string(),
                    serde_json::Value::String(from.to_string()),
                );
                metadata.insert(
                    "to".to_string(),
                    serde_json::Value::Array(
                        to_values
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
                metadata.insert(
                    "subject".to_string(),
                    serde_json::Value::String(subject.to_string()),
                );
                metadata.insert(
                    "body".to_string(),
                    serde_json::Value::String(body.to_string()),
                );
            }
            OP_UPDATE => {
                // Fail-closed: draft MUST exist for update
                let original_draft = store.get(&draft_id).ok_or_else(|| {
                    Self::phase_wrap_validation(
                        PHASE_PREPARE,
                        format!("draft not found for update: {}", draft_id),
                    )
                })?;

                // Store original draft state for potential rollback
                metadata.insert(
                    "original_from".to_string(),
                    serde_json::Value::String(original_draft.from.clone()),
                );
                metadata.insert(
                    "original_to".to_string(),
                    serde_json::Value::Array(
                        original_draft
                            .to
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
                metadata.insert(
                    "original_subject".to_string(),
                    serde_json::Value::String(original_draft.subject.clone()),
                );
                metadata.insert(
                    "original_body".to_string(),
                    serde_json::Value::String(original_draft.body.clone()),
                );

                // Validate new fields from metadata if provided
                let from = request
                    .metadata
                    .get("from")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&original_draft.from);
                let to_values = request
                    .metadata
                    .get("to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| original_draft.to.clone());
                let subject = request
                    .metadata
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&original_draft.subject);
                let body = request
                    .metadata
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&original_draft.body);

                // Validate fields
                Self::validate_draft_fields(&draft_id, from, &to_values, subject, body)?;

                // Store new values in metadata for execute phase
                metadata.insert(
                    "from".to_string(),
                    serde_json::Value::String(from.to_string()),
                );
                metadata.insert(
                    "to".to_string(),
                    serde_json::Value::Array(
                        to_values
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
                metadata.insert(
                    "subject".to_string(),
                    serde_json::Value::String(subject.to_string()),
                );
                metadata.insert(
                    "body".to_string(),
                    serde_json::Value::String(body.to_string()),
                );
            }
            OP_DELETE => {
                // Fail-closed: draft MUST exist for delete
                let original_draft = store.get(&draft_id).ok_or_else(|| {
                    Self::phase_wrap_validation(
                        PHASE_PREPARE,
                        format!("draft not found for delete: {}", draft_id),
                    )
                })?;

                // Store original draft state for potential rollback (recreation)
                metadata.insert(
                    "original_from".to_string(),
                    serde_json::Value::String(original_draft.from.clone()),
                );
                metadata.insert(
                    "original_to".to_string(),
                    serde_json::Value::Array(
                        original_draft
                            .to
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
                metadata.insert(
                    "original_subject".to_string(),
                    serde_json::Value::String(original_draft.subject.clone()),
                );
                metadata.insert(
                    "original_body".to_string(),
                    serde_json::Value::String(original_draft.body.clone()),
                );
                metadata.insert(
                    "original_created_at".to_string(),
                    serde_json::Value::String(original_draft.created_at.to_rfc3339()),
                );
            }
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported operation: {}",
                    operation
                )));
            }
        }

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: metadata,
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        _payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        // Validate that action_type is MailDraft
        if !matches!(contract.action_type, ActionType::MailDraft) {
            return Err(AdapterError::Unsupported(format!(
                "unsupported action type: {:?}",
                contract.action_type
            )));
        }

        let draft_id = Self::extract_draft_id(&contract.target)?;
        let operation = Self::get_operation(&contract.metadata)?;

        let mut store = self.store.lock().map_err(|e| {
            Self::phase_wrap_internal(
                PHASE_EXECUTE,
                format!("failed to acquire store lock: {}", e),
            )
        })?;

        match operation {
            OP_CREATE => {
                // Get fields from metadata (validated in prepare)
                let from = contract
                    .metadata
                    .get("from")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::Validation("from not found in metadata".into()))?;
                let to_values = contract
                    .metadata
                    .get("to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .ok_or_else(|| AdapterError::Validation("to not found in metadata".into()))?;
                let subject = contract
                    .metadata
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("subject not found in metadata".into())
                    })?;
                let body = contract
                    .metadata
                    .get("body")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::Validation("body not found in metadata".into()))?;

                let draft = EmailDraft {
                    id: draft_id.clone(),
                    from: from.to_string(),
                    to: to_values,
                    subject: subject.to_string(),
                    body: body.to_string(),
                    created_at: Utc::now(),
                };

                store.insert(draft);

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "operation".to_string(),
                    serde_json::Value::String(OP_CREATE.to_string()),
                );
                metadata.insert("draft_id".to_string(), serde_json::Value::String(draft_id));

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            OP_UPDATE => {
                // Get fields from metadata (validated in prepare)
                let from = contract
                    .metadata
                    .get("from")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::Validation("from not found in metadata".into()))?;
                let to_values = contract
                    .metadata
                    .get("to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .ok_or_else(|| AdapterError::Validation("to not found in metadata".into()))?;
                let subject = contract
                    .metadata
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("subject not found in metadata".into())
                    })?;
                let body = contract
                    .metadata
                    .get("body")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::Validation("body not found in metadata".into()))?;

                // Get original created_at if it exists (for update, preserve original timestamp)
                let original_created_at = if let Some(created_at_str) = contract
                    .metadata
                    .get("original_created_at")
                    .and_then(|v| v.as_str())
                {
                    DateTime::parse_from_rfc3339(created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                } else {
                    None
                };

                let created_at = original_created_at.unwrap_or_else(Utc::now);

                let draft = EmailDraft {
                    id: draft_id.clone(),
                    from: from.to_string(),
                    to: to_values,
                    subject: subject.to_string(),
                    body: body.to_string(),
                    created_at,
                };

                store.insert(draft);

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "operation".to_string(),
                    serde_json::Value::String(OP_UPDATE.to_string()),
                );
                metadata.insert("draft_id".to_string(), serde_json::Value::String(draft_id));

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            OP_DELETE => {
                store.remove(&draft_id);

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "operation".to_string(),
                    serde_json::Value::String(OP_DELETE.to_string()),
                );
                metadata.insert("draft_id".to_string(), serde_json::Value::String(draft_id));

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            _ => Err(AdapterError::Unsupported(format!(
                "unsupported operation: {}",
                operation
            ))),
        }
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        // Validate that action_type is MailDraft
        if !matches!(contract.action_type, ActionType::MailDraft) {
            return Err(AdapterError::Unsupported(format!(
                "unsupported action type: {:?}",
                contract.action_type
            )));
        }

        let draft_id = Self::extract_draft_id(&contract.target)?;
        let operation = Self::get_operation(&contract.metadata)?;

        let store = self.store.lock().map_err(|e| {
            Self::phase_wrap_internal(PHASE_VERIFY, format!("failed to acquire store lock: {}", e))
        })?;

        match operation {
            OP_CREATE | OP_UPDATE => {
                // Fail-closed: draft must exist after create/update
                let draft = store.get(&draft_id).ok_or_else(|| {
                    Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("draft not found after {}: {}", operation, draft_id),
                    )
                })?;

                // Verify expected content if provided in metadata
                if let Some(expected_from) = contract.metadata.get("from").and_then(|v| v.as_str())
                {
                    if draft.from != expected_from {
                        return Err(Self::phase_wrap_validation(
                            PHASE_VERIFY,
                            format!(
                                "from mismatch: expected '{}', got '{}'",
                                expected_from, draft.from
                            ),
                        ));
                    }
                }

                if let Some(expected_to_arr) =
                    contract.metadata.get("to").and_then(|v| v.as_array())
                {
                    let expected_to: Vec<&str> =
                        expected_to_arr.iter().filter_map(|v| v.as_str()).collect();
                    if draft.to.iter().map(|s| s.as_str()).collect::<Vec<_>>() != expected_to {
                        return Err(Self::phase_wrap_validation(
                            PHASE_VERIFY,
                            format!(
                                "to mismatch: expected {:?}, got {:?}",
                                expected_to, draft.to
                            ),
                        ));
                    }
                }

                if let Some(expected_subject) =
                    contract.metadata.get("subject").and_then(|v| v.as_str())
                {
                    if draft.subject != expected_subject {
                        return Err(Self::phase_wrap_validation(
                            PHASE_VERIFY,
                            format!(
                                "subject mismatch: expected '{}', got '{}'",
                                expected_subject, draft.subject
                            ),
                        ));
                    }
                }

                if let Some(expected_body) = contract.metadata.get("body").and_then(|v| v.as_str())
                {
                    if draft.body != expected_body {
                        return Err(Self::phase_wrap_validation(
                            PHASE_VERIFY,
                            format!(
                                "body mismatch: expected '{}', got '{}'",
                                expected_body, draft.body
                            ),
                        ));
                    }
                }
            }
            OP_DELETE => {
                // Fail-closed: draft must NOT exist after delete
                if store.contains(&draft_id) {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("draft still exists after delete: {}", draft_id),
                    ));
                }
            }
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported operation: {}",
                    operation
                )));
            }
        }

        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // Compensate is the same as rollback for MailDraft
        self.rollback(contract).await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // Validate that action_type is MailDraft
        if !matches!(contract.action_type, ActionType::MailDraft) {
            return Err(AdapterError::Unsupported(format!(
                "unsupported action type: {:?}",
                contract.action_type
            )));
        }

        let draft_id = Self::extract_draft_id(&contract.target)?;
        let operation = Self::get_operation(&contract.metadata)?;

        let mut store = self.store.lock().map_err(|e| {
            Self::phase_wrap_internal(
                PHASE_ROLLBACK,
                format!("failed to acquire store lock: {}", e),
            )
        })?;

        match operation {
            OP_CREATE => {
                // Rollback create: delete the created draft (idempotent)
                store.remove(&draft_id);

                // Verify it's gone
                if store.contains(&draft_id) {
                    return Err(Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!("rollback failed: draft still exists: {}", draft_id),
                    ));
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "rollback_action".to_string(),
                    serde_json::Value::String("deleted_created_draft".to_string()),
                );
                metadata.insert("draft_id".to_string(), serde_json::Value::String(draft_id));

                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                })
            }
            OP_UPDATE => {
                // Rollback update: restore original draft state
                let original_from = contract
                    .metadata
                    .get("original_from")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("original_from not found in metadata".into())
                    })?;
                let original_to = contract
                    .metadata
                    .get("original_to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .ok_or_else(|| {
                        AdapterError::Validation("original_to not found in metadata".into())
                    })?;
                let original_subject = contract
                    .metadata
                    .get("original_subject")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("original_subject not found in metadata".into())
                    })?;
                let original_body = contract
                    .metadata
                    .get("original_body")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("original_body not found in metadata".into())
                    })?;
                let original_created_at = contract
                    .metadata
                    .get("original_created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                let restored_draft = EmailDraft {
                    id: draft_id.clone(),
                    from: original_from.to_string(),
                    to: original_to,
                    subject: original_subject.to_string(),
                    body: original_body.to_string(),
                    created_at: original_created_at.unwrap_or_else(Utc::now),
                };

                store.insert(restored_draft);

                // Verify it's restored
                let restored = store.get(&draft_id).ok_or_else(|| {
                    Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!(
                            "rollback failed: draft not found after restore: {}",
                            draft_id
                        ),
                    )
                })?;

                // Verify content matches
                if restored.from != original_from
                    || restored.subject != original_subject
                    || restored.body != original_body
                {
                    return Err(Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!(
                            "rollback failed: restored draft content mismatch for {}",
                            draft_id
                        ),
                    ));
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "rollback_action".to_string(),
                    serde_json::Value::String("restored_original".to_string()),
                );
                metadata.insert("draft_id".to_string(), serde_json::Value::String(draft_id));

                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                })
            }
            OP_DELETE => {
                // Rollback delete: recreate the deleted draft
                let from = contract
                    .metadata
                    .get("original_from")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("original_from not found in metadata".into())
                    })?;
                let to = contract
                    .metadata
                    .get("original_to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .ok_or_else(|| {
                        AdapterError::Validation("original_to not found in metadata".into())
                    })?;
                let subject = contract
                    .metadata
                    .get("original_subject")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("original_subject not found in metadata".into())
                    })?;
                let body = contract
                    .metadata
                    .get("original_body")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AdapterError::Validation("original_body not found in metadata".into())
                    })?;
                let created_at = contract
                    .metadata
                    .get("original_created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);

                let restored_draft = EmailDraft {
                    id: draft_id.clone(),
                    from: from.to_string(),
                    to,
                    subject: subject.to_string(),
                    body: body.to_string(),
                    created_at,
                };

                store.insert(restored_draft);

                // Verify it's restored
                if !store.contains(&draft_id) {
                    return Err(Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!("rollback failed: draft not recreated: {}", draft_id),
                    ));
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "rollback_action".to_string(),
                    serde_json::Value::String("recreated_deleted_draft".to_string()),
                );
                metadata.insert("draft_id".to_string(), serde_json::Value::String(draft_id));

                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                })
            }
            _ => Err(AdapterError::Unsupported(format!(
                "unsupported operation: {}",
                operation
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{ExecutionId, IntentId, ProposalId, RollbackContractId, RollbackState};

    fn create_test_request(draft_id: &str, operation: &str) -> RollbackPrepareRequest {
        let mut metadata = JsonMap::new();
        metadata.insert(
            "operation".to_string(),
            serde_json::Value::String(operation.to_string()),
        );

        if operation == OP_CREATE || operation == OP_UPDATE {
            metadata.insert(
                "from".to_string(),
                serde_json::Value::String("sender@example.com".to_string()),
            );
            metadata.insert(
                "to".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(
                    "recipient@example.com".to_string(),
                )]),
            );
            metadata.insert(
                "subject".to_string(),
                serde_json::Value::String("Test Subject".to_string()),
            );
            metadata.insert(
                "body".to_string(),
                serde_json::Value::String("Test Body".to_string()),
            );
        }

        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::MailDraft,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "maildraft".to_string(),
            target: RollbackTarget::EmailDraft {
                draft_id: Some(draft_id.to_string()),
                recipients: vec!["recipient@example.com".to_string()],
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata,
        }
    }

    fn create_test_contract(
        draft_id: &str,
        _operation: &str,
        metadata: JsonMap,
    ) -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::MailDraft,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "maildraft".to_string(),
            target: RollbackTarget::EmailDraft {
                draft_id: Some(draft_id.to_string()),
                recipients: vec!["recipient@example.com".to_string()],
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata,
        }
    }

    #[tokio::test]
    async fn test_maildraft_prepare_create_validates_required_fields() {
        let adapter = MailDraftAdapter::new("maildraft");

        // Missing subject
        let mut request = create_test_request("draft1", OP_CREATE);
        request.metadata.remove("subject");
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("subject"));

        // Missing body
        let mut request = create_test_request("draft1", OP_CREATE);
        request.metadata.remove("body");
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("body"));

        // Missing from
        let mut request = create_test_request("draft1", OP_CREATE);
        request.metadata.remove("from");
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("from"));

        // Empty to array
        let mut request = create_test_request("draft1", OP_CREATE);
        request
            .metadata
            .insert("to".to_string(), serde_json::Value::Array(vec![]));
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("to"));
    }

    #[tokio::test]
    async fn test_maildraft_prepare_create_rejects_existing_draft() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "existing-draft".to_string(),
                from: "sender@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Existing".to_string(),
                body: "Body".to_string(),
                created_at: Utc::now(),
            });
        }

        // Try to create with same id
        let request = create_test_request("existing-draft", OP_CREATE);
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_maildraft_execute_create_stores_draft() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        let request = create_test_request("new-draft", OP_CREATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        let contract = create_test_contract("new-draft", OP_CREATE, prep_receipt.adapter_metadata);
        let _exec_receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify draft is in store
        let store_lock = store.lock().unwrap();
        let draft = store_lock.get("new-draft").expect("draft should exist");
        assert_eq!(draft.id, "new-draft");
        assert_eq!(draft.from, "sender@example.com");
        assert_eq!(draft.subject, "Test Subject");
    }

    #[tokio::test]
    async fn test_maildraft_verify_create_confirms_exists() {
        let adapter = MailDraftAdapter::new("maildraft");

        let request = create_test_request("verify-draft", OP_CREATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract =
            create_test_contract("verify-draft", OP_CREATE, prep_receipt.adapter_metadata);

        // Execute first
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Then verify
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_maildraft_rollback_create_deletes_draft() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        let request = create_test_request("rollback-create-draft", OP_CREATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = create_test_contract(
            "rollback-create-draft",
            OP_CREATE,
            prep_receipt.adapter_metadata,
        );
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify draft exists
        {
            let store_lock = store.lock().unwrap();
            assert!(store_lock.contains("rollback-create-draft"));
        }

        // Rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify draft is gone
        let store_lock = store.lock().unwrap();
        assert!(!store_lock.contains("rollback-create-draft"));
    }

    #[tokio::test]
    async fn test_maildraft_prepare_update_captures_original() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "update-draft".to_string(),
                from: "original@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Original Subject".to_string(),
                body: "Original Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("update-draft", OP_UPDATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        // Verify original state is captured in metadata
        let metadata = &prep_receipt.adapter_metadata;
        assert_eq!(
            metadata.get("original_from").and_then(|v| v.as_str()),
            Some("original@example.com")
        );
        assert_eq!(
            metadata.get("original_subject").and_then(|v| v.as_str()),
            Some("Original Subject")
        );
    }

    #[tokio::test]
    async fn test_maildraft_rollback_update_restores_original() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "restore-draft".to_string(),
                from: "original@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Original Subject".to_string(),
                body: "Original Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("restore-draft", OP_UPDATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract =
            create_test_contract("restore-draft", OP_UPDATE, prep_receipt.adapter_metadata);
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify updated content
        {
            let store_lock = store.lock().unwrap();
            let draft = store_lock.get("restore-draft").expect("draft should exist");
            assert_eq!(draft.from, "sender@example.com"); // New from
            assert_eq!(draft.subject, "Test Subject"); // New subject
        }

        // Rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify original is restored
        {
            let store_lock = store.lock().unwrap();
            let draft = store_lock.get("restore-draft").expect("draft should exist");
            assert_eq!(draft.from, "original@example.com");
            assert_eq!(draft.subject, "Original Subject");
            assert_eq!(draft.body, "Original Body");
        }
    }

    #[tokio::test]
    async fn test_maildraft_compensate_aliases_rollback() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "compensate-draft".to_string(),
                from: "original@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Original Subject".to_string(),
                body: "Original Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("compensate-draft", OP_UPDATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract =
            create_test_contract("compensate-draft", OP_UPDATE, prep_receipt.adapter_metadata);
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Compensate should do the same as rollback
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);

        // Verify original is restored
        let store_lock = store.lock().unwrap();
        let draft = store_lock
            .get("compensate-draft")
            .expect("draft should exist");
        assert_eq!(draft.from, "original@example.com");
        assert_eq!(draft.subject, "Original Subject");
    }

    #[tokio::test]
    async fn test_maildraft_prepare_delete_captures_original() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "delete-draft".to_string(),
                from: "sender@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "To Be Deleted".to_string(),
                body: "This will be gone".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("delete-draft", OP_DELETE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        // Verify original state is captured in metadata
        let metadata = &prep_receipt.adapter_metadata;
        assert_eq!(
            metadata.get("original_subject").and_then(|v| v.as_str()),
            Some("To Be Deleted")
        );
    }

    #[tokio::test]
    async fn test_maildraft_execute_delete_removes_draft() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "execute-delete-draft".to_string(),
                from: "sender@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Subject".to_string(),
                body: "Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("execute-delete-draft", OP_DELETE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = create_test_contract(
            "execute-delete-draft",
            OP_DELETE,
            prep_receipt.adapter_metadata,
        );
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify draft is gone
        let store_lock = store.lock().unwrap();
        assert!(!store_lock.contains("execute-delete-draft"));
    }

    #[tokio::test]
    async fn test_maildraft_rollback_delete_recreates_draft() {
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "rollback-delete-draft".to_string(),
                from: "sender@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Subject".to_string(),
                body: "Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("rollback-delete-draft", OP_DELETE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = create_test_contract(
            "rollback-delete-draft",
            OP_DELETE,
            prep_receipt.adapter_metadata,
        );
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Rollback should recreate the draft
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify draft is restored
        let store_lock = store.lock().unwrap();
        let draft = store_lock
            .get("rollback-delete-draft")
            .expect("draft should exist");
        assert_eq!(draft.from, "sender@example.com");
        assert_eq!(draft.subject, "Subject");
    }

    #[tokio::test]
    async fn test_maildraft_prepare_delete_fails_on_nonexistent() {
        let adapter = MailDraftAdapter::new("maildraft");

        let request = create_test_request("nonexistent-draft", OP_DELETE);
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_maildraft_prepare_update_fails_on_nonexistent() {
        let adapter = MailDraftAdapter::new("maildraft");

        let request = create_test_request("nonexistent-draft", OP_UPDATE);
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_maildraft_rollback_create_idempotent() {
        // Verify that rollback on an already-deleted draft (via prior rollback) is idempotent.
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        let request = create_test_request("idempotent-create-draft", OP_CREATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = create_test_contract(
            "idempotent-create-draft",
            OP_CREATE,
            prep_receipt.adapter_metadata,
        );
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // First rollback: deletes the draft
        let rollback_receipt1 = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt1.recovered);

        // Verify draft is gone after first rollback
        {
            let store_lock = store.lock().unwrap();
            assert!(!store_lock.contains("idempotent-create-draft"));
        }

        // Second rollback: should succeed (idempotent) — draft already deleted
        let rollback_receipt2 = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt2.recovered);

        // Draft should still be gone
        {
            let store_lock = store.lock().unwrap();
            assert!(!store_lock.contains("idempotent-create-draft"));
        }
    }

    #[tokio::test]
    async fn test_maildraft_rollback_update_idempotent() {
        // Verify that calling rollback update twice is idempotent (restoring same state twice).
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "idempotent-update-draft".to_string(),
                from: "original@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Original Subject".to_string(),
                body: "Original Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("idempotent-update-draft", OP_UPDATE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = create_test_contract(
            "idempotent-update-draft",
            OP_UPDATE,
            prep_receipt.adapter_metadata,
        );
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // First rollback: restores original
        let rollback_receipt1 = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt1.recovered);

        // Verify original is restored
        let draft1 = {
            let store_lock = store.lock().unwrap();
            store_lock
                .get("idempotent-update-draft")
                .expect("draft should exist")
                .clone()
        };
        assert_eq!(draft1.from, "original@example.com");
        assert_eq!(draft1.subject, "Original Subject");

        // Second rollback: should succeed (idempotent) — restoring same state again
        let rollback_receipt2 = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt2.recovered);

        // Draft should still have original content
        let draft2 = {
            let store_lock = store.lock().unwrap();
            store_lock
                .get("idempotent-update-draft")
                .expect("draft should exist")
                .clone()
        };
        assert_eq!(draft2.from, "original@example.com");
        assert_eq!(draft2.subject, "Original Subject");
        assert_eq!(draft2.body, "Original Body");
    }

    #[tokio::test]
    async fn test_maildraft_rollback_delete_idempotent() {
        // Verify that rollback delete on an already-recreated draft is idempotent.
        let adapter = MailDraftAdapter::new("maildraft");
        let store = Arc::clone(&adapter.store);

        // Pre-create a draft
        {
            let mut store_lock = store.lock().unwrap();
            store_lock.insert(EmailDraft {
                id: "idempotent-delete-draft".to_string(),
                from: "sender@example.com".to_string(),
                to: vec!["recipient@example.com".to_string()],
                subject: "Subject".to_string(),
                body: "Body".to_string(),
                created_at: Utc::now(),
            });
        }

        let request = create_test_request("idempotent-delete-draft", OP_DELETE);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = create_test_contract(
            "idempotent-delete-draft",
            OP_DELETE,
            prep_receipt.adapter_metadata,
        );
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify draft is gone after execute
        {
            let store_lock = store.lock().unwrap();
            assert!(!store_lock.contains("idempotent-delete-draft"));
        }

        // First rollback: recreates the draft
        let rollback_receipt1 = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt1.recovered);

        // Verify draft is restored
        let draft1 = {
            let store_lock = store.lock().unwrap();
            store_lock
                .get("idempotent-delete-draft")
                .expect("draft should exist")
                .clone()
        };
        assert_eq!(draft1.from, "sender@example.com");
        assert_eq!(draft1.subject, "Subject");

        // Second rollback: should succeed (idempotent) — removing and recreating same draft
        let rollback_receipt2 = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt2.recovered);

        // Draft should still exist with same content
        let draft2 = {
            let store_lock = store.lock().unwrap();
            store_lock
                .get("idempotent-delete-draft")
                .expect("draft should exist")
                .clone()
        };
        assert_eq!(draft2.from, "sender@example.com");
        assert_eq!(draft2.subject, "Subject");
        assert_eq!(draft2.body, "Body");
    }
}
