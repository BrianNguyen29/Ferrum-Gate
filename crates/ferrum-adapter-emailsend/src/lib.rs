//! Ferrum-Gate EmailSend adapter scaffold.
//!
//! **Scope:** This adapter is a scaffold-only implementation for the governed-path
//! entry point. It provides:
//! - Prepare-time validation for `EmailSend` action and `auto_commit=false` enforcement
//! - Fail-closed `execute()` that returns a clear validation error (send not implemented)
//! - No-op `verify()`, `compensate()`, and `rollback()` semantics (consistent with R3)
//!
//! **What this adapter does NOT do:**
//! - Actual email send operations
//! - Provider integration
//! - Any form of email delivery
//!
//! **Current boundary (must remain intact):**
//! - Gateway: `allow_send=true` EmailDraft bindings are denied at prepare-time
//! - This adapter: execute fails closed with validation error
//!
//! **Future work (post-scaffold):**
//! - Provider-level send/revoke semantics
//! - Real send implementation with proper R3 compensation model

use async_trait::async_trait;
use ferrum_proto::{ActionType, JsonMap, RollbackContract, RollbackPrepareRequest};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};

/// Adapter key for EmailSend adapter
pub const ADAPTER_KEY: &str = "emailsend";

/// EmailSend adapter scaffold.
///
/// This adapter enforces R3 prepare-time validation and fail-closed execute behavior.
/// It is intentionally a scaffold: execute does not perform any actual send operation.
#[derive(Debug, Clone)]
pub struct EmailSendAdapter {
    key: &'static str,
}

impl EmailSendAdapter {
    /// Create a new EmailSendAdapter with default key
    pub fn new() -> Self {
        Self { key: ADAPTER_KEY }
    }

    /// Create a new EmailSendAdapter with a custom key
    pub fn with_key(key: &'static str) -> Self {
        Self { key }
    }
}

impl Default for EmailSendAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RollbackAdapter for EmailSendAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    /// Prepare-time validation for EmailSend.
    ///
    /// Validates:
    /// - `action_type` must be `ActionType::EmailSend`
    /// - `auto_commit` must be `false` (R3 enforcement: irreversible operations cannot auto-commit)
    ///
    /// Returns `PrepareReceipt { accepted: true }` if validation passes.
    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate action_type is EmailSend
        if !matches!(request.action_type, ActionType::EmailSend) {
            return Err(AdapterError::Unsupported(format!(
                "EmailSendAdapter only supports EmailSend action type, got: {:?}",
                request.action_type
            )));
        }

        // R3 enforcement: auto_commit must be false for EmailSend (irreversible operations)
        if request.auto_commit {
            return Err(AdapterError::Validation(
                "EmailSend does not support auto_commit=true (R3: irreversible operations cannot auto-commit)"
                    .to_string(),
            ));
        }

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    /// Execute scaffold — fail-closed.
    ///
    /// Returns a clear validation error indicating that actual send is not implemented.
    /// This preserves the fail-closed boundary: execute never succeeds silently.
    async fn execute(
        &self,
        _contract: &RollbackContract,
        _payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        // Fail-closed: EmailSend execute is not implemented in this scaffold.
        // Real send requires separate provider integration and R3 safety analysis.
        Err(AdapterError::Validation(
            "EmailSend adapter: execute not implemented (scaffold only). \
             Real send requires provider integration and R3 safety analysis."
                .to_string(),
        ))
    }

    /// Verify scaffold — no-op returning verified=true.
    ///
    /// Since execute does nothing, verification is trivial. In a real implementation,
    /// this would check provider delivery confirmation.
    async fn verify(&self, _contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        Ok(VerifyReceipt {
            verified: true, // Scaffold: nothing to verify since execute is no-op
            adapter_metadata: JsonMap::new(),
        })
    }

    /// Compensate scaffold — no-op returning recovered=false.
    ///
    /// EmailSend is R3 (irreversible). True "unsend" is not available from providers.
    /// This no-op documents that compensation is not possible.
    async fn compensate(
        &self,
        _contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        let mut metadata = JsonMap::new();
        metadata.insert("compensate".to_string(), serde_json::json!("no-op"));
        metadata.insert(
            "reason".to_string(),
            serde_json::json!("EmailSend is R3: no automatic undo available"),
        );

        Ok(RecoveryReceipt {
            recovered: false, // Cannot undo send
            adapter_metadata: metadata,
        })
    }

    /// Rollback scaffold — same as compensate for EmailSend (R3).
    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        self.compensate(contract).await
    }
}

