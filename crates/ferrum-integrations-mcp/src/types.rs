//! Local Rust types for MCP runtime events that flow through the bridge.
//!
//! These are bridge-local types; they are mapped to `ExternalEventIngestRequest`
//! before being posted to the gateway.
//!
//! All event variants carry at minimum the fields needed to anchor them in
//! an execution lineage (execution_id, parent_event_id) and to produce a
//! deterministic payload digest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A call to an MCP tool that has completed (success or failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallCompleted {
    /// Name of the tool that was invoked.
    pub tool_name: String,
    /// JSON-encoded input arguments passed to the tool.
    pub input_json: String,
    /// Optional JSON-encoded return value. None indicates the tool returned
    /// no output or output was suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_json: Option<String>,
    /// Numeric exit code. 0 indicates success; non-zero indicates failure.
    pub exit_code: i32,
    /// Optional wall-clock time when the call completed.
    /// If omitted, the bridge uses the current time at ingest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Optional human-readable summary of the call outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// A notification that an MCP tool call started (fire-and-forget from the bridge).
/// The bridge emits this when it first dispatches a tool call to the runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStarted {
    /// Name of the tool being invoked.
    pub tool_name: String,
    /// JSON-encoded input arguments.
    pub input_json: String,
    /// Optional wall-clock time when the call was dispatched.
    /// If omitted, the bridge uses the current time at ingest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Optional human-readable description of what will run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// A notification that an MCP session or connection was established.
/// This can be used to anchor session-level context in the lineage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStarted {
    /// Opaque session identifier assigned by the MCP runtime.
    pub session_id: String,
    /// Optional endpoint or transport type of the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// Optional wall-clock time when the session began.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Optional human-readable summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// A generic external event with an opaque type tag and JSON payload.
/// Use this for events that do not fit the above variants but still need
/// to be recorded in the provenance graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericExternalEvent {
    /// Stable type identifier, e.g. "mcp.tool.execution" or "custom.event".
    pub event_type: String,
    /// JSON-encoded event payload.
    pub payload_json: String,
    /// Optional wall-clock time when the event was observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<DateTime<Utc>>,
    /// Optional human-readable summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// One-of enum covering all MCP runtime event types supported by this bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum McpRuntimeEvent {
    ToolCallCompleted(ToolCallCompleted),
    ToolCallStarted(ToolCallStarted),
    SessionStarted(SessionStarted),
    GenericExternalEvent(GenericExternalEvent),
}

impl McpRuntimeEvent {
    /// Returns a stable event-type string for use in source_event_id.
    pub fn event_type_tag(&self) -> &'static str {
        match self {
            McpRuntimeEvent::ToolCallCompleted(_) => "mcp.tool.completed",
            McpRuntimeEvent::ToolCallStarted(_) => "mcp.tool.started",
            McpRuntimeEvent::SessionStarted(_) => "mcp.session.started",
            McpRuntimeEvent::GenericExternalEvent(_) => "mcp.generic",
        }
    }

    /// Returns a short human-readable label for logging/debugging.
    pub fn label(&self) -> &'static str {
        match self {
            McpRuntimeEvent::ToolCallCompleted(_) => "ToolCallCompleted",
            McpRuntimeEvent::ToolCallStarted(_) => "ToolCallStarted",
            McpRuntimeEvent::SessionStarted(_) => "SessionStarted",
            McpRuntimeEvent::GenericExternalEvent(_) => "GenericExternalEvent",
        }
    }

    /// Returns the optional summary if set, otherwise a short default.
    pub fn summary_or_default(&self) -> String {
        match self {
            McpRuntimeEvent::ToolCallCompleted(e) => e
                .summary
                .clone()
                .unwrap_or_else(|| format!("{} completed (exit={})", e.tool_name, e.exit_code)),
            McpRuntimeEvent::ToolCallStarted(e) => e
                .summary
                .clone()
                .unwrap_or_else(|| format!("{} started", e.tool_name)),
            McpRuntimeEvent::SessionStarted(e) => e
                .summary
                .clone()
                .unwrap_or_else(|| format!("session {} started", e.session_id)),
            McpRuntimeEvent::GenericExternalEvent(e) => e
                .summary
                .clone()
                .unwrap_or_else(|| format!("{} event", e.event_type)),
        }
    }

    /// Returns the wall-clock timestamp embedded in the event variant, if any.
    pub fn observed_at(&self) -> Option<chrono::DateTime<Utc>> {
        match self {
            McpRuntimeEvent::ToolCallCompleted(e) => e.completed_at,
            McpRuntimeEvent::ToolCallStarted(e) => e.started_at,
            McpRuntimeEvent::SessionStarted(e) => e.started_at,
            McpRuntimeEvent::GenericExternalEvent(e) => e.observed_at,
        }
    }
}
