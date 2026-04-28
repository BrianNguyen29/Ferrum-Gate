//! External event source adapter for polling provenance events.
//!
//! This module provides a trait boundary for external event sources (e.g., MCP runtimes)
//! that can be faked for testing, enabling sync to consume external events without
//! real network dependencies.

use async_trait::async_trait;
use std::sync::Mutex;

use crate::error::{Result, SyncError};
use ferrum_proto::ProvenanceEvent;

/// Trait for external event sources that can be polled for provenance events.
///
/// Implementations must be Send + Sync because the trait is used as a trait object
/// in GatewayRuntime's bridges field, which must be Send + Sync for the runtime
/// to be usable as axum State.
///
/// # Design
///
/// This trait uses &self methods with interior mutability to enable:
/// - Trait objects stored in Arc<dyn ExternalEventSource> (requires &self only methods)
/// - Safe state mutations via Mutex guards within implementations
///
/// This trait is designed to be "fakeable" - a simple in-memory implementation
/// can satisfy this contract without any real network or external dependencies.
#[async_trait]
pub trait ExternalEventSource: Send + Sync {
    /// Returns the unique identifier for this runtime/event source.
    fn runtime_id(&self) -> &str;

    /// Returns true if the event source is currently connected.
    fn is_connected(&self) -> bool;

    /// Attempt to connect to the external event source.
    ///
    /// Idempotent: returns Ok(()) if already connected.
    async fn try_connect(&self) -> Result<()>;

    /// Poll for new provenance events from the external source.
    ///
    /// Returns a vector of events (may be empty). Returns error if not connected.
    async fn poll_events(&self) -> Result<Vec<ProvenanceEvent>>;
}

/// A fakeable in-memory external event source implementation for testing.
///
/// This implementation:
/// - Requires NO real external runtime
/// - Is fully deterministic and controllable via construction options
/// - Supports pre-loading events for test scenarios
///
/// # Builder Pattern
///
/// Use [`FakeExternalEventSource::new`] to create, then chain builder methods:
/// ```
/// # use ferrum_sync::FakeExternalEventSource;
/// let source = FakeExternalEventSource::new("test-runtime")
///     .with_events(vec![/* ProvenanceEvents */]);
/// ```
#[derive(Debug)]
pub struct FakeExternalEventSource {
    runtime_id: String,
    connected: Mutex<bool>,
    events: Mutex<Vec<ProvenanceEvent>>,
}

impl FakeExternalEventSource {
    /// Create a new FakeExternalEventSource with the given runtime ID.
    pub fn new(runtime_id: impl Into<String>) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            connected: Mutex::new(false),
            events: Mutex::new(Vec::new()),
        }
    }

    /// Pre-load events to be returned by subsequent poll_events calls.
    pub fn with_events(self, events: Vec<ProvenanceEvent>) -> Self {
        *self.events.lock().unwrap() = events;
        self
    }
}

#[async_trait]
impl ExternalEventSource for FakeExternalEventSource {
    fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    async fn try_connect(&self) -> Result<()> {
        *self.connected.lock().unwrap() = true;
        Ok(())
    }

    async fn poll_events(&self) -> Result<Vec<ProvenanceEvent>> {
        if !self.is_connected() {
            return Err(SyncError::Transport("not connected".to_string()));
        }
        // Drain events vec and return all pre-loaded events
        Ok(std::mem::take(&mut *self.events.lock().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActorRef, ActorType, HashChainRef, JsonMap, ObjectRef, ObjectType, ProvenanceEventKind,
    };

    fn make_test_event(kind: ProvenanceEventKind) -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test-actor".to_string(),
                display_name: Some("Test Actor".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: "test-object".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::default(),
            source_runtime_id: Some("test-runtime".to_string()),
        }
    }

    #[tokio::test]
    async fn fake_source_runtime_id() {
        let source = FakeExternalEventSource::new("mcp-runtime-42");
        assert_eq!(source.runtime_id(), "mcp-runtime-42");
    }

    #[tokio::test]
    async fn fake_source_disconnected_initially() {
        let source = FakeExternalEventSource::new("test");
        assert!(!source.is_connected());
    }

    #[tokio::test]
    async fn fake_source_connected_after_try_connect() {
        let source = FakeExternalEventSource::new("test");
        assert!(!source.is_connected());

        source
            .try_connect()
            .await
            .expect("try_connect should succeed");
        assert!(source.is_connected());

        // Idempotent: calling again should still succeed
        source
            .try_connect()
            .await
            .expect("try_connect should be idempotent");
        assert!(source.is_connected());
    }

    #[tokio::test]
    async fn fake_source_poll_events_roundtrip() {
        let event1 = make_test_event(ProvenanceEventKind::UserGoalReceived);
        let event2 = make_test_event(ProvenanceEventKind::IntentCompiled);

        let source = FakeExternalEventSource::new("test-runtime")
            .with_events(vec![event1.clone(), event2.clone()]);

        // Connect first
        source
            .try_connect()
            .await
            .expect("try_connect should succeed");

        // Poll and verify
        let polled = source
            .poll_events()
            .await
            .expect("poll_events should succeed");
        assert_eq!(polled.len(), 2);
        assert_eq!(polled[0].event_id, event1.event_id);
        assert_eq!(polled[1].event_id, event2.event_id);

        // Second poll returns empty (events drained)
        let polled_again = source
            .poll_events()
            .await
            .expect("poll_events should succeed");
        assert!(polled_again.is_empty());
    }

    #[tokio::test]
    async fn fake_source_poll_events_when_disconnected_returns_error() {
        let source = FakeExternalEventSource::new("test");

        // poll_events before connect should return error
        let result = source.poll_events().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, SyncError::Transport(_) if err.to_string().contains("not connected"))
        );
    }
}
