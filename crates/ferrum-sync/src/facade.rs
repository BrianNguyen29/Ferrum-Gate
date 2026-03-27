//! Sync-3a.2: Probe Facade — clean caller-facing boundary over TransportProbe.
//!
//! This module provides a thin, read-only facade that hides all transport DTOs
//! and internal error taxonomy from callers. The facade contract is:
//!
//! ## Caller Input
//!
//! `ProbeFacadeRequest` contains only explicit, necessary inputs:
//! - `follower_identity`: which node is probing
//! - `follower_tip_sequence`: follower's current tip (start of range to probe)
//! - `probe_count`: N for multi-probe consistency check (default 3)
//!
//! ## Caller Output
//!
//! `ProbeFacadeResponse` is either:
//! - `ProbeOk { tip, proof_structure }` — success with shape-only proof info
//! - `ProbeAborted { code }` — failure, code is the only failure information
//!
//! ## Guarantees
//!
//! - **Read-only**: no local ledger state is modified
//! - **Abort-only failures**: no transport DTOs or error variants leak through
//! - **Shape-only proof**: caller receives proof structure, not apply-ready entries
//!
//! This facade is a pure adapter layer over `TransportProbe`; it adds no
//! diagnostic logic of its own.

use crate::error::Sync1AbortCode;
use crate::proof::verify_proof_structure;
use crate::transport::{LeaderTip, Proof, Transport, TransportProbe};
use ferrum_proto::Sha256Hex;

/// Shape-only proof information returned to callers.
///
/// This is deliberately limited: caller cannot apply entries or perform
/// cryptographic proof verification (requires apply-phase anchor). The facade
/// contract guarantees only structure shape information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStructureInfo {
    /// Number of entries in the range.
    pub entry_count: usize,
    /// Hash of the entry range (not cryptographically verified at facade level).
    pub range_hash: Sha256Hex,
    /// Continuity proof shape information.
    pub continuity_proof_shape: ContinuityProofShape,
}

/// Shape of the continuity proof (Merkle proof nodes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityProofShape {
    /// Number of proof nodes.
    pub node_count: usize,
    /// Number of leaves in the original Merkle tree.
    pub leaf_count: u64,
}

impl ProofStructureInfo {
    /// Extract shape-only info from a full `Proof`.
    ///
    /// This is the only conversion point from internal proof representation
    /// to facade-visible shape info. No entry content escapes this boundary.
    fn from_proof(proof: &Proof) -> Self {
        Self {
            entry_count: proof.entries.len(),
            range_hash: proof.range_hash.clone(),
            continuity_proof_shape: ContinuityProofShape {
                node_count: proof.continuity_proof.nodes.len(),
                leaf_count: proof.continuity_proof.leaf_count,
            },
        }
    }
}

/// Caller-facing request for the probe facade.
///
/// This is the only input type callers use. All transport DTOs
/// (`LeaderTipRequest`, `ProofRequest`, etc.) are internal internals.
#[derive(Debug, Clone)]
pub struct ProbeFacadeRequest {
    /// Identity of the follower node performing the probe.
    pub follower_identity: String,
    /// Follower's current tip sequence (start of range to probe).
    pub follower_tip_sequence: u64,
    /// Number of consistency probes to perform (default 3).
    pub probe_count: usize,
}

impl ProbeFacadeRequest {
    /// Create a new probe request with default settings.
    pub fn new(follower_identity: impl Into<String>, follower_tip_sequence: u64) -> Self {
        Self {
            follower_identity: follower_identity.into(),
            follower_tip_sequence,
            probe_count: 3,
        }
    }

    /// Set the probe count (for custom consistency requirements).
    pub fn with_probe_count(mut self, count: usize) -> Self {
        self.probe_count = count;
        self
    }
}

/// Caller-facing response from the probe facade.
///
/// This is the only output type callers receive. All transport DTOs
/// and internal error taxonomy are hidden behind `ProbeAborted`.
#[derive(Debug, Clone)]
pub enum ProbeFacadeResponse {
    /// Probe succeeded with tip and proof structure.
    ProbeOk {
        /// The leader's tip at probe time.
        tip: LeaderTip,
        /// Shape-only proof structure information.
        proof_structure: ProofStructureInfo,
    },
    /// Probe failed; failure is always fail-closed.
    ProbeAborted {
        /// The Sync-1 abort code representing the failure.
        code: Sync1AbortCode,
    },
}

