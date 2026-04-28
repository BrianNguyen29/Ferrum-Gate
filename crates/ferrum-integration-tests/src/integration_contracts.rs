//! Integration tests for contract conformance and related behavior.
//!
//! This file contains contract-related integration tests that complement
//! the behavior quality tests in integration_gateway_flow.rs.
//!
//! Currently these tests focus on contract preparation and state transitions.

use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::RollbackClass;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{SqliteStore, StoreFacade};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Contract preparation tests
// ---------------------------------------------------------------------------

/// Verify that R3 contracts are created with the correct rollback class
/// and that preparation succeeds.
#[tokio::test]
async fn test_r3_contract_preparation_succeeds() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);

    // Create R3 prepare request
    let r3_request = runtime.rollback.default_prepare_request(
        ferrum_proto::IntentId::new(),
        ferrum_proto::ProposalId::new(),
        ferrum_proto::ExecutionId::new(),
        RollbackClass::R3IrreversibleHighConsequence,
    );

    let r3_response = runtime
        .rollback
        .prepare(r3_request)
        .await
        .expect("prepare R3 should succeed");

    assert!(r3_response.accepted, "R3 prepare should be accepted");
    assert_eq!(
        r3_response.contract.rollback_class,
        RollbackClass::R3IrreversibleHighConsequence
    );
}

/// Verify that R0 contracts are created with auto_commit=true.
#[tokio::test]
async fn test_r0_contract_has_auto_commit_true() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);

    // Create R0 prepare request
    let r0_request = runtime.rollback.default_prepare_request(
        ferrum_proto::IntentId::new(),
        ferrum_proto::ProposalId::new(),
        ferrum_proto::ExecutionId::new(),
        RollbackClass::R0NativeReversible,
    );

    let r0_response = runtime
        .rollback
        .prepare(r0_request)
        .await
        .expect("prepare R0 should succeed");

    assert!(r0_response.accepted, "R0 prepare should be accepted");
    assert!(
        r0_response.contract.auto_commit,
        "R0 contract should have auto_commit=true"
    );
}
