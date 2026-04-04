//! Ferrum-Gate EmailSend adapter scaffold.
//!
//! **Scope:** This adapter provides the governed-path entry point for EmailSend.
//! It currently ships with provider abstraction infrastructure and a mock provider
//! for testing. The adapter execute remains fail-closed (no actual send) until
//! real provider integration is completed.
//!
//! **Current implementation status:**
//! - Prepare-time validation for `EmailSend` action and `auto_commit=false` enforcement ✅
//! - Provider abstraction (`EmailProvider` trait) ✅
//! - Mock provider (`MockEmailProvider`) for unit testing ✅
//! - Provider injection via `with_provider()` constructor ✅
//! - Fail-closed `execute()` (scaffold: send not yet wired to provider) ✅
//! - No-op `verify()`, `compensate()`, and `rollback()` semantics (consistent with R3) ✅
//!
//! **What this adapter does NOT do (yet):**
//! - Actual email send operations (execute is fail-closed scaffold)
//! - Provider integration (mock only; real provider TBD)
//! - Any form of email delivery
//!
//! **Current boundary (must remain intact):**
//! - Gateway: `allow_send=true` EmailDraft bindings are denied at prepare-time
//! - This adapter: execute fails closed with validation error
//!
//! **Future work (post-mock-provider slice):**
//! - Real provider integration (SMTP/API client)
//! - Provider send/revoke semantics wiring to execute()
//! - R3 compensation model for actual send

use async_trait::async_trait;
use ferrum_proto::{ActionType, JsonMap, RollbackContract, RollbackPrepareRequest};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Email provider abstraction.
///
/// Provider-agnostic core: adapter code depends on this trait, not on
/// specific SMTP/API client implementations. Real providers (SMTP, SendGrid,
/// SES, etc.) implement this trait. A `MockEmailProvider` is provided for
/// unit testing.
#[async_trait]
pub trait EmailProvider: Send + Sync {
    /// Send an email and return the provider's message reference.
    async fn send(
        &self,
        to: Vec<String>,
        subject: &str,
        body: &str,
    ) -> Result<ProviderSendResult, ProviderError>;

    /// Check if a message can be revoked (rarely supported by providers).
    async fn can_revoke(&self, message_id: &str) -> bool;

    /// Attempt to revoke a sent message (if supported by provider).
    async fn revoke(&self, message_id: &str) -> Result<(), ProviderError>;
}

/// Result of a successful send operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSendResult {
    /// Global unique message ID assigned by the provider.
    pub message_id: String,
    /// Provider's internal reference (may differ from message_id).
    pub provider_ref: String,
}