impl ProbeFacadeResponse {
    /// Returns true if the probe succeeded.
    pub fn is_ok(&self) -> bool {
        matches!(self, ProbeFacadeResponse::ProbeOk { .. })
    }

    /// Returns true if the probe aborted.
    pub fn is_aborted(&self) -> bool {
        matches!(self, ProbeFacadeResponse::ProbeAborted { .. })
    }

    /// Returns the abort code if aborted, None otherwise.
    pub fn abort_code(&self) -> Option<Sync1AbortCode> {
        match self {
            ProbeFacadeResponse::ProbeOk { .. } => None,
            ProbeFacadeResponse::ProbeAborted { code } => Some(*code),
        }
    }
}

/// A read-only probe facade over a transport.
///
/// This facade provides a clean boundary between callers and the internal
/// `TransportProbe` implementation. Callers interact only with:
/// - `ProbeFacadeRequest` for input
/// - `ProbeFacadeResponse` for output
///
/// All transport DTOs, error taxonomy, and proof structure remain internal.
///
/// # Type Parameters
///
/// - `T`: The underlying transport implementation.
#[derive(Debug)]
pub struct ProbeFacade<T: Transport> {
    inner: TransportProbe<T>,
}

impl<T: Transport> ProbeFacade<T> {
    /// Create a new probe facade wrapping the given transport.
    pub fn new(transport: T) -> Self {
        Self {
            inner: TransportProbe::new(transport),
        }
    }

    /// Create a new probe facade with custom probe count.
    pub fn with_probes(transport: T, probe_count: usize) -> Self {
        Self {
            inner: TransportProbe::with_probes(transport, probe_count),
        }
    }

    /// Run the diagnostic probe and return only facade-visible results.
    ///
    /// This is the main entry point for callers. It:
    /// 1. Delegates to `TransportProbe::run()` internally
    /// 2. Maps the internal `ProbeSuccess` to `ProbeOk { tip, proof_structure }`
    /// 3. Maps `Sync1AbortCode` directly to `ProbeAborted { code }`
    ///
    /// No transport DTOs, request IDs, or error taxonomy escape this boundary.
    ///
    /// # Returns
    ///
    /// - `ProbeFacadeResponse::ProbeOk { tip, proof_structure }` on success
    /// - `ProbeFacadeResponse::ProbeAborted { code }` on any failure
    pub async fn probe(&self, request: &ProbeFacadeRequest) -> ProbeFacadeResponse {
        let result = self
            .inner
            .run_with_probe_count(
                &request.follower_identity,
                request.follower_tip_sequence,
                request.probe_count,
            )
            .await;

        match result {
            Ok(success) => {
                // Validate proof structure before returning shape info
                // This is redundant with TransportProbe internals but confirms
                // the facade contract: only valid shapes escape
                if let Err(proof_err) = verify_proof_structure(&success.proof) {
                    return ProbeFacadeResponse::ProbeAborted {
                        code: proof_err.to_abort_code(),
                    };
                }

                ProbeFacadeResponse::ProbeOk {
                    tip: success.tip,
                    proof_structure: ProofStructureInfo::from_proof(&success.proof),
                }
            }
            Err(code) => ProbeFacadeResponse::ProbeAborted { code },
        }
    }

