//! Strict mapping from bridge-local `McpRuntimeEvent` to `ExternalEventIngestRequest`.
//!
//! This module is deliberately narrow: it only handles the conversion that
//! this bridge requires. No magic, no inference, no fallbacks.
//!
//! # Validation rules
//! - `execution_id` and `parent_event_id` are passed through unchanged (caller guarantees existence).
//! - `source_system` is copied from the bridge config (never per-call override).
//! - `source_event_id` is constructed as `{event_type_tag}:{uuid}` for uniqueness.
//! - `observed_at` is taken from the event variant if set, otherwise None (server uses current time).
//! - `summary` is taken from the event variant if set.
//! - `payload_digest` is a SHA-256 digest of the canonical JSON serialization of the event variant.
//! - `metadata` is omitted (None) unless the bridge needs to inject bridge-level keys in future.

use ferrum_proto::{EventId, ExecutionId, ExternalEventIngestRequest, Timestamp};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::Error;
use crate::types::McpRuntimeEvent;

/// SHA-256 hex digest of the canonical JSON bytes of an event variant.
fn compute_payload_digest(event: &McpRuntimeEvent) -> Result<String, Error> {
    let json_bytes = serde_json::to_vec(event)
        .map_err(|e| Error::validation(format!("serialization failed: {}", e)))?;
    let mut hasher = Sha256::new();
    hasher.update(&json_bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

/// Map a bridge-local runtime event to a gateway `ExternalEventIngestRequest`.
///
/// # Parameters
/// - `execution_id`: required anchor; must refer to an existing execution record in the gateway.
/// - `parent_event_id`: required anchor; must refer to an existing provenance event in the same execution.
/// - `source_system`: bridge-owned config string; copied directly into the request.
/// - `event`: the MCP runtime event to map.
///
/// # Errors
/// - Serialization errors if the event cannot be canonicalized to JSON.
pub fn map_to_ingest_request(
    execution_id: ExecutionId,
    parent_event_id: EventId,
    source_system: &str,
    event: &McpRuntimeEvent,
) -> Result<ExternalEventIngestRequest, Error> {
    // Validate non-empty source_system (bridge config is trusted but we guard against empty)
    if source_system.is_empty() {
        return Err(Error::validation(
            "source_system must not be empty (bridge config error)",
        ));
    }

    let source_event_id = format!("{}:{}", event.event_type_tag(), Uuid::new_v4());

    let observed_at: Option<Timestamp> = event.observed_at();

    let summary = Some(event.summary_or_default());

    let payload_digest = Some(compute_payload_digest(event)?);

    // Metadata: empty for this slice. Future slices can inject bridge-level keys here.
    let metadata = None;

    Ok(ExternalEventIngestRequest {
        execution_id,
        parent_event_id,
        source_system: source_system.to_string(),
        source_event_id,
        observed_at,
        summary,
        payload_digest,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GenericExternalEvent, SessionStarted, ToolCallCompleted, ToolCallStarted};
    use chrono::TimeZone;
    use chrono::Utc;

    fn dummy_execution_id() -> ExecutionId {
        // Valid UUID format for testing
        ExecutionId(uuid::Uuid::nil())
    }

    fn dummy_parent_event_id() -> EventId {
        EventId(uuid::Uuid::nil())
    }

    #[test]
    fn test_tool_call_completed_mapping() {
        let event = McpRuntimeEvent::ToolCallCompleted(ToolCallCompleted {
            tool_name: "bash".into(),
            input_json: r#"{"cmd": "echo hello"}"#.into(),
            output_json: Some(r#"{"stdout":"hello\n"}"#.into()),
            exit_code: 0,
            completed_at: None,
            summary: None,
        });

        let req = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-test",
            &event,
        )
        .unwrap();

        assert_eq!(req.source_system, "mcp-test");
        assert!(req.source_event_id.starts_with("mcp.tool.completed:"));
        assert!(req.observed_at.is_none()); // None because completed_at was None
        assert!(req.summary.is_some());
        assert!(req.payload_digest.is_some());
        assert!(req.payload_digest.as_ref().unwrap().starts_with("sha256:"));
        assert!(req.metadata.is_none());
    }

    #[test]
    fn test_tool_call_started_mapping() {
        let event = McpRuntimeEvent::ToolCallStarted(ToolCallStarted {
            tool_name: "read_file".into(),
            input_json: r#"{"path": "/etc/passwd"}"#.into(),
            started_at: None,
            summary: None,
        });

        let req = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-prod",
            &event,
        )
        .unwrap();

        assert_eq!(req.source_system, "mcp-prod");
        assert!(req.source_event_id.starts_with("mcp.tool.started:"));
        assert_eq!(req.summary.unwrap(), "read_file started");
    }

    #[test]
    fn test_session_started_mapping() {
        let event = McpRuntimeEvent::SessionStarted(SessionStarted {
            session_id: "sess-abc123".into(),
            transport_type: Some("stdio".into()),
            started_at: None,
            summary: None,
        });

        let req = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-dev",
            &event,
        )
        .unwrap();

        assert_eq!(req.source_system, "mcp-dev");
        assert!(req.source_event_id.starts_with("mcp.session.started:"));
        assert_eq!(req.summary.unwrap(), "session sess-abc123 started");
    }

    #[test]
    fn test_generic_external_event_mapping() {
        let event = McpRuntimeEvent::GenericExternalEvent(GenericExternalEvent {
            event_type: "mcp.custom.thing".into(),
            payload_json: r#"{"foo":"bar"}"#.into(),
            observed_at: None,
            summary: Some("a custom thing happened".into()),
        });

        let req = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-custom",
            &event,
        )
        .unwrap();

        assert_eq!(req.source_system, "mcp-custom");
        assert!(req.source_event_id.starts_with("mcp.generic:"));
        assert_eq!(req.summary.unwrap(), "a custom thing happened");
    }

    #[test]
    fn test_observed_at_passed_through() {
        let fixed_time = Utc.with_ymd_and_hms(2025, 1, 15, 0, 0, 0).unwrap();
        let event = McpRuntimeEvent::ToolCallCompleted(ToolCallCompleted {
            tool_name: "test".into(),
            input_json: "{}".into(),
            output_json: None,
            exit_code: 0,
            completed_at: Some(fixed_time),
            summary: None,
        });

        let req = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-test",
            &event,
        )
        .unwrap();

        // observed_at must be set when the event carries a timestamp
        assert!(req.observed_at.is_some());
        // It must equal the value from the event (Timestamp is DateTime<Utc>)
        assert_eq!(req.observed_at.unwrap(), fixed_time);
    }

    #[test]
    fn test_empty_source_system_rejected() {
        let event = McpRuntimeEvent::ToolCallStarted(ToolCallStarted {
            tool_name: "test".into(),
            input_json: "{}".into(),
            started_at: None,
            summary: None,
        });

        let result =
            map_to_ingest_request(dummy_execution_id(), dummy_parent_event_id(), "", &event);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Validation(_)));
    }

    #[test]
    fn test_payload_digest_deterministic() {
        let event = McpRuntimeEvent::ToolCallCompleted(ToolCallCompleted {
            tool_name: "bash".into(),
            input_json: r#"{"cmd": "echo hello"}"#.into(),
            output_json: Some(r#"{"stdout":"hello\n"}"#.into()),
            exit_code: 0,
            completed_at: None,
            summary: None,
        });

        let req1 = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-test",
            &event,
        )
        .unwrap();
        let req2 = map_to_ingest_request(
            dummy_execution_id(),
            dummy_parent_event_id(),
            "mcp-test",
            &event,
        )
        .unwrap();

        // source_event_id differs (UUID), but payload_digest must be identical
        assert_eq!(req1.payload_digest, req2.payload_digest);
    }
}