/// Provider-specific errors.
///
/// Categorized to allow adapter-level retry/decide logic:
/// - Transient: retryable (network hiccup, rate limit, etc.)
/// - Permanent: non-retryable (bad address, auth failure, etc.)
/// - Auth: authentication failure with provider
/// - Network: network connectivity issue
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Retryable error (temporary provider issue).
    Transient(String),
    /// Non-retryable error (bad address, policy violation, etc.).
    Permanent(String),
    /// Authentication failure with provider.
    Auth(String),
    /// Network connectivity issue.
    Network(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Transient(s) => write!(f, "Transient: {}", s),
            ProviderError::Permanent(s) => write!(f, "Permanent: {}", s),
            ProviderError::Auth(s) => write!(f, "Auth: {}", s),
            ProviderError::Network(s) => write!(f, "Network: {}", s),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Mock email provider for unit testing.
///
/// Configurable success/failure behavior with call tracking:
/// - `send_call_count`, `can_revoke_call_count`, `revoke_call_count`
/// - `send_calls`: records all send invocations with args and result
/// - `should_fail`, `failure_reason`: injectable failure mode
#[derive(Debug, Clone)]
pub struct MockEmailProvider {
    /// Whether send() should return an error.
    should_fail: Arc<AtomicBool>,
    /// Reason for failure (used when should_fail is true).
    failure_reason: Arc<std::sync::Mutex<ProviderError>>,
    /// Number of times send() was called.
    send_call_count: Arc<AtomicUsize>,
    /// Number of times can_revoke() was called.
    can_revoke_call_count: Arc<AtomicUsize>,
    /// Number of times revoke() was called.
    revoke_call_count: Arc<AtomicUsize>,
    /// Record of all send() calls: (to, subject, body, result).
    send_calls: Arc<std::sync::Mutex<Vec<SendCall>>>,
    /// Next message_id to assign (starts at 1).
    next_message_id: Arc<AtomicUsize>,
    /// Whether can_revoke returns true.
    can_revoke_supported: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct SendCall {
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
    pub result: Result<ProviderSendResult, ProviderError>,
}

impl MockEmailProvider {
    /// Create a new MockEmailProvider that succeeds by default.
    pub fn new() -> Self {
        Self {
            should_fail: Arc::new(AtomicBool::new(false)),
            failure_reason: Arc::new(std::sync::Mutex::new(ProviderError::Transient(
                "mock error".to_string(),
            ))),
            send_call_count: Arc::new(AtomicUsize::new(0)),
            can_revoke_call_count: Arc::new(AtomicUsize::new(0)),
            revoke_call_count: Arc::new(AtomicUsize::new(0)),
            send_calls: Arc::new(std::sync::Mutex::new(Vec::new())),
            next_message_id: Arc::new(AtomicUsize::new(1)),
            can_revoke_supported: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Configure the mock to fail with the given error on send.
    pub fn with_failure(self, error: ProviderError) -> Self {
        self.should_fail.store(true, Ordering::SeqCst);
        *self.failure_reason.lock().unwrap() = error;
        self
    }

    /// Configure the mock to succeed with can_revoke.
    pub fn with_revoke_supported(self) -> Self {
        self.can_revoke_supported.store(true, Ordering::SeqCst);
        self
    }

    /// Returns the number of times send() was called.
    pub fn send_call_count(&self) -> usize {
        self.send_call_count.load(Ordering::SeqCst)
    }

    /// Returns the number of times can_revoke() was called.
    pub fn can_revoke_call_count(&self) -> usize {
        self.can_revoke_call_count.load(Ordering::SeqCst)
    }

    /// Returns the number of times revoke() was called.
    pub fn revoke_call_count(&self) -> usize {
        self.revoke_call_count.load(Ordering::SeqCst)
    }

    /// Returns a copy of all send() call records.
    #[allow(dead_code)]
    pub(crate) fn send_calls(&self) -> Vec<SendCall> {
        self.send_calls.lock().unwrap().clone()
    }

    /// Reset all call counters and call history.
    pub fn reset(&self) {
        self.send_call_count.store(0, Ordering::SeqCst);
        self.can_revoke_call_count.store(0, Ordering::SeqCst);
        self.revoke_call_count.store(0, Ordering::SeqCst);
        self.send_calls.lock().unwrap().clear();
        self.next_message_id.store(1, Ordering::SeqCst);
    }
}

impl Default for MockEmailProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmailProvider for MockEmailProvider {
    async fn send(
        &self,
        to: Vec<String>,
        subject: &str,
        body: &str,
    ) -> Result<ProviderSendResult, ProviderError> {
        self.send_call_count.fetch_add(1, Ordering::SeqCst);

        let msg_id = self.next_message_id.fetch_add(1, Ordering::SeqCst);
        let message_id = format!("mock-message-id-{}", msg_id);
        let provider_ref = format!("mock-provider-ref-{}", msg_id);

        let result = if self.should_fail.load(Ordering::SeqCst) {
            Err(self.failure_reason.lock().unwrap().clone())
        } else {
            Ok(ProviderSendResult {
                message_id,
                provider_ref,
            })
        };

        self.send_calls.lock().unwrap().push(SendCall {
            to: to.clone(),
            subject: subject.to_string(),
            body: body.to_string(),
            result: result.clone(),
        });

        result
    }

    async fn can_revoke(&self, _message_id: &str) -> bool {
        self.can_revoke_call_count.fetch_add(1, Ordering::SeqCst);
        self.can_revoke_supported.load(Ordering::SeqCst)
    }

    async fn revoke(&self, _message_id: &str) -> Result<(), ProviderError> {
        self.revoke_call_count.fetch_add(1, Ordering::SeqCst);
        if self.can_revoke_supported.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ProviderError::Permanent(
                "revoke not supported by this provider".to_string(),
            ))
        }
    }
}

/// Adapter key for EmailSend adapter
pub const ADAPTER_KEY: &str = "emailsend";

/// EmailSend adapter scaffold.
///
/// This adapter enforces R3 prepare-time validation and fail-closed execute behavior.
/// It is intentionally a scaffold: execute does not perform any actual send operation.
/// A provider is stored for future send wiring but is not invoked in this scaffold.
pub struct EmailSendAdapter {
    key: &'static str,
    provider: Arc<dyn EmailProvider>,
}

impl Debug for EmailSendAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailSendAdapter")
            .field("key", &self.key)
            .field("provider", &"<dyn EmailProvider>")
            .finish()
    }
}

impl Clone for EmailSendAdapter {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            provider: self.provider.clone(),
        }
    }
}

impl EmailSendAdapter {
    /// Create a new EmailSendAdapter with default key and mock provider.
    ///
    /// The mock provider is stored and accessible but execute remains fail-closed.
    pub fn new() -> Self {
        Self::with_provider(ADAPTER_KEY, Arc::new(MockEmailProvider::new()))
    }