    /// Run the probe with explicit start sequence.
    ///
    /// This variant allows callers to specify the exact start sequence,
    /// which is useful when the follower's tip has been observed directly.
    pub async fn probe_with_start_sequence(
        &self,
        follower_identity: &str,
        start_sequence: u64,
    ) -> ProbeFacadeResponse {
        let result = self
            .inner
            .run_with_start_sequence(follower_identity, start_sequence)
            .await;

        match result {
            Ok(success) => {
                if let Err(proof_err) = verify_proof_structure(&success.proof) {
                    return ProbeFacadeResponse::ProbeAborted {
                        code: proof_err.to_abort_code(),
                    };
                }

                ProbeFacadeResponse::ProbeOk {
                    tip: success.tip,
                    proof_structure: ProofStructureInfo::from_proof(&success.proof),
                }
            }
            Err(code) => ProbeFacadeResponse::ProbeAborted { code },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{EntryHashInfo, FakeLeaderTransport, HashPath, LeaderVersion, Proof};

    fn make_tip(sequence: u64, hash: &str) -> LeaderTip {
        LeaderTip {
            sequence,
            hash: hash.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    fn make_version() -> LeaderVersion {
        LeaderVersion {
            version: "1.0.0".to_string(),
            min_follower_version: "1.0.0".to_string(),
        }
    }

    fn make_proof(sequences: Vec<u64>, hashes: Vec<&str>) -> Proof {
        let entries: Vec<EntryHashInfo> = sequences
            .into_iter()
            .zip(hashes.into_iter())
            .map(|(seq, hash)| EntryHashInfo {
                sequence: seq,
                entry_hash: hash.to_string(),
            })
            .collect();

        let range_hash = entries
            .iter()
            .map(|e| e.entry_hash.clone())
            .collect::<Vec<_>>()
            .join("");

        Proof {
            entries,
            range_hash,
            continuity_proof: HashPath {
                nodes: vec!["node1".to_string(), "node2".to_string()],
                leaf_count: 10,
            },
        }
    }

    #[tokio::test]
    async fn facade_returns_probe_ok_on_success() {
        let tip = make_tip(100, "abc123");
        let version = make_version();
        let proof = make_proof(vec![5, 6, 7], vec!["hash1", "hash2", "hash3"]);

        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip.clone()).await;
        transport.set_version(version.clone()).await;
        transport.set_proof(proof.clone()).await;

        let facade = ProbeFacade::new(transport);
        let request = ProbeFacadeRequest::new("follower-1", 4);

        let response = facade.probe(&request).await;

        assert!(response.is_ok());
        let ProbeFacadeResponse::ProbeOk {
            tip: resp_tip,
            proof_structure,
        } = response
        else {
            panic!("expected ProbeOk");
        };

        assert_eq!(resp_tip.sequence, 100);
        assert_eq!(resp_tip.hash, "abc123");
        assert_eq!(proof_structure.entry_count, 3);
        assert_eq!(proof_structure.range_hash, "hash1hash2hash3");
        assert_eq!(proof_structure.continuity_proof_shape.node_count, 2);
    }

    #[tokio::test]
    async fn facade_returns_probe_aborted_on_transport_error() {
        let transport = FakeLeaderTransport::new();
        transport
            .inject_tip_error(crate::transport::TransportError::LeaderUnreachable {
                address: "127.0.0.1:8080".to_string(),
            })
            .await;

        let facade = ProbeFacade::new(transport);
        let request = ProbeFacadeRequest::new("follower-1", 4);

        let response = facade.probe(&request).await;

        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A7));
    }

    #[tokio::test]
    async fn facade_returns_probe_aborted_on_proof_error() {
        let transport = FakeLeaderTransport::new();
        transport.set_tip(make_tip(100, "abc")).await;
        transport.set_version(make_version()).await;
        transport
            .inject_proof_error(crate::transport::TransportError::RangeNotAvailable {
                start: 5,
                end: 10,
            })
            .await;

        let facade = ProbeFacade::new(transport);
        let request = ProbeFacadeRequest::new("follower-1", 4);

        let response = facade.probe(&request).await;

        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A3));
    }

    #[tokio::test]
    async fn facade_hides_transport_dtos_from_callers() {
        // This test proves the facade contract: caller only interacts with
        // ProbeFacadeRequest and ProbeFacadeResponse. No transport DTOs leak.

        let tip = make_tip(50, "tiphash");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip).await;
        transport.set_version(make_version()).await;
        transport
            .set_proof(make_proof(vec![1], vec!["entry1"]))
            .await;

        let facade = ProbeFacade::new(transport);

        // Callers only know about ProbeFacadeRequest
        let request = ProbeFacadeRequest::new("test-follower", 0);

        // Callers only know about ProbeFacadeResponse
        let response = facade.probe(&request).await;

        // Verify the response is one of the two facade variants
        match response {
            ProbeFacadeResponse::ProbeOk { .. } => {}
            ProbeFacadeResponse::ProbeAborted { code: _ } => {}
        }

        // The code below would NOT compile if transport DTOs leaked:
        // let _ = response.leader_tip; // No such field on ProbeFacadeResponse
        // let _ = response.proof;      // No such field on ProbeFacadeResponse
    }

    #[tokio::test]
    async fn facade_preserves_abort_code_only_failures() {
        // Test that ALL failure types collapse to ProbeAborted with a code.
        // No distinction between error types at facade level.

        let transport = FakeLeaderTransport::new();

        // A7: LeaderUnreachable
        transport
            .inject_tip_error(crate::transport::TransportError::LeaderUnreachable {
                address: "127.0.0.1:8080".to_string(),
            })
            .await;

        let facade = ProbeFacade::new(transport);
        let response = facade.probe(&ProbeFacadeRequest::new("f1", 0)).await;
        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A7));

        // A8: LeaderCapabilityDenied
        let transport2 = FakeLeaderTransport::new();
        transport2
            .inject_tip_error(crate::transport::TransportError::LeaderCapabilityDenied {
                leader: "leader-1".to_string(),
                required_capability: "sync".to_string(),
            })
            .await;

        let facade2 = ProbeFacade::new(transport2);
        let response2 = facade2.probe(&ProbeFacadeRequest::new("f2", 0)).await;
        assert!(response2.is_aborted());
        assert_eq!(response2.abort_code(), Some(Sync1AbortCode::A8));

        // A7: LeaderVersionIncompatible
        let transport3 = FakeLeaderTransport::new();
        transport3
            .inject_tip_error(
                crate::transport::TransportError::LeaderVersionIncompatible {
                    leader_version: "2.0.0".to_string(),
                    follower_min_version: "1.0.0".to_string(),
                },
            )
            .await;

        let facade3 = ProbeFacade::new(transport3);
        let response3 = facade3.probe(&ProbeFacadeRequest::new("f3", 0)).await;
        assert!(response3.is_aborted());
        assert_eq!(response3.abort_code(), Some(Sync1AbortCode::A7));
    }

    #[tokio::test]
    async fn facade_with_custom_probe_count() {
        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        // Create facade with 5 probes
        let facade = ProbeFacade::with_probes(transport, 5);
        let request = ProbeFacadeRequest::new("follower", 0).with_probe_count(5);

        let response = facade.probe(&request).await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn facade_probe_count_is_honored_via_error_on_nth_call() {
        // Verify that request.probe_count actually affects how many tip fetches
        // are made. Inject an error on call 3 — if probe_count=3, we abort.
        // If probe_count is NOT honored (always uses default 3), same result.
        // We verify by checking that probe_count=2 SUCCEEDS (doesn't hit the error
        // on call 3) and probe_count=3 FAILS (hits the error on call 3).
        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        // Inject error on the 3rd tip fetch call
        transport
            .inject_tip_error_on_call(
                3,
                crate::transport::TransportError::LeaderUnreachable {
                    address: "127.0.0.1:8080".to_string(),
                },
            )
            .await;

        // Probe with probe_count=2 should succeed (only 2 tip fetches, doesn't hit call 3)
        let facade = ProbeFacade::new(transport.clone());
        let request_2 = ProbeFacadeRequest::new("follower", 0).with_probe_count(2);
        let response_2 = facade.probe(&request_2).await;
        assert!(
            response_2.is_ok(),
            "probe_count=2 should not hit error on call 3"
        );

        // Probe with probe_count=3 should abort (hits error on call 3)
        let facade3 = ProbeFacade::new(transport.clone());
        let request_3 = ProbeFacadeRequest::new("follower", 0).with_probe_count(3);
        let response_3 = facade3.probe(&request_3).await;
        assert!(
            response_3.is_aborted(),
            "probe_count=3 should hit error on call 3"
        );
        assert_eq!(response_3.abort_code(), Some(Sync1AbortCode::A7));
    }

    #[tokio::test]
    async fn facade_proof_structure_contains_shape_only() {
        let proof = make_proof(vec![10, 11, 12, 13], vec!["a", "b", "c", "d"]);
        let transport = FakeLeaderTransport::new();
        transport.set_tip(make_tip(13, "latesthash")).await;
        transport.set_version(make_version()).await;
        transport.set_proof(proof).await;

        let facade = ProbeFacade::new(transport);
        let request = ProbeFacadeRequest::new("follower", 9);

        let response = facade.probe(&request).await;

        let ProbeFacadeResponse::ProbeOk {
            proof_structure, ..
        } = response
        else {
            panic!("expected ProbeOk");
        };

        // Shape info is available
        assert_eq!(proof_structure.entry_count, 4);
        assert!(!proof_structure.range_hash.is_empty());
        assert_eq!(proof_structure.continuity_proof_shape.node_count, 2);

        // Full entry content is NOT available (caller cannot access entries)
        // This is enforced by ProofStructureInfo not having an `entries` field.
        // If we got here, the test passes - shape-only info escaped correctly.
    }
}
