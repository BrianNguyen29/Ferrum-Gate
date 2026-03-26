//! `McpBridge`: the main public type for the observation-only MCP event bridge.
//!
//! This bridge is constructed with a gateway URL and a `source_system` tag,
//! then used to ingest `McpRuntimeEvent`s into the provenance graph by
//! anchoring them to an explicit (execution_id, parent_event_id) pair.
//!
//! # Guardrail
//! `source_system` is set at construction time and stored in the bridge.
//! It is never overridden per-call. This keeps the bridge configuration
//! explicit and auditable.

use ferrum_proto::{EventId, ExecutionId, ProvenanceEvent};

use crate::client::HttpSink;
use crate::error::Error;
use crate::mapping::map_to_ingest_request;
use crate::types::McpRuntimeEvent;

/// Observation-only bridge for mapping MCP/runtime events into FerrumGate provenance.
///
/// Construct with `McpBridge::new(gateway_url, source_system)`.
///
/// Each `ingest` call requires explicit lineage anchors: `execution_id` and
/// `parent_event_id`. The bridge maps the event and POSTs it to the gateway's
/// external-event ingest endpoint.
///
/// # Differences from a full MCP server
/// - No protocol parsing (no LSP/jsonrpc framing)
/// - No session management
/// - No subscription/push from gateway to client
/// - No auto-correlation or anchor resolution
///
/// This is a minimal scaffold; future slices can extend it into a full MCP client.
#[derive(Debug, Clone)]
pub struct McpBridge {
    sink: HttpSink,
    source_system: String,
}

impl McpBridge {
    /// Construct a new bridge.
    ///
    /// `gateway_url` is the base URL of the FerrumGate gateway (e.g. `"http://localhost:8080"`).
    /// `source_system` is a stable, bridge-owned identifier for this event source
    /// (e.g. `"mcp-claude-desktop"`, `"cursor-ide"`). Must be non-empty.
    pub fn new(gateway_url: &str, source_system: &str) -> Result<Self, Error> {
        if source_system.is_empty() {
            return Err(Error::validation("source_system must not be empty"));
        }

        let sink = HttpSink::new(gateway_url)?;

        Ok(Self {
            sink,
            source_system: source_system.to_string(),
        })
    }

    /// Ingest a single MCP runtime event into the provenance graph.
    ///
    /// Requires explicit lineage anchors:
    /// - `execution_id` — must refer to an existing execution in the gateway store.
    /// - `parent_event_id` — must refer to an existing provenance event in the same execution.
    ///
    /// Returns the newly created `ProvenanceEvent` on success.
    ///
    /// # Errors
    /// - `Error::Validation` if the event cannot be serialized or the request is invalid.
    /// - `Error::Gateway` if the gateway returns a non-2xx response.
    /// - `Error::Http` for transport-level errors.
    pub async fn ingest(
        &self,
        execution_id: ExecutionId,
        parent_event_id: EventId,
        event: McpRuntimeEvent,
    ) -> Result<ProvenanceEvent, Error> {
        let request =
            map_to_ingest_request(execution_id, parent_event_id, &self.source_system, &event)?;

        tracing::debug!(
            event_type = %event.event_type_tag(),
            execution_id = %request.execution_id,
            parent_event_id = %request.parent_event_id,
            source_event_id = %request.source_event_id,
            "ingesting MCP runtime event"
        );

        let response = self.sink.post(&request).await?;

        tracing::debug!(
            event_id = %response.event.event_id,
            "MCP runtime event ingested successfully"
        );

        Ok(response.event)
    }

    /// Returns the configured source_system tag.
    pub fn source_system(&self) -> &str {
        &self.source_system
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_construction_valid() {
        let bridge = McpBridge::new("http://localhost:8080", "mcp-test");
        assert!(bridge.is_ok());
        assert_eq!(bridge.unwrap().source_system(), "mcp-test");
    }

    #[test]
    fn test_bridge_construction_empty_source_system() {
        let bridge = McpBridge::new("http://localhost:8080", "");
        assert!(bridge.is_err());
        assert!(matches!(bridge.unwrap_err(), Error::Validation(_)));
    }

    #[test]
    fn test_bridge_construction_invalid_url() {
        let bridge = McpBridge::new("not-a-url", "mcp-test");
        assert!(bridge.is_err());
    }
}
