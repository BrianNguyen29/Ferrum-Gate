//! # ferrum-sync: Cross-Node Ledger Sync (Transport Probe + Decision Kernel)
//!
//! This crate implements two layers:
//!
//! 1. **Sync-3a / Sync-3a.1**: A diagnostic-only transport probe (`ProbeFacade`) that
//!    exercises Sync-3 transport contracts without committing any state. It validates
//!    transport connectivity, error mapping, and proof structure before any write-path
//!    work begins.
//!
//! 2. **Sync-1 Decision Kernel**: A pure, read-only decision function that implements
//!    the one-way fast-forward sync decision table. Given follower state and leader state,
//!    it returns the correct Sync-1 decision (DONE / SYNC / FAST_FORWARD / ABORT) with
//!    no side effects, no transport, and no mutation.
//!
//! ## What Is In Scope
//!
//! - Diagnostic tip fetch: verify leader is reachable and returning consistent tip data
//! - Diagnostic proof fetch: verify proof retrieval returns well-formed proofs
//! - Local proof structure verification: verify proof has correct shape, non-empty ranges,
//!   and hash continuity without applying entries
//! - Abort-code mapping validation: confirm all transport error variants map to Sync-1
//!   abort codes per the fail-closed table
//! - Sync-1 decision kernel: pure decision table for one-way fast-forward sync
//!
//! ## What Is Out of Scope
//!
//! - Entry apply/write-path
//! - Consensus algorithm or leader election
//! - Two-way merge or bidirectional sync
//! - Peer discovery or address management

pub mod decision;
pub mod error;
pub mod facade;
pub mod proof;
pub mod transport;

pub use decision::{DecisionInput, Sync1Decision, TipId, decide};
pub use error::{ProbeError, Sync1AbortCode, map_transport_error_to_abort};
pub use facade::{
    ContinuityProofShape, ProbeFacade, ProbeFacadeConfig, ProbeFacadeRequest, ProbeFacadeResponse,
    ProofStructureInfo,
};
pub use proof::{verify_entry_hashes, verify_proof_structure};
pub use transport::{
    FakeLeaderTransport, LeaderTip, LeaderTipRequest, LeaderTipResponse, ProbeResult, Proof,
    ProofRequest, ProofResponse, TransportError, TransportProbe,
};
