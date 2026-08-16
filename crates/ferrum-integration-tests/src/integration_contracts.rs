//! Integration tests for contract conformance and related behavior.
//!
//! This file contains contract-related integration tests that complement
//! the behavior quality tests in integration_gateway_flow.rs.
//!
//! Currently these tests focus on contract preparation and state transitions.

use ferrum_proto::RollbackClass;
use ferrum_testkit::gateway::SqliteGateway;

// ---------------------------------------------------------------------------
// Contract preparation tests
// ---------------------------------------------------------------------------

/// Verify that R3 contracts are created with the correct rollback class
/// and that preparation succeeds.
#[tokio::test]
async fn test_r3_contract_preparation_succeeds() {
    let gateway = SqliteGateway::new().await.expect("create sqlite gateway");
    let runtime = gateway.runtime;

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
    let gateway = SqliteGateway::new().await.expect("create sqlite gateway");
    let runtime = gateway.runtime;

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
