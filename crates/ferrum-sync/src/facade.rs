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
    /// Number of consistency probes to perform (default 3, minimum 3).
    pub probe_count: usize,
    /// Per-probe timeout in milliseconds.
    pub timeout_per_probe_ms: u64,
}

/// Configuration for ProbeFacade defaults and policy.
///
/// This struct centralizes all magic numbers and policy defaults
/// that were previously scattered as magic constants in the codebase.
/// All validation thresholds are enforced here to ensure consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeFacadeConfig {
    /// Default probe count for consistency checking.
    pub default_probe_count: usize,
    /// Default per-probe timeout in milliseconds.
    pub default_timeout_ms: u64,
    /// Minimum allowed probe count per contract (always 3 for Sync-3a.1).
    pub min_probe_count: usize,
    /// Minimum sane timeout per probe (1ms).
    pub min_timeout_ms: u64,
    /// Maximum reasonable timeout per probe (30 seconds).
    pub max_timeout_ms: u64,
}

impl Default for ProbeFacadeConfig {
    fn default() -> Self {
        Self {
            default_probe_count: 3,
            default_timeout_ms: 5000,
            min_probe_count: 3,
            min_timeout_ms: 1,
            max_timeout_ms: 30_000,
        }
    }
}

impl ProbeFacadeConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config with a custom default probe count.
    ///
    /// The probe_count must be >= min_probe_count (3).
    /// Returns None if the provided count is invalid.
    pub fn with_default_probe_count(mut self, count: usize) -> Option<Self> {
        if count < self.min_probe_count {
            return None;
        }
        self.default_probe_count = count;
        Some(self)
    }

    /// Create a config with a custom default timeout.
    ///
    /// The timeout must be in range [min_timeout_ms, max_timeout_ms].
    /// Returns None if the provided timeout is invalid.
    pub fn with_default_timeout_ms(mut self, timeout_ms: u64) -> Option<Self> {
        if timeout_ms < self.min_timeout_ms || timeout_ms > self.max_timeout_ms {
            return None;
        }
        self.default_timeout_ms = timeout_ms;
        Some(self)
    }

    /// Validate a probe count against contract constraints.
    ///
    /// Returns true if valid (>= min_probe_count).
    pub fn is_valid_probe_count(&self, count: usize) -> bool {
        count >= self.min_probe_count
    }

    /// Validate a timeout against contract constraints.
    ///
    /// Returns true if valid (in range [min_timeout_ms, max_timeout_ms]).
    pub fn is_valid_timeout(&self, timeout_ms: u64) -> bool {
        timeout_ms >= self.min_timeout_ms && timeout_ms <= self.max_timeout_ms
    }

    /// Build a ProbeFacadeRequest with this config's defaults.
    ///
    /// This is the primary factory method for creating requests
    /// with centralized default values.
    pub fn new_request(
        &self,
        follower_identity: impl Into<String>,
        follower_tip_sequence: u64,
    ) -> ProbeFacadeRequest {
        ProbeFacadeRequest {
            follower_identity: follower_identity.into(),
            follower_tip_sequence,
            probe_count: self.default_probe_count,
            timeout_per_probe_ms: self.default_timeout_ms,
        }
    }
}

impl ProbeFacadeRequest {
    /// Create a new probe request with default settings from ProbeFacadeConfig.
    pub fn new(follower_identity: impl Into<String>, follower_tip_sequence: u64) -> Self {
        ProbeFacadeConfig::default().new_request(follower_identity, follower_tip_sequence)
    }

    /// Create a new probe request using explicit config values.
    ///
    /// This is the factory method that uses the centralized config.
    /// Prefer this over the individual with_* methods for bulk construction.
    pub fn from_config(
        config: &ProbeFacadeConfig,
        follower_identity: impl Into<String>,
        follower_tip_sequence: u64,
    ) -> Self {
        config.new_request(follower_identity, follower_tip_sequence)
    }

    /// Set the probe count (for custom consistency requirements).
    /// Note: probe_count must be >= 3 per facade contract; invalid values
    /// will cause the probe to fail with A0.
    pub fn with_probe_count(mut self, count: usize) -> Self {
        self.probe_count = count;
        self
    }