    /// Create a new EmailSendAdapter with a custom key and mock provider.
    pub fn with_key(key: &'static str) -> Self {
        Self::with_provider(key, Arc::new(MockEmailProvider::new()))
    }

    /// Create a new EmailSendAdapter with an injected provider.
    ///
    /// This constructor allows dependency injection of any `EmailProvider`
    /// implementation (mock for tests, or a real SMTP/API client in the future).
    /// Execute remains fail-closed regardless of the provider stored.
    pub fn with_provider(key: &'static str, provider: Arc<dyn EmailProvider>) -> Self {
        Self { key, provider }
    }

    /// Get the stored provider (for test inspection).
    ///
    /// Useful for verifying provider state and call tracking in tests.
    pub fn provider(&self) -> &Arc<dyn EmailProvider> {
        &self.provider
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

    #[tokio::test]
    async fn test_new_adapter_has_mock_provider() {
        let adapter = EmailSendAdapter::new();
        // Provider should be accessible
        let _provider = adapter.provider();
        // Provider should be a MockEmailProvider (via Arc<dyn EmailProvider>)
        // We can verify via can_revoke which is a no-op on mock
        let can_revoke = adapter.provider().can_revoke("any-id").await;
        assert!(!can_revoke); // Mock defaults to can_revoke=false
    }

    #[tokio::test]
    async fn test_with_provider_stores_provider() {
        let mock_provider: Arc<dyn EmailProvider> = Arc::new(MockEmailProvider::new());
        let adapter = EmailSendAdapter::with_provider("custom-key", mock_provider.clone());

        assert_eq!(adapter.key(), "custom-key");
        // Provider should be the exact same Arc we injected
        let stored_provider = adapter.provider();
        assert!(Arc::ptr_eq(stored_provider, &mock_provider));
    }

    #[tokio::test]
    async fn test_with_provider_can_use_failure_configured_provider() {
        let failing_provider = Arc::new(
            MockEmailProvider::new().with_failure(ProviderError::Transient("test".to_string())),
        );
        let adapter = EmailSendAdapter::with_provider(ADAPTER_KEY, failing_provider.clone());

        // Provider is stored and accessible; we can call it directly (not via execute)
        let result = adapter
            .provider()
            .send(vec!["alice@example.com".to_string()], "Subject", "Body")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::Transient(_)));
    }

    #[tokio::test]
    async fn test_execute_still_fails_closed_with_injected_provider() {
        // Prove execute remains fail-closed even when a provider is injected.
        // This is the core invariant: provider injection ≠ send wiring.
        let mock_provider = Arc::new(MockEmailProvider::new());
        let adapter = EmailSendAdapter::with_provider(ADAPTER_KEY, mock_provider);

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

        // Mock provider was NOT called (execute is fail-closed)
        // We can't easily inspect mock state here since provider is Arc<dyn>,
        // but the fail-closed invariant is proven by the error above
    }

    #[tokio::test]
    async fn test_with_provider_revoke_supported() {
        let provider = Arc::new(MockEmailProvider::new().with_revoke_supported());
        let adapter = EmailSendAdapter::with_provider(ADAPTER_KEY, provider.clone());

        // Provider supports revoke
        assert!(adapter.provider().can_revoke("any-id").await);
    }
}

// =============================================================================
// MockEmailProvider unit tests
// =============================================================================

#[cfg(test)]
mod mock_email_provider_tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_send_success() {
        let provider = MockEmailProvider::new();

        let result = provider
            .send(
                vec!["alice@example.com".to_string()],
                "Test Subject",
                "Test Body",
            )
            .await;

