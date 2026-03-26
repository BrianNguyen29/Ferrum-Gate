//! ferrum-integrations-mcp: observation-only bridge for mapping external
//! MCP/runtime events into the FerrumGate provenance graph via the existing
//! external event ingest API.
//!
//! This crate does NOT implement a full MCP transport loop. It provides a
//! minimal scaffold that future slices can extend.
//!
//! # Guardrails (this slice only)
//! - No auto anchor resolution; callers supply explicit execution_id + parent_event_id.
//! - No retries; one-shot POST.
//! - No auto-correlation or background replay worker.
//! - source_system is bridge-owned config, not per-call override.
//!
//! # Example
//! ```ignore
//! let bridge = McpBridge::new("http://localhost:8080", "mcp-dev")?;
//! let event = McpRuntimeEvent::ToolCallCompleted {
//!     tool_name: "bash".into(),
//!     input_json: r#"{"cmd": "echo hello"}"#.into(),
//!     output_json: Some(r#"{"stdout":"hello\n"}"#.into()),
//!     exit_code: 0,
//! };
//! let provenance = bridge.ingest(execution_id, parent_event_id, event).await?;
//! ```

pub mod bridge;
pub mod client;
pub mod error;
pub mod mapping;
pub mod types;
pub mod vendors;

// Re-exports for ergonomic public API
pub use bridge::McpBridge;
pub use error::Error;
pub use types::McpRuntimeEvent;