    /// Set the per-probe timeout in milliseconds.
    /// Note: timeout must be in range [1, 30000] per facade contract;
    /// invalid values will cause the probe to fail with A0.
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_per_probe_ms = timeout_ms;
        self
    }

    /// Validate this request's preconditions using default config bounds.
    ///
    /// Returns `Some(Sync1AbortCode::A0)` if invalid (fail-closed).
    /// Returns `None` if the request is valid.
    ///
    /// Uses `ProbeFacadeConfig::default()` for validation bounds to ensure
    /// centralized contract enforcement.
    #[allow(dead_code)]
    fn validate(&self) -> Option<Sync1AbortCode> {
        self.validate_with_config(&ProbeFacadeConfig::default())
    }

    /// Validate this request's preconditions using explicit config bounds.
    ///
    /// Returns `Some(Sync1AbortCode::A0)` if invalid (fail-closed).
    /// Returns `None` if the request is valid.
    ///
    /// Use this variant when you need to validate against a custom config.
    fn validate_with_config(&self, config: &ProbeFacadeConfig) -> Option<Sync1AbortCode> {
        if !config.is_valid_probe_count(self.probe_count) {
            return Some(Sync1AbortCode::A0);
        }

        if !config.is_valid_timeout(self.timeout_per_probe_ms) {
            return Some(Sync1AbortCode::A0);
        }

        None
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
    config: ProbeFacadeConfig,
}

impl<T: Transport> ProbeFacade<T> {
    /// Create a new probe facade wrapping the given transport.
    ///
    /// Uses `ProbeFacadeConfig::default()` for validation bounds.
    pub fn new(transport: T) -> Self {
        Self {
            inner: TransportProbe::new(transport),
            config: ProbeFacadeConfig::default(),
        }
    }

    /// Create a new probe facade with custom probe count.
    ///
    /// Uses `ProbeFacadeConfig::default()` for validation bounds.
    pub fn with_probes(transport: T, probe_count: usize) -> Self {
        Self {
            inner: TransportProbe::with_probes(transport, probe_count),
            config: ProbeFacadeConfig::default(),
        }
    }

    /// Create a new probe facade with explicit configuration.
    ///
    /// The config is stored and used for request validation bounds.
    pub fn with_config(transport: T, config: ProbeFacadeConfig) -> Self {
        Self {
            inner: TransportProbe::new(transport),
            config,
        }
    }

