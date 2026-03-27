//! # ferrum-sync: Read-Only Transport Probe for Cross-Node Ledger Sync
//!
//! This crate implements Sync-3a: a diagnostic-only transport probe that exercises
//! the Sync-3 transport contracts without committing any state. Its purpose is to
//! validate transport connectivity, error mapping, and proof structure before any
//! write-path work begins.
//!
//! ## What Is In Scope (Sync-3a Only)
//!
//! - Diagnostic tip fetch: verify leader is reachable and returning consistent tip data
//! - Diagnostic proof fetch: verify proof retrieval returns well-formed proofs
//! - Local proof structure verification: verify proof has correct shape, non-empty ranges,
//!   and hash continuity without applying entries
//! - Abort-code mapping validation: confirm all transport error variants map to Sync-1
//!   abort codes per the fail-closed table
//!
//! ## What Is Out of Scope
//!
//! - Entry apply/write-path
//! - Consensus algorithm or leader election
//! - Two-way merge or bidirectional sync
//! - Peer discovery or address management

pub mod error;
pub mod facade;
pub mod proof;
pub mod transport;

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