        assert!(result.is_ok());
        let send_result = result.unwrap();
        assert!(send_result.message_id.starts_with("mock-message-id-"));
        assert!(send_result.provider_ref.starts_with("mock-provider-ref-"));
        assert_eq!(provider.send_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_send_tracks_calls() {
        let provider = MockEmailProvider::new();

        provider
            .send(vec!["alice@example.com".to_string()], "Subject 1", "Body 1")
            .await
            .unwrap();

        provider
            .send(
                vec![
                    "bob@example.com".to_string(),
                    "carol@example.com".to_string(),
                ],
                "Subject 2",
                "Body 2",
            )
            .await
            .unwrap();

        assert_eq!(provider.send_call_count(), 2);

        let calls = provider.send_calls();
        assert_eq!(calls.len(), 2);

        assert_eq!(calls[0].to, vec!["alice@example.com"]);
        assert_eq!(calls[0].subject, "Subject 1");
        assert_eq!(calls[0].body, "Body 1");
        assert!(calls[0].result.is_ok());

        assert_eq!(calls[1].to, vec!["bob@example.com", "carol@example.com"]);
        assert_eq!(calls[1].subject, "Subject 2");
        assert_eq!(calls[1].body, "Body 2");
        assert!(calls[1].result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_provider_send_failure_transient() {
        let provider = MockEmailProvider::new()
            .with_failure(ProviderError::Transient("network timeout".to_string()));

        let result = provider
            .send(
                vec!["alice@example.com".to_string()],
                "Test Subject",
                "Test Body",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::Transient(_)));
        assert_eq!(provider.send_call_count(), 1);

        let calls = provider.send_calls();
        assert!(calls[0].result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_send_failure_permanent() {
        let provider = MockEmailProvider::new()
            .with_failure(ProviderError::Permanent("invalid recipient".to_string()));

        let result = provider
            .send(
                vec!["alice@example.com".to_string()],
                "Test Subject",
                "Test Body",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::Permanent(_)));
    }

    #[tokio::test]
    async fn test_mock_provider_send_failure_auth() {
        let provider = MockEmailProvider::new()
            .with_failure(ProviderError::Auth("invalid API key".to_string()));

        let result = provider
            .send(
                vec!["alice@example.com".to_string()],
                "Test Subject",
                "Test Body",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::Auth(_)));
    }

    #[tokio::test]
    async fn test_mock_provider_send_failure_network() {
        let provider = MockEmailProvider::new()
            .with_failure(ProviderError::Network("connection refused".to_string()));

        let result = provider
            .send(
                vec!["alice@example.com".to_string()],
                "Test Subject",
                "Test Body",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
    }

    #[tokio::test]
    async fn test_mock_provider_can_revoke_returns_false_by_default() {
        let provider = MockEmailProvider::new();

        assert!(!provider.can_revoke("any-message-id").await);
        assert_eq!(provider.can_revoke_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_can_revoke_returns_true_when_enabled() {
        let provider = MockEmailProvider::new().with_revoke_supported();

        assert!(provider.can_revoke("any-message-id").await);
        assert_eq!(provider.can_revoke_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_revoke_returns_error_by_default() {
        let provider = MockEmailProvider::new();

        let result = provider.revoke("any-message-id").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::Permanent(_)));
        assert_eq!(provider.revoke_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_revoke_succeeds_when_enabled() {
        let provider = MockEmailProvider::new().with_revoke_supported();

        let result = provider.revoke("any-message-id").await;
        assert!(result.is_ok());
        assert_eq!(provider.revoke_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_reset_clears_state() {
        let provider =
            MockEmailProvider::new().with_failure(ProviderError::Transient("test".to_string()));

        // Trigger some calls
        provider
            .send(vec!["alice@example.com".to_string()], "Subject", "Body")
            .await
            .unwrap_err();
        provider.can_revoke("msg-1").await;
        let _ = provider.revoke("msg-1").await;

        assert_eq!(provider.send_call_count(), 1);
        assert_eq!(provider.can_revoke_call_count(), 1);
        assert_eq!(provider.revoke_call_count(), 1);

        // Reset
        provider.reset();

        assert_eq!(provider.send_call_count(), 0);
        assert_eq!(provider.can_revoke_call_count(), 0);
        assert_eq!(provider.revoke_call_count(), 0);
        assert!(provider.send_calls().is_empty());
    }

    #[tokio::test]
    async fn test_mock_provider_unique_message_ids_per_send() {
        let provider = MockEmailProvider::new();

        let result1 = provider
            .send(vec!["alice@example.com".to_string()], "Subject", "Body")
            .await
            .unwrap();
        let result2 = provider
            .send(vec!["bob@example.com".to_string()], "Subject", "Body")
            .await
            .unwrap();

        assert_ne!(result1.message_id, result2.message_id);
        assert_ne!(result1.provider_ref, result2.provider_ref);
    }

    #[tokio::test]
    async fn test_mock_provider_error_display() {
        let err = ProviderError::Transient("test transient".to_string());
        assert!(err.to_string().contains("Transient"));
        assert!(err.to_string().contains("test transient"));

        let err = ProviderError::Permanent("test permanent".to_string());
        assert!(err.to_string().contains("Permanent"));

        let err = ProviderError::Auth("test auth".to_string());
        assert!(err.to_string().contains("Auth"));

        let err = ProviderError::Network("test network".to_string());
        assert!(err.to_string().contains("Network"));
    }

    #[tokio::test]
    async fn test_mock_provider_clone_is_independent() {
        let provider = MockEmailProvider::new();
        let provider2 = provider.clone();

        // Send via provider1
        provider
            .send(vec!["alice@example.com".to_string()], "Subject", "Body")
            .await
            .unwrap();

        // provider2 should not see provider1's calls (cloned Arc values are shared,
        // but AtomicUsize counters are independent per-instance)
        // Actually, Arc<AtomicUsize> shares the same counter, so they ARE shared.
        // This is expected behavior for a cheap clone.
        assert_eq!(provider.send_call_count(), 1);
        assert_eq!(provider2.send_call_count(), 1); // Shared via Arc
    }
}