    /// Returns a reference to the facade's configuration.
    pub fn config(&self) -> &ProbeFacadeConfig {
        &self.config
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
        // Fail-closed validation of caller preconditions (A0 on invalid config)
        if let Some(code) = request.validate_with_config(&self.config) {
            return ProbeFacadeResponse::ProbeAborted { code };
        }

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

    /// Validate parameters for probe_with_start_sequence.
    ///
    /// Returns `Some(Sync1AbortCode::A0)` if invalid (fail-closed).
    /// Returns `None` if the parameters are valid.
    fn validate_probe_params(
        follower_identity: &str,
        _start_sequence: u64,
    ) -> Option<Sync1AbortCode> {
        // follower_identity must not be empty
        if follower_identity.is_empty() {
            return Some(Sync1AbortCode::A0);
        }

        None
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
        // Fail-closed validation of caller preconditions (A0 on invalid params)
        if let Some(code) = Self::validate_probe_params(follower_identity, start_sequence) {
            return ProbeFacadeResponse::ProbeAborted { code };
        }

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
        // are made. Inject an error on call 4 — if probe_count=4, we abort.
        // If probe_count is NOT honored (always uses default 3), same result.
        // We verify by checking that probe_count=3 SUCCEEDS (doesn't hit the error
        // on call 4) and probe_count=4 FAILS (hits the error on call 4).
        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        // Inject error on the 4th tip fetch call
        transport
            .inject_tip_error_on_call(
                4,
                crate::transport::TransportError::LeaderUnreachable {
                    address: "127.0.0.1:8080".to_string(),
                },
            )
            .await;

        // Probe with probe_count=3 should succeed (only 3 tip fetches, doesn't hit call 4)
        let facade = ProbeFacade::new(transport.clone());
        let request_3 = ProbeFacadeRequest::new("follower", 0).with_probe_count(3);
        let response_3 = facade.probe(&request_3).await;
        assert!(
            response_3.is_ok(),
            "probe_count=3 should not hit error on call 4"
        );

        // Probe with probe_count=4 should abort (hits error on call 4)
        let facade4 = ProbeFacade::new(transport.clone());
        let request_4 = ProbeFacadeRequest::new("follower", 0).with_probe_count(4);
        let response_4 = facade4.probe(&request_4).await;
        assert!(
            response_4.is_aborted(),
            "probe_count=4 should hit error on call 4"
        );
        assert_eq!(response_4.abort_code(), Some(Sync1AbortCode::A7));
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

    #[tokio::test]
    async fn facade_rejects_probe_count_below_minimum() {
        // probe_count < 3 is invalid per facade contract; must fail with A0
        let transport = FakeLeaderTransport::new();
        let facade = ProbeFacade::new(transport);

        // probe_count = 2 should fail
        let request = ProbeFacadeRequest::new("follower", 0).with_probe_count(2);
        let response = facade.probe(&request).await;
        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));

        // probe_count = 1 should fail
        let request_1 = ProbeFacadeRequest::new("follower", 0).with_probe_count(1);
        let response_1 = facade.probe(&request_1).await;
        assert!(response_1.is_aborted());
        assert_eq!(response_1.abort_code(), Some(Sync1AbortCode::A0));

        // probe_count = 0 should fail
        let request_0 = ProbeFacadeRequest::new("follower", 0).with_probe_count(0);
        let response_0 = facade.probe(&request_0).await;
        assert!(response_0.is_aborted());
        assert_eq!(response_0.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_rejects_timeout_too_low() {
        // timeout < 1ms is invalid per facade contract; must fail with A0
        let transport = FakeLeaderTransport::new();
        let facade = ProbeFacade::new(transport);

        let request = ProbeFacadeRequest::new("follower", 0).with_timeout_ms(0);
        let response = facade.probe(&request).await;
        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_rejects_timeout_too_high() {
        // timeout > 30000ms is invalid per facade contract; must fail with A0
        let transport = FakeLeaderTransport::new();
        let facade = ProbeFacade::new(transport);

        let request = ProbeFacadeRequest::new("follower", 0).with_timeout_ms(60_000);
        let response = facade.probe(&request).await;
        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_accepts_valid_probe_count_at_minimum() {
        // probe_count = 3 is the minimum valid value
        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        let facade = ProbeFacade::new(transport);
        let request = ProbeFacadeRequest::new("follower", 0).with_probe_count(3);
        let response = facade.probe(&request).await;
        // Should succeed (not A0), actual result depends on transport
        // but A0 means validation failure, not transport failure
        assert_ne!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_accepts_valid_timeout_boundaries() {
        let tip = make_tip(100, "abc");

        // timeout = 1ms (minimum valid)
        let transport1 = FakeLeaderTransport::new();
        transport1.set_tip(tip.clone()).await;
        transport1.set_version(make_version()).await;
        transport1.set_proof(make_proof(vec![1], vec!["h1"])).await;
        let facade1 = ProbeFacade::new(transport1);
        let request_min = ProbeFacadeRequest::new("follower", 0).with_timeout_ms(1);
        let response_min = facade1.probe(&request_min).await;
        assert_ne!(response_min.abort_code(), Some(Sync1AbortCode::A0));

        // timeout = 30000ms (maximum valid)
        let transport2 = FakeLeaderTransport::new();
        transport2.set_tip(tip).await;
        transport2.set_version(make_version()).await;
        transport2.set_proof(make_proof(vec![1], vec!["h1"])).await;
        let facade2 = ProbeFacade::new(transport2);
        let request_max = ProbeFacadeRequest::new("follower", 0).with_timeout_ms(30_000);
        let response_max = facade2.probe(&request_max).await;
        assert_ne!(response_max.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_fails_closed_on_invalid_config_a0() {
        // Comprehensive test: any invalid config returns A0, never A7 or other codes
        // This proves fail-closed behavior for config/precondition failures
        let transport = FakeLeaderTransport::new();
        let facade = ProbeFacade::new(transport);

        // Invalid: probe_count = 0
        let response = facade
            .probe(&ProbeFacadeRequest::new("f", 0).with_probe_count(0))
            .await;
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));

        // Invalid: probe_count = 1
        let response = facade
            .probe(&ProbeFacadeRequest::new("f", 0).with_probe_count(1))
            .await;
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));

        // Invalid: probe_count = 2
        let response = facade
            .probe(&ProbeFacadeRequest::new("f", 0).with_probe_count(2))
            .await;
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));

        // Invalid: timeout = 0
        let response = facade
            .probe(&ProbeFacadeRequest::new("f", 0).with_timeout_ms(0))
            .await;
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));

