//! Runtime bridge trait and MCP bridge implementation.
//!
//! This module extends the ExternalEventSource trait with bridge-specific
//! operations for tool discovery and event submission to external runtimes.

use async_trait::async_trait;
use std::sync::Mutex;

use crate::error::{Result, SyncError};
use crate::external_source::ExternalEventSource;
use crate::transport::BoxedTransport;
use ferrum_proto::ProvenanceEvent;

/// Information about a tool available through a runtime bridge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BridgeToolInfo {
    /// The tool name as registered in the external runtime.
    pub name: String,
    /// Human-readable description of the tool.
    pub description: String,
    /// JSON Schema for the tool's input parameters (optional).
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// Result of submitting an event through a runtime bridge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BridgeSubmitResult {
    /// Whether the submission was accepted by the external runtime.
    pub accepted: bool,
    /// Optional message from the external runtime.
    pub message: Option<String>,
}

/// Extended trait for runtime bridges that support tool discovery and event submission.
///
/// RuntimeBridge extends ExternalEventSource with additional capabilities:
/// - Tool discovery: list tools available in the external runtime
/// - Event submission: send events to the external runtime
///
/// Implementations must be Send + Sync for use as trait objects in async contexts.
#[async_trait]
pub trait RuntimeBridge: ExternalEventSource + Send + Sync {
    /// List tools available through this bridge.
    async fn list_tools(&self) -> Result<Vec<BridgeToolInfo>>;

    /// Submit an event to the external runtime.
    async fn submit_event(&self, event: &ProvenanceEvent) -> Result<BridgeSubmitResult>;
}

/// MCP (Model Context Protocol) bridge implementation.
///
/// This is a skeleton implementation that wraps a Transport for future
/// real MCP communication. Currently uses in-memory state for testing.
///
/// # Design
///
/// - Owns a `BoxedTransport` for future real transport usage
/// - Uses interior mutability (Mutex) for mutable state
/// - Pre-loaded tools and events for test scenarios
pub struct McpBridge {
    runtime_id: String,
    #[allow(dead_code)]
    transport: Option<BoxedTransport>,
    connected: Mutex<bool>,
    tools: Mutex<Vec<BridgeToolInfo>>,
    pending_events: Mutex<Vec<ProvenanceEvent>>,
}

impl std::fmt::Debug for McpBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBridge")
            .field("runtime_id", &self.runtime_id)
            .field("transport", &"<BoxedTransport>")
            .field("connected", &*self.connected.lock().unwrap())
            .field("tools", &*self.tools.lock().unwrap())
            .field("pending_events", &self.pending_events.lock().unwrap().len())
            .finish()
    }
}

impl McpBridge {
    /// Create a new McpBridge with the given runtime ID.
    pub fn new(runtime_id: impl Into<String>) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            transport: None,
            connected: Mutex::new(false),
            tools: Mutex::new(Vec::new()),
            pending_events: Mutex::new(Vec::new()),
        }
    }

    /// Create a McpBridge with a transport and pre-loaded tools.
    pub fn with_transport(mut self, transport: BoxedTransport) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Pre-load tools for test scenarios.
    pub fn with_tools(self, tools: Vec<BridgeToolInfo>) -> Self {
        *self.tools.lock().unwrap() = tools;
        self
    }
}

#[async_trait]
impl ExternalEventSource for McpBridge {
    fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    async fn try_connect(&self) -> Result<()> {
        // In a real implementation, this would use the transport to establish connection
        *self.connected.lock().unwrap() = true;
        Ok(())
    }

    async fn poll_events(&self) -> Result<Vec<ProvenanceEvent>> {
        if !self.is_connected() {
            return Err(SyncError::Transport("not connected".to_string()));
        }
        Ok(std::mem::take(&mut *self.pending_events.lock().unwrap()))
    }
}

#[async_trait]
impl RuntimeBridge for McpBridge {
    async fn list_tools(&self) -> Result<Vec<BridgeToolInfo>> {
        if !self.is_connected() {
            return Err(SyncError::Transport("not connected".to_string()));
        }
        Ok(self.tools.lock().unwrap().clone())
    }

    async fn submit_event(&self, event: &ProvenanceEvent) -> Result<BridgeSubmitResult> {
        if !self.is_connected() {
            return Err(SyncError::Transport("not connected".to_string()));
        }
        self.pending_events.lock().unwrap().push(event.clone());
        Ok(BridgeSubmitResult {
            accepted: true,
            message: Some("event accepted by MCP runtime".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActorRef, ActorType, HashChainRef, JsonMap, ObjectRef, ObjectType, ProvenanceEvent,
        ProvenanceEventKind,
    };

    fn make_test_event() -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test-actor".to_string(),
                display_name: None,
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
            source_runtime_id: Some("mcp-bridge-test".to_string()),
        }
    }

    fn make_test_tool(name: &str) -> BridgeToolInfo {
        BridgeToolInfo {
            name: name.to_string(),
            description: format!("Test tool: {}", name),
            input_schema: None,
        }
    }

    #[tokio::test]
    async fn mcp_bridge_runtime_id() {
        let bridge = McpBridge::new("mcp://test-runtime");
        assert_eq!(bridge.runtime_id(), "mcp://test-runtime");
    }

    #[tokio::test]
    async fn mcp_bridge_connect_and_list_tools() {
        let bridge = McpBridge::new("mcp://test").with_tools(vec![
            make_test_tool("read_file"),
            make_test_tool("write_file"),
        ]);

        bridge.try_connect().await.unwrap();

        let tools = bridge.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[1].name, "write_file");
    }

    #[tokio::test]
    async fn mcp_bridge_submit_event() {
        let bridge = McpBridge::new("mcp://test");
        bridge.try_connect().await.unwrap();

        let event = make_test_event();
        let result = bridge.submit_event(&event).await.unwrap();
        assert!(result.accepted);
    }

    #[tokio::test]
    async fn mcp_bridge_list_tools_when_disconnected() {
        let bridge = McpBridge::new("mcp://test").with_tools(vec![make_test_tool("tool")]);

        let result = bridge.list_tools().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mcp_bridge_submit_event_when_disconnected() {
        let bridge = McpBridge::new("mcp://test");

        let event = make_test_event();
        let result = bridge.submit_event(&event).await;
        assert!(result.is_err());
    }
}
