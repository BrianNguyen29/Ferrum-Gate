//! Read-only probe facade for diagnostic operations.
//!
//! This facade provides a read-only diagnostic interface that:
//! - Wraps a minimal transport adapter boundary
//! - Exposes probe operations for health, readiness, and status
//! - Does NOT mutate any ledger or have write-path side effects
//! - Is designed to be usable without real network infrastructure
//!
//! The facade does NOT change the existing ProbeFacade caller-facing contract
//! except for minimal additive wiring to support the transport adapter.

use std::sync::Arc;

use crate::error::Result;
use crate::transport::{BoxedTransport, ProbeResponse, Transport};

/// Read-only probe facade for diagnostic operations.
///
/// This facade is designed to:
/// - Be cheaply fakeable for tests (no real network needed)
/// - Provide clear read-only semantics (no write-path)
/// - Integrate with existing ProbeFacade caller-facing contract
///
/// All operations are diagnostic only and have no side effects
/// on any ledger, store, or external system.
#[derive(Clone)]
pub struct ProbeFacade {
    transport: BoxedTransport,
}

impl std::fmt::Debug for ProbeFacade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeFacade")
            .field("transport", &"BoxedTransport")
            .finish()
    }
}

impl ProbeFacade {
    /// Create a new ProbeFacade wrapping the given transport.
    ///
    /// The transport is consumed into an Arc for cheap cloning.
    pub fn new(transport: impl Transport + 'static) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    /// Create a ProbeFacade from a boxed transport.
    pub fn from_boxed(transport: BoxedTransport) -> Self {
        Self { transport }
    }

    /// Execute a health probe (read-only).
    ///
    /// Returns Ok(()) if the underlying transport is healthy.
    pub async fn health(&self) -> Result<()> {
        self.transport.health_check().await
    }

    /// Execute a readiness probe (read-only).
    ///
    /// Checks if the system is ready to receive traffic.
    pub async fn ready(&self) -> Result<ProbeResponse> {
        self.transport.probe("ready").await
    }

    /// Execute a status probe (read-only).
    ///
    /// Returns detailed status information without any side effects.
    pub async fn status(&self) -> Result<ProbeResponse> {
        self.transport.probe("status").await
    }

    /// Execute a generic probe by kind (read-only).
    ///
    /// Allows probing arbitrary diagnostic categories.
    pub async fn probe(&self, kind: &str) -> Result<ProbeResponse> {
        self.transport.probe(kind).await
    }

    /// Get the underlying transport reference for advanced use cases.
    ///
    /// This should rarely be needed - prefer the facade's built-in probe methods.
    pub fn transport(&self) -> &BoxedTransport {
        &self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    #[tokio::test]
    async fn probe_facade_health_ok() {
        let facade = ProbeFacade::new(FakeTransport::always_healthy());
        assert!(facade.health().await.is_ok());
    }

    #[tokio::test]
    async fn probe_facade_health_err() {
        let facade = ProbeFacade::new(FakeTransport::always_unhealthy());
        assert!(facade.health().await.is_err());
    }

    #[tokio::test]
    async fn probe_facade_ready() {
        let ready_resp = ProbeResponse::ok("ready", "system ready");
        let facade =
            ProbeFacade::new(FakeTransport::new().with_probe_response("ready", ready_resp));
        let resp = facade.ready().await.unwrap();
        assert!(resp.success);
        assert_eq!(resp.message, "system ready");
    }

    #[tokio::test]
    async fn probe_facade_status() {
        let status_resp = ProbeResponse::ok("status", "all systems nominal");
        let facade =
            ProbeFacade::new(FakeTransport::new().with_probe_response("status", status_resp));
        let resp = facade.status().await.unwrap();
        assert!(resp.success);
    }

    #[tokio::test]
    async fn probe_facade_generic_probe() {
        let custom_resp = ProbeResponse::ok("custom", "custom probe result");
        let facade =
            ProbeFacade::new(FakeTransport::new().with_probe_response("custom", custom_resp));
        let resp = facade.probe("custom").await.unwrap();
        assert!(resp.success);
        assert_eq!(resp.message, "custom probe result");
    }

    #[tokio::test]
    async fn probe_facade_default_probe() {
        // When no specific response is configured, should return a default
        let facade = ProbeFacade::new(FakeTransport::new());
        let resp = facade.probe("some-unknown-probe").await.unwrap();
        assert!(resp.success);
        assert!(resp.message.contains("some-unknown-probe"));
    }

    #[tokio::test]
    async fn probe_facade_cloneable() {
        let facade = ProbeFacade::new(FakeTransport::always_healthy());
        let _cloned = facade.clone();
        // Both should work independently
        assert!(facade.health().await.is_ok());
    }
}