        // Invalid: timeout > 30000
        let response = facade
            .probe(&ProbeFacadeRequest::new("f", 0).with_timeout_ms(u64::MAX))
            .await;
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_probe_with_start_sequence_rejects_empty_identity() {
        // Empty follower_identity must fail with A0 (fail-closed)
        let transport = FakeLeaderTransport::new();
        let facade = ProbeFacade::new(transport);

        let response = facade.probe_with_start_sequence("", 0).await;
        assert!(response.is_aborted());
        assert_eq!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[tokio::test]
    async fn facade_probe_with_start_sequence_accepts_valid_identity() {
        // Non-empty follower_identity should be accepted and not return A0
        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        let facade = ProbeFacade::new(transport);
        let response = facade.probe_with_start_sequence("valid-follower", 0).await;
        // A0 means validation failure - we should NOT get A0 for valid params
        assert_ne!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    // =============================================================================
    // ProbeFacadeConfig tests
    // =============================================================================

    #[test]
    fn config_default_values() {
        let config = ProbeFacadeConfig::default();

        assert_eq!(config.default_probe_count, 3);
        assert_eq!(config.default_timeout_ms, 5000);
        assert_eq!(config.min_probe_count, 3);
        assert_eq!(config.min_timeout_ms, 1);
        assert_eq!(config.max_timeout_ms, 30_000);
    }

    #[test]
    fn config_new_is_same_as_default() {
        let config = ProbeFacadeConfig::new();
        assert_eq!(config, ProbeFacadeConfig::default());
    }

    #[test]
    fn config_is_valid_probe_count_accepts_valid_values() {
        let config = ProbeFacadeConfig::default();

        assert!(config.is_valid_probe_count(3)); // minimum
        assert!(config.is_valid_probe_count(4));
        assert!(config.is_valid_probe_count(100));
    }

    #[test]
    fn config_is_valid_probe_count_rejects_invalid_values() {
        let config = ProbeFacadeConfig::default();

        assert!(!config.is_valid_probe_count(0));
        assert!(!config.is_valid_probe_count(1));
        assert!(!config.is_valid_probe_count(2));
    }

    #[test]
    fn config_is_valid_timeout_accepts_valid_values() {
        let config = ProbeFacadeConfig::default();

        assert!(config.is_valid_timeout(1)); // minimum
        assert!(config.is_valid_timeout(5000)); // default
        assert!(config.is_valid_timeout(30_000)); // maximum
    }

    #[test]
    fn config_is_valid_timeout_rejects_invalid_values() {
        let config = ProbeFacadeConfig::default();

        assert!(!config.is_valid_timeout(0)); // below minimum
        assert!(!config.is_valid_timeout(u64::MAX)); // above maximum
    }

    #[test]
    fn config_with_default_probe_count_accepts_valid_values() {
        let config = ProbeFacadeConfig::default();

        let config_with_5 = config.with_default_probe_count(5);
        assert!(config_with_5.is_some());
        assert_eq!(config_with_5.unwrap().default_probe_count, 5);
    }

    #[test]
    fn config_with_default_probe_count_rejects_invalid_values() {
        let config = ProbeFacadeConfig::default();

        assert!(config.with_default_probe_count(0).is_none());
        assert!(config.with_default_probe_count(1).is_none());
        assert!(config.with_default_probe_count(2).is_none());
    }

    #[test]
    fn config_with_default_timeout_ms_accepts_valid_values() {
        let config = ProbeFacadeConfig::default();

        let config_with_10s = config.with_default_timeout_ms(10_000);
        assert!(config_with_10s.is_some());
        assert_eq!(config_with_10s.unwrap().default_timeout_ms, 10_000);
    }

    #[test]
    fn config_with_default_timeout_ms_rejects_invalid_values() {
        let config = ProbeFacadeConfig::default();

        assert!(config.with_default_timeout_ms(0).is_none()); // below min
        assert!(config.with_default_timeout_ms(u64::MAX).is_none()); // above max
    }

    #[test]
    fn config_new_request_uses_config_defaults() {
        let config = ProbeFacadeConfig::default();
        let request = config.new_request("node-1", 100);

        assert_eq!(request.follower_identity, "node-1");
        assert_eq!(request.follower_tip_sequence, 100);
        assert_eq!(request.probe_count, config.default_probe_count);
        assert_eq!(request.timeout_per_probe_ms, config.default_timeout_ms);
    }

    #[test]
    fn config_new_request_with_custom_config() {
        let config = ProbeFacadeConfig::default()
            .with_default_probe_count(5)
            .unwrap();
        let request = config.new_request("node-2", 200);

        assert_eq!(request.follower_identity, "node-2");
        assert_eq!(request.follower_tip_sequence, 200);
        assert_eq!(request.probe_count, 5);
        assert_eq!(request.timeout_per_probe_ms, config.default_timeout_ms);
    }

    #[test]
    fn probe_facade_request_from_config_matches_new_request() {
        let config = ProbeFacadeConfig::default();
        let request1 = ProbeFacadeRequest::new("test-node", 50);
        let request2 = ProbeFacadeRequest::from_config(&config, "test-node", 50);

        assert_eq!(request1.follower_identity, request2.follower_identity);
        assert_eq!(
            request1.follower_tip_sequence,
            request2.follower_tip_sequence
        );
        assert_eq!(request1.probe_count, request2.probe_count);
        assert_eq!(request1.timeout_per_probe_ms, request2.timeout_per_probe_ms);
    }

    #[test]
    fn probe_facade_request_validate_uses_default_config_bounds() {
        // Valid request should pass validation
        let valid_request = ProbeFacadeRequest::new("node", 0);
        assert!(valid_request.validate().is_none());

        // Invalid probe_count (below min) should fail
        let invalid_count_request = ProbeFacadeRequest::new("node", 0).with_probe_count(2);
        assert_eq!(invalid_count_request.validate(), Some(Sync1AbortCode::A0));

        // Invalid timeout (above max) should fail
        let invalid_timeout_request = ProbeFacadeRequest::new("node", 0).with_timeout_ms(60_000);
        assert_eq!(invalid_timeout_request.validate(), Some(Sync1AbortCode::A0));
    }

    #[test]
    fn probe_facade_request_validate_with_custom_config() {
        // Test that validate_with_config uses the config's bounds correctly
        // Default config has min_probe_count=3, so probe_count=2 should fail
        let default_config = ProbeFacadeConfig::default();

        let request_below_min = ProbeFacadeRequest::new("node", 0).with_probe_count(2);
        assert_eq!(
            request_below_min.validate_with_config(&default_config),
            Some(Sync1AbortCode::A0)
        );

        // But probe_count=3 (equal to min) should pass
        let request_at_min = ProbeFacadeRequest::new("node", 0).with_probe_count(3);
        assert!(
            request_at_min
                .validate_with_config(&default_config)
                .is_none()
        );

        // Custom default doesn't change validation bounds (min_probe_count stays 3)
        let custom_default_config = ProbeFacadeConfig::default()
            .with_default_probe_count(10)
            .unwrap();
        let request_with_custom_default = ProbeFacadeRequest::new("node", 0).with_probe_count(5);
        // probe_count=5 >= min_probe_count=3, so validation passes
        assert!(
            request_with_custom_default
                .validate_with_config(&custom_default_config)
                .is_none()
        );

        // Request with custom timeout that exceeds default max should fail validation
        let request_high_timeout = ProbeFacadeRequest::new("node", 0).with_timeout_ms(100_000);
        assert_eq!(
            request_high_timeout.validate_with_config(&default_config),
            Some(Sync1AbortCode::A0)
        );
    }

    #[tokio::test]
    async fn facade_request_created_via_config_produces_valid_response() {
        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip.clone()).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        let facade = ProbeFacade::new(transport);
        let config = ProbeFacadeConfig::default();
        let request = config.new_request("follower-config-test", 0);

        let response = facade.probe(&request).await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn facade_with_config_stores_and_uses_config() {
        // Verify that with_config stores the config and uses it for validation
        let custom_config = ProbeFacadeConfig::default()
            .with_default_probe_count(5)
            .unwrap();

        let tip = make_tip(100, "abc");
        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip.clone()).await;
        transport.set_version(make_version()).await;
        transport.set_proof(make_proof(vec![1], vec!["h1"])).await;

        let facade = ProbeFacade::with_config(transport, custom_config);

        // Verify config() returns the stored config
        assert_eq!(facade.config().default_probe_count, 5);
        assert_eq!(facade.config().default_timeout_ms, 5000);

        // Verify the facade uses the stored config (probe_count=5 is valid with stored config)
        let request = ProbeFacadeRequest::new("follower", 0).with_probe_count(5);
        let response = facade.probe(&request).await;
        // Should NOT be A0 (validation error) since probe_count=5 >= min_probe_count=3
        assert_ne!(response.abort_code(), Some(Sync1AbortCode::A0));
    }

    #[test]
    fn config_chain_validators() {
        // Verify that with_default_probe_count chains correctly
        let config = ProbeFacadeConfig::default()
            .with_default_probe_count(7)
            .unwrap()
            .with_default_timeout_ms(15_000)
            .unwrap();

        assert_eq!(config.default_probe_count, 7);
        assert_eq!(config.default_timeout_ms, 15_000);
    }

    #[test]
    fn config_equality() {
        let config1 = ProbeFacadeConfig::default();
        let config2 = ProbeFacadeConfig::default();
        assert_eq!(config1, config2);

        let custom_config = config1.with_default_probe_count(5).unwrap();
        assert_ne!(config1, custom_config);
    }
}