/// Register the EmailSend adapter in the registry
pub fn register_emailsend_adapter(registry: &mut ferrum_rollback::AdapterRegistry) {
    registry.register(std::sync::Arc::new(EmailSendAdapter::new()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::RollbackTarget;

    fn make_email_send_prepare_request() -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id: ferrum_proto::ExecutionId::new(),
            action_type: ActionType::EmailSend,
            rollback_class: ferrum_proto::RollbackClass::R3IrreversibleHighConsequence,
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
        }
    }

    fn make_email_send_contract(request: &RollbackPrepareRequest) -> RollbackContract {
        RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: request.intent_id,
            proposal_id: request.proposal_id,
            execution_id: request.execution_id,
            action_type: request.action_type.clone(),
            rollback_class: request.rollback_class.clone(),
            adapter_key: request.adapter_key.clone(),
            target: request.target.clone(),
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: request.auto_commit,
            state: ferrum_proto::RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        }
    }

    #[tokio::test]
    async fn test_prepare_accepts_email_send_with_auto_commit_false() {
        let adapter = EmailSendAdapter::new();
        let request = make_email_send_prepare_request();

        let result = adapter.prepare(&request).await;
        assert!(result.is_ok());
        let receipt = result.unwrap();
        assert!(receipt.accepted);
    }

    #[tokio::test]
    async fn test_prepare_rejects_auto_commit_true() {
        let adapter = EmailSendAdapter::new();
        let mut request = make_email_send_prepare_request();
        request.auto_commit = true;

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("auto_commit"));
        assert!(err.to_string().contains("R3"));
    }

    #[tokio::test]
    async fn test_prepare_rejects_non_email_send_action() {
        let adapter = EmailSendAdapter::new();
        let mut request = make_email_send_prepare_request();
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("EmailSend"));
        assert!(err.to_string().contains("unsupported"));
    }

    #[tokio::test]
    async fn test_execute_fails_closed_with_validation_error() {
        let adapter = EmailSendAdapter::new();
        let request = make_email_send_prepare_request();
        let contract = make_email_send_contract(&request);

        let payload = serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Test",
            "body": "Hello!"
        });

        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not implemented"));
        assert!(err.to_string().contains("scaffold"));
    }

    #[tokio::test]
    async fn test_verify_returns_true_no_op() {
        let adapter = EmailSendAdapter::new();
        let request = make_email_send_prepare_request();
        let contract = make_email_send_contract(&request);

        let result = adapter.verify(&contract).await;
        assert!(result.is_ok());
        let receipt = result.unwrap();
        assert!(receipt.verified);
    }

    #[tokio::test]
    async fn test_compensate_returns_recovered_false() {
        let adapter = EmailSendAdapter::new();
        let request = make_email_send_prepare_request();
        let contract = make_email_send_contract(&request);

        let result = adapter.compensate(&contract).await;
        assert!(result.is_ok());
        let receipt = result.unwrap();
        assert!(!receipt.recovered);
        assert_eq!(
            receipt
                .adapter_metadata
                .get("reason")
                .and_then(|v| v.as_str()),
            Some("EmailSend is R3: no automatic undo available")
        );
    }

    #[tokio::test]
    async fn test_rollback_returns_recovered_false() {
        let adapter = EmailSendAdapter::new();
        let request = make_email_send_prepare_request();
        let contract = make_email_send_contract(&request);

        let result = adapter.rollback(&contract).await;
        assert!(result.is_ok());
        let receipt = result.unwrap();
        assert!(!receipt.recovered);
    }

    #[tokio::test]
    async fn test_adapter_key_is_emailsend() {
        let adapter = EmailSendAdapter::new();
        assert_eq!(adapter.key(), ADAPTER_KEY);
        assert_eq!(adapter.key(), "emailsend");
    }

    #[tokio::test]
    async fn test_adapter_with_custom_key() {
        let adapter = EmailSendAdapter::with_key("custom-key");
        assert_eq!(adapter.key(), "custom-key");
    }
}
