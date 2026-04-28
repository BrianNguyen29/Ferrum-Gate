//! # ferrum-sync
//!
//! Read-only sync probe facade and fakeable transport adapter.
//!
//! ## Overview
//!
//! This crate provides:
//! - [`ProbeFacade`]: A read-only diagnostic facade for health, readiness, and status probes
//! - [`Transport`]: A minimal provider trait for transport operations (fakeable)
//! - [`FakeTransport`]: An in-memory transport implementation for tests/development
//! - [`ExternalEventSource`]: A trait for polling external event sources (e.g., MCP runtimes)
//! - [`FakeExternalEventSource`]: An in-memory external event source for tests/development
//! - Decision kernel, preflight checker, and diff classifier for Sync-1/Sync-2
//! - External event source polling for provenance events (Sync-3)
//!
//! ## Design Principles
//!
//! - **Read-only only**: No write-path, no ledger mutations, no side effects
//! - **Fakeable**: Transport trait can be satisfied with in-memory implementations
//! - **No real network**: Designed to work without HTTP, gRPC, or external dependencies
//! - **Internal DTOs**: All transport DTOs and errors stay within this crate
//! - [`RuntimeBridge`]: Extended trait for runtime bridges with tool discovery and event submission
//! - [`McpBridge`]: MCP (Model Context Protocol) bridge implementation with Transport wrapper

pub mod decision;
pub mod error;
pub mod external_source;
pub mod facade;
pub mod mcp_bridge;
pub mod preflight;
pub mod repo;
pub mod transport;

// Re-exports for convenience
pub use decision::{DecisionInput, Sync1Decision, TipId, decide};
pub use error::{Result, Sync1AbortCode, SyncError};
pub use external_source::{ExternalEventSource, FakeExternalEventSource};
pub use facade::ProbeFacade;
pub use mcp_bridge::{BridgeSubmitResult, BridgeToolInfo, McpBridge, RuntimeBridge};
pub use preflight::{
    DiffClass, PreflightCheckCode, PreflightInput, PreflightResult, build_preflight_input,
    classify, diff_class_to_decision, run_preflight,
};
pub use repo::{InMemorySyncPreflightRepo, LocalPreflightState, SyncPreflightRepo, SyncRepoError};
pub use transport::{
    BoxedTransport, FakeTransport, PreflightTransportFlags, PreflightTransportInput, ProbeResponse,
    TipId as TransportTipId, Transport,
};
