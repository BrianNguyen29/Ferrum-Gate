//! Minimal transport adapter boundary for read-only sync probes.
//!
//! This module provides a service-internal adapter boundary that wraps a minimal
//! provider trait, avoiding commitment to HTTP, gRPC, or any real network transport.
//!
//! All transport DTOs and errors remain internal to ferrum-sync.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{Result, SyncError};

/// A read-only probe response from the transport layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResponse {
    /// The probe kind (e.g., "health", "ready", "status").
    pub probe_kind: String,
    /// Whether the probe succeeded.
    pub success: bool,
    /// Human-readable message from the probe.
    pub message: String,
    /// Arbitrary metadata from the probe.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ProbeResponse {
    pub fn ok(probe_kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            probe_kind: probe_kind.into(),
            success: true,
            message: message.into(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn error(probe_kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            probe_kind: probe_kind.into(),
            success: false,
            message: message.into(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A tip identity: sequence number + hash.
///
/// This is a lightweight, transport-independent representation of a ledger tip.
/// It is used by both the decision kernel and the leader tip cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipId {
    /// Sequence number of the tip entry.
    pub sequence: u64,
    /// Hash of the tip entry.
    pub hash: String,
}

/// Input for evaluating transport-boundary preflight flags (PF3 and PF8).
///
/// PF3: Is the leader address/identity known?
/// PF8: Is the leader tip available (cached)?
///
/// This struct carries the raw inputs; `evaluate()` derives the boolean flags.
#[derive(Debug, Clone)]
pub struct PreflightTransportInput {
    /// The leader's address. `None` or empty means identity is unknown (PF3 fails).
    pub leader_address: Option<String>,
    /// The cached leader tip, if known. `None` means tip is not cached (PF8 fails).
    pub cached_leader_tip: Option<TipId>,
}

/// Result of evaluating transport-boundary preflight flags.
#[derive(Debug, Clone)]
pub struct PreflightTransportFlags {
    /// PF3: Is the leader address/identity known?
    pub leader_identity_known: bool,
    /// PF8: Is the leader tip available (cached)?
    pub leader_tip_available: bool,
}

impl PreflightTransportInput {
    /// Evaluate PF3 (leader identity known) and PF8 (leader tip available).
    ///
    /// PF3 fails when `leader_address` is `None` or empty.
    /// PF8 fails when `cached_leader_tip` is `None`.
    pub fn evaluate(&self) -> PreflightTransportFlags {
        let leader_identity_known = self
            .leader_address
            .as_ref()
            .map(|a| !a.trim().is_empty())
            .unwrap_or(false);

        let leader_tip_available = self.cached_leader_tip.is_some();

        PreflightTransportFlags {
            leader_identity_known,
            leader_tip_available,
        }
    }
}

/// Minimal read-only transport provider trait.
///
/// This trait is designed to be fakeable - a simple in-memory implementation
/// can satisfy this contract without any real network or write-path.
///
/// Implementations must be Send + Sync to allow use in async contexts.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Execute a read-only probe and return the response.
    ///
    /// This method MUST NOT have any side effects outside of diagnostic reading.
    /// No writes, no mutations, no ledger changes.
    async fn probe(&self, probe_kind: &str) -> Result<ProbeResponse>;

    /// Check if the transport is available (read-only health check).
    ///
    /// Returns Ok(()) if the transport is healthy and can respond to probes.
    async fn health_check(&self) -> Result<()>;
}

/// A fakeable in-memory transport implementation for testing and development.
///
/// This implementation:
/// - Requires NO real network
/// - Has NO write-path
/// - Is fully deterministic and controllable via construction options
///
/// Use this for:
/// - Unit tests that need a Transport without network
/// - Local development without external dependencies
/// - ProbeFacade integration tests
#[derive(Debug, Clone)]
pub struct FakeTransport {
    probe_responses: std::collections::HashMap<String, ProbeResponse>,
    health_ok: bool,
}

impl FakeTransport {
    /// Create a new FakeTransport with default responses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a FakeTransport that always returns healthy.
    pub fn always_healthy() -> Self {
        Self {
            probe_responses: std::collections::HashMap::new(),
            health_ok: true,
        }
    }

    /// Create a FakeTransport that always returns unhealthy.
    pub fn always_unhealthy() -> Self {
        Self {
            probe_responses: std::collections::HashMap::new(),
            health_ok: false,
        }
    }

    /// Set a specific probe response.
    pub fn with_probe_response(mut self, kind: impl Into<String>, response: ProbeResponse) -> Self {
        self.probe_responses.insert(kind.into(), response);
        self
    }

    /// Set a custom health check result.
    pub fn with_health(mut self, ok: bool) -> Self {
        self.health_ok = ok;
        self
    }

    /// Add a default probe response for unknown probe kinds.
    pub fn with_default_probe_response(mut self, response: ProbeResponse) -> Self {
        self.probe_responses.insert("*".to_string(), response);
        self
    }
}

impl Default for FakeTransport {
    fn default() -> Self {
        Self {
            probe_responses: std::collections::HashMap::new(),
            health_ok: true,
        }
    }
}

#[async_trait]
impl Transport for FakeTransport {
    async fn probe(&self, probe_kind: &str) -> Result<ProbeResponse> {
        // Return specific response if set, otherwise default
        if let Some(response) = self.probe_responses.get(probe_kind) {
            return Ok(response.clone());
        }
        // Check for wildcard default
        if let Some(default) = self.probe_responses.get("*") {
            return Ok(default.clone());
        }
        // Fallback: return a generic ok response
        Ok(ProbeResponse::ok(
            probe_kind,
            format!("fake probe for '{}' - no response configured", probe_kind),
        ))
    }

    async fn health_check(&self) -> Result<()> {
        if self.health_ok {
            Ok(())
        } else {
            Err(SyncError::transport("fake transport unhealthy"))
        }
    }
}

/// Boxed transport for type erasure.
pub type BoxedTransport = Arc<dyn Transport>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_transport_health_ok() {
        let transport = FakeTransport::always_healthy();
        assert!(transport.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn fake_transport_health_err() {
        let transport = FakeTransport::always_unhealthy();
        assert!(transport.health_check().await.is_err());
    }

    #[tokio::test]
    async fn fake_transport_probe_default() {
        let transport = FakeTransport::new();
        let resp = transport.probe("health").await.unwrap();
        assert!(resp.success);
        assert_eq!(resp.probe_kind, "health");
    }

    #[tokio::test]
    async fn fake_transport_probe_custom() {
        let custom_resp = ProbeResponse::error("custom", "custom error");
        let transport = FakeTransport::new().with_probe_response("custom", custom_resp.clone());

        let resp = transport.probe("custom").await.unwrap();
        assert!(!resp.success);
        assert_eq!(resp.message, "custom error");
    }

    #[tokio::test]
    async fn fake_transport_probe_wildcard_default() {
        let default_resp = ProbeResponse::ok("default-kind", "using wildcard");
        let transport = FakeTransport::new().with_default_probe_response(default_resp.clone());

        let resp = transport.probe("unknown-probe").await.unwrap();
        assert!(resp.success);
        assert_eq!(resp.probe_kind, "default-kind");
    }

    // ========================================================================
    // PreflightTransportInput tests
    // ========================================================================

    #[test]
    fn preflight_transport_flags_all_present() {
        let tip = TipId {
            sequence: 10,
            hash: "abc".to_string(),
        };
        let flags = PreflightTransportInput {
            leader_address: Some("leader:9000".to_string()),
            cached_leader_tip: Some(tip),
        }
        .evaluate();

        assert!(flags.leader_identity_known);
        assert!(flags.leader_tip_available);
    }

    #[test]
    fn preflight_transport_flags_missing_address() {
        let tip = TipId {
            sequence: 10,
            hash: "abc".to_string(),
        };
        let flags = PreflightTransportInput {
            leader_address: None,
            cached_leader_tip: Some(tip),
        }
        .evaluate();

        assert!(!flags.leader_identity_known);
        assert!(flags.leader_tip_available);
    }

    #[test]
    fn preflight_transport_flags_empty_address() {
        let tip = TipId {
            sequence: 10,
            hash: "abc".to_string(),
        };
        let flags = PreflightTransportInput {
            leader_address: Some("".to_string()),
            cached_leader_tip: Some(tip),
        }
        .evaluate();

        assert!(!flags.leader_identity_known);
        assert!(flags.leader_tip_available);
    }

    #[test]
    fn preflight_transport_flags_whitespace_address() {
        let tip = TipId {
            sequence: 10,
            hash: "abc".to_string(),
        };
        let flags = PreflightTransportInput {
            leader_address: Some("   ".to_string()),
            cached_leader_tip: Some(tip),
        }
        .evaluate();

        assert!(!flags.leader_identity_known);
        assert!(flags.leader_tip_available);
    }

    #[test]
    fn preflight_transport_flags_missing_tip() {
        let flags = PreflightTransportInput {
            leader_address: Some("leader:9000".to_string()),
            cached_leader_tip: None,
        }
        .evaluate();

        assert!(flags.leader_identity_known);
        assert!(!flags.leader_tip_available);
    }

    #[test]
    fn preflight_transport_flags_both_missing() {
        let flags = PreflightTransportInput {
            leader_address: None,
            cached_leader_tip: None,
        }
        .evaluate();

        assert!(!flags.leader_identity_known);
        assert!(!flags.leader_tip_available);
    }

    #[test]
    fn tip_id_equality() {
        let t1 = TipId {
            sequence: 10,
            hash: "abc".to_string(),
        };
        let t2 = TipId {
            sequence: 10,
            hash: "abc".to_string(),
        };
        let t3 = TipId {
            sequence: 10,
            hash: "def".to_string(),
        };
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
    }
}
