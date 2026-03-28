//! # ferrum-sync: Cross-Node Ledger Sync (Transport Probe + Decision Kernel + Sync-2 Groundwork)
//!
//! This crate implements three layers:
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
//! 3. **Sync-2 Groundwork** (partial): A read-only preflight checker (PF1-PF8) and diff
//!    classifier (`DiffClass`) that operates purely on caller-provided inputs. No transport,
//!    no repo queries, no mutation. This is groundwork aligned with
//!    `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`.
//!
//!    Also includes a read-only repo port (`SyncPreflightRepo`) and supporting
//!    types (`LocalPreflightState`, `SyncRepoError`) for repo-backed preflight
//!    reads. A concrete `SqliteSyncPreflightRepo` implementation exists in
//!    `ferrum-store` (PF1 + PF5); PF2/PF4/PF6/PF7/PF8 are deferred until the
//!    corresponding schema tables are added.
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
//! - Sync-2 groundwork: pure preflight checker (PF1-PF8) + diff classifier (`DiffClass`)
//!   + bridge to Sync-1 decision kernel
//! - Sync-2 repo port: read-only preflight port (`SyncPreflightRepo`) plus
//!   pure adapter (`build_preflight_input`) to bridge `LocalPreflightState`
//!   into `PreflightInput`
//!
//! ## What Is Out of Scope
//!
//! - Entry apply/write-path
//! - Consensus algorithm or leader election
//! - Two-way merge or bidirectional sync
//! - Peer discovery or address management
//! - Full Sync-2 implementation (repo queries, transport-based tip acquisition,
//!   sync session tracking, capability model enforcement)
//! - Concrete `SyncPreflightRepo` implementations (SQLite, in-memory; deferred to P3)

pub mod decision;
pub mod error;
pub mod facade;
pub mod preflight;
pub mod proof;
pub mod repo;
pub mod transport;

// NOTE: http_transport is excluded from the main re-exports above to keep
// the public API surface aligned with the facade boundary: only types that
// appear in ProbeFacadeResponse (LeaderTip) are re-exported at crate root.
// All other transport DTOs and implementations remain internal.
// The http_transport module IS available for production use; downstream code
// that needs the HTTP transport explicitly imports crate::http_transport.
pub mod http_transport;

pub use decision::{DecisionInput, Sync1Decision, TipId, decide};
pub use error::{ProbeError, Sync1AbortCode, map_transport_error_to_abort};
pub use facade::{
    ContinuityProofShape, ProbeFacade, ProbeFacadeConfig, ProbeFacadeRequest, ProbeFacadeResponse,
    ProofStructureInfo,
};
pub use preflight::{
    DiffClass, PreflightCheckCode, PreflightInput, PreflightResult, build_preflight_input,
    classify, diff_class_to_decision, run_preflight,
};
pub use proof::{verify_entry_hashes, verify_proof_structure};
pub use repo::{InMemorySyncPreflightRepo, LocalPreflightState, SyncPreflightRepo, SyncRepoError};
// Only LeaderTip is re-exported at crate root because it appears in
// ProbeFacadeResponse::ProbeOk { tip: LeaderTip }.  All other transport
// DTOs (requests, responses, TransportError, TransportProbe, etc.) are
// internal to the crate and must NOT leak through the public facade
// boundary.  Downstream code that needs the transport layer directly
// (tests, future adapters) imports crate::transport explicitly.
pub use transport::{LeaderTip, PreflightTransportFlags, PreflightTransportInput};
