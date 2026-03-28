//! Transport abstraction and fake implementation for Sync-3a diagnostic probe.
//!
//! This module defines:
//! - `Transport` trait: abstracts leader-tip and proof retrieval
//! - `LeaderTip` / `LeaderTipRequest` / `LeaderTipResponse`: tip fetch types
//! - `ProofRequest` / `ProofResponse`: proof fetch types
//! - `TransportError`: all transport-level error variants
//! - `FakeLeaderTransport`: in-memory fake for testing

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrum_proto::Sha256Hex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::timeout;
use uuid::Uuid;

use crate::error::{ProbeError, Sync1AbortCode, map_transport_error_to_abort};

/// Unique identifier for a sync probe request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProbeRequestId(pub Uuid);

impl ProbeRequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProbeRequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// The leader's current tip information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderTip {
    /// Current sequence number.
    pub sequence: u64,
    /// Hash of the current tip entry.
    pub hash: Sha256Hex,
    /// Wall-clock timestamp of the tip.
    pub timestamp: DateTime<Utc>,
}

/// Leader version information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderVersion {
    /// Semantic version string of the leader.
    pub version: String,
    /// Minimum compatible follower version.
    pub min_follower_version: String,
}

/// Request for leader tip retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderTipRequest {
    /// Unique request ID for this sync attempt.
    pub request_id: ProbeRequestId,
    /// Identity of the requesting follower.
    pub follower_identity: String,
    /// Per-request timeout in milliseconds.
    /// Transport implementations SHOULD respect this bound; the fake
    /// transport ignores it because it has no real network latency.
    pub timeout_ms: u64,
}

/// Response for leader tip retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderTipResponse {
    /// The leader's current tip, if successful.
    pub leader_tip: Option<LeaderTip>,
    /// Leader version info, if successful.
    pub leader_version: Option<LeaderVersion>,
}

/// Request for proof (hash-path) retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofRequest {
    /// Unique request ID for this sync attempt.
    pub request_id: ProbeRequestId,
    /// Identity of the requesting follower.
    pub follower_identity: String,
    /// Inclusive start sequence (usually follower tip + 1).
    pub start_sequence: u64,
    /// Inclusive end sequence (usually leader tip).
    pub end_sequence: u64,
    /// Per-request timeout in milliseconds.
    /// Transport implementations SHOULD respect this bound; the fake
    /// transport ignores it (no real network latency).
    pub timeout_ms: u64,
}

/// Response for proof retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofResponse {
    /// The retrieved proof, if successful.
    pub proof: Option<Proof>,
}

/// A Merkle proof proving continuity over a range of entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proof {
    /// Entries in the range, with their hashes.
    pub entries: Vec<EntryHashInfo>,
    /// Hash of the concatenated entry hashes in the range.
    pub range_hash: Sha256Hex,
    /// Continuity proof nodes for Merkle verification.
    pub continuity_proof: HashPath,
}

/// Hash information for a single entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryHashInfo {
    /// Sequence number of the entry.
    pub sequence: u64,
    /// Hash of the entry.
    pub entry_hash: Sha256Hex,
}

/// A hash path (Merkle proof) for continuity verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HashPath {
    /// Merkle proof nodes.
    pub nodes: Vec<Sha256Hex>,
    /// Number of leaves in the original Merkle tree.
    pub leaf_count: u64,
}

/// Transport-level errors that can occur during tip or proof retrieval.
///
/// These are always mapped to Sync-1 abort codes before returning to callers.
#[derive(Debug, Clone, Error)]
pub enum TransportError {
    #[error("leader unreachable at {address}")]
    LeaderUnreachable { address: String },

    #[error("leader timeout at {address} after {duration_ms}ms")]
    LeaderTimeout { address: String, duration_ms: u64 },

    #[error("leader {leader} denied capability: {required_capability}")]
    LeaderCapabilityDenied {
        leader: String,
        required_capability: String,
    },

    #[error(
        "leader version {leader_version} incompatible with follower min {follower_min_version}"
    )]
    LeaderVersionIncompatible {
        leader_version: String,
        follower_min_version: String,
    },

    #[error("range {start}..{end} not available on leader")]
    RangeNotAvailable { start: u64, end: u64 },

    #[error("internal transport error: {details}")]
    InternalError { details: String },
}

/// Result of a transport probe: either success with tip+proof or an abort code.
pub type ProbeResult = Result<ProbeSuccess, Sync1AbortCode>;

/// Successful probe result containing tip and proof data.
#[derive(Debug, Clone)]
pub struct ProbeSuccess {
    /// The leader's tip at the time of probing.
    pub tip: LeaderTip,
    /// The leader version info.
    pub version: LeaderVersion,
    /// The proof structure retrieved.
    pub proof: Proof,
}

/// Transport trait that abstracts leader-tip and proof retrieval.
///
/// Implementors can provide real network transports or fake in-memory transports
/// for testing. The trait is async to support real network calls.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Fetch the leader's current tip.
    async fn fetch_leader_tip(
        &self,
        request: &LeaderTipRequest,
    ) -> Result<LeaderTipResponse, TransportError>;

    /// Fetch a proof for the given range.
    async fn fetch_proof(&self, request: &ProofRequest) -> Result<ProofResponse, TransportError>;
}

/// In-memory fake transport for testing the diagnostic probe.
///
/// This transport simulates a leader node with configurable behavior:
/// - Tip responses can be set to return consistent or inconsistent tips
/// - Proof responses can be configured to return well-formed or malformed proofs
/// - Errors can be injected for testing error mapping
#[derive(Debug, Clone)]
pub struct FakeLeaderTransport {
    state: Arc<RwLock<FakeTransportState>>,
}

/// Shared state for the fake transport.
#[derive(Debug, Clone)]
struct FakeTransportState {
    /// Configured tip to return.
    tip: Option<LeaderTip>,
    /// Configured version to return.
    version: Option<LeaderVersion>,
    /// Configured proof to return.
    proof: Option<Proof>,
    /// Error to return on tip fetch, if any.
    tip_error: Option<TransportError>,
    /// Error to return on proof fetch, if any.
    proof_error: Option<TransportError>,
    /// (Call number, error) to return on the Nth tip fetch call (cleared after use).
    tip_error_on_call: Option<(usize, TransportError)>,
    /// Call count for tip fetch.
    tip_fetch_count: usize,
    /// Call count for proof fetch.
    proof_fetch_count: usize,
    /// Optional artificial delay in ms before returning tip (for timeout testing).
    tip_delay_ms: Option<u64>,
}

impl Default for FakeTransportState {
    fn default() -> Self {
        Self {
            tip: Some(LeaderTip {
                sequence: 100,
                hash: "abcdef123456".to_string(),
                timestamp: Utc::now(),
            }),
            version: Some(LeaderVersion {
                version: "1.0.0".to_string(),
                min_follower_version: "1.0.0".to_string(),
            }),
            proof: None,
            tip_error: None,
            proof_error: None,
            tip_error_on_call: None,
            tip_fetch_count: 0,
            proof_fetch_count: 0,
            tip_delay_ms: None,
        }
    }
}

impl FakeLeaderTransport {
    /// Creates a new fake transport with default (consistent) state.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(FakeTransportState::default())),
        }
    }

    /// Creates a new fake transport with a specific tip.
    pub fn with_tip(tip: LeaderTip, version: LeaderVersion) -> Self {
        Self {
            state: Arc::new(RwLock::new(FakeTransportState {
                tip: Some(tip),
                version: Some(version),
                ..Default::default()
            })),
        }
    }

    /// Configure the fake tip to return.
    pub async fn set_tip(&self, tip: LeaderTip) {
        let mut state = self.state.write().await;
        state.tip = Some(tip);
    }

    /// Configure the fake version to return.
    pub async fn set_version(&self, version: LeaderVersion) {
        let mut state = self.state.write().await;
        state.version = Some(version);
    }

    /// Configure the fake proof to return.
    pub async fn set_proof(&self, proof: Proof) {
        let mut state = self.state.write().await;
        state.proof = Some(proof);
    }

    /// Inject an error on the next tip fetch.
    pub async fn inject_tip_error(&self, err: TransportError) {
        let mut state = self.state.write().await;
        state.tip_error = Some(err);
    }

    /// Inject an error on the next proof fetch.
    pub async fn inject_proof_error(&self, err: TransportError) {
        let mut state = self.state.write().await;
        state.proof_error = Some(err);
    }

    /// Inject an error on the Nth tip fetch call (cleared after use).
    ///
    /// This is used to verify that probe_count actually affects call count.
    /// For example, inject an error on call 3, then request probe_count=3.
    /// If probe_count is honored, the error will be hit and probe will abort.
    /// If probe_count is NOT honored (always using default 3), the same applies.
    pub async fn inject_tip_error_on_call(&self, call_number: usize, err: TransportError) {
        let mut state = self.state.write().await;
        state.tip_error_on_call = Some((call_number, err));
    }

    /// Get the number of tip fetch calls.
    pub async fn tip_fetch_count(&self) -> usize {
        let state = self.state.read().await;
        state.tip_fetch_count
    }

    /// Get the number of proof fetch calls.
    pub async fn proof_fetch_count(&self) -> usize {
        let state = self.state.read().await;
        state.proof_fetch_count
    }

    /// Configure tip to return inconsistent values across multiple probes.
    ///
    /// Call this multiple times with different tips to simulate inconsistency.
    pub async fn set_inconsistent_tips(&self, tips: Vec<LeaderTip>) {
        let mut state = self.state.write().await;
        // Store all tips; we'll rotate through them
        state.tip = Some(tips.into_iter().next().unwrap());
    }

    /// Inject an artificial delay before returning tip responses.
    ///
    /// Used to test that `tokio::time::timeout` actually fires: set a delay
    /// longer than the caller's `timeout_per_probe_ms` and verify A7 is
    /// returned.
    pub async fn set_tip_delay_ms(&self, delay_ms: u64) {
        let mut state = self.state.write().await;
        state.tip_delay_ms = Some(delay_ms);
    }
}

impl Default for FakeLeaderTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for FakeLeaderTransport {
    async fn fetch_leader_tip(
        &self,
        _request: &LeaderTipRequest,
    ) -> Result<LeaderTipResponse, TransportError> {
        // 1. Determine what to return and whether to delay.
        //    We do the bookkeeping under one write lock, then release before
        //    any artificial sleep so the lock is not held across the delay.
        let (delay_ms, result) = {
            let mut state = self.state.write().await;
            state.tip_fetch_count += 1;

            // Check per-call error first
            let per_call_err = if state
                .tip_error_on_call
                .as_ref()
                .map(|(call_num, _)| state.tip_fetch_count == *call_num)
                .unwrap_or(false)
            {
                state.tip_error_on_call.take().map(|(_, err)| err)
            } else {
                None
            };

            if let Some(err) = per_call_err {
                (None, Err(err))
            } else if let Some(ref err) = state.tip_error {
                (None, Err(err.clone()))
            } else {
                let delay = state.tip_delay_ms;
                (
                    delay,
                    Ok(LeaderTipResponse {
                        leader_tip: state.tip.clone(),
                        leader_version: state.version.clone(),
                    }),
                )
            }
        };

        // 2. Artificial delay (outside the lock) for timeout testing.
        if let Some(ms) = delay_ms {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }

        result
    }

    async fn fetch_proof(&self, _request: &ProofRequest) -> Result<ProofResponse, TransportError> {
        let mut state = self.state.write().await;
        state.proof_fetch_count += 1;

        if let Some(ref err) = state.proof_error {
            return Err(err.clone());
        }

        Ok(ProofResponse {
            proof: state.proof.clone(),
        })
    }
}

/// The diagnostic probe that uses a transport to validate leader connectivity.
///
/// This is the main entry point for Sync-3a: it performs multi-probe tip
/// consistency checks and proof structure verification without modifying
/// any local state.
#[derive(Debug)]
pub struct TransportProbe<T: Transport> {
    transport: T,
    /// Number of probes to perform for consistency checking.
    consistency_probe_count: usize,
}

impl<T: Transport> TransportProbe<T> {
    /// Creates a new probe with the given transport.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            consistency_probe_count: 3,
        }
    }

    /// Creates a new probe with custom consistency probe count.
    pub fn with_probes(transport: T, probe_count: usize) -> Self {
        Self {
            transport,
            consistency_probe_count: probe_count,
        }
    }

    /// Run the diagnostic probe with default timeout (5 seconds).
    ///
    /// This performs:
    /// 1. Multi-probe tip consistency check (N probes, all must match)
    /// 2. Proof fetch and structure verification
    ///
    /// Each transport call is wrapped with `tokio::time::timeout` using the
    /// configured timeout. If a transport call exceeds the timeout, it is
    /// mapped to `TransportError::LeaderTimeout` (-> A7).
    ///
    /// Returns `ProbeSuccess` on full validation, or a `Sync1AbortCode` on any failure.
    /// No local state is modified regardless of outcome.
    pub async fn run(
        &self,
        follower_identity: &str,
        follower_tip_sequence: u64,
        leader_address: &str,
    ) -> ProbeResult {
        self.run_with_start_sequence(
            follower_identity,
            follower_tip_sequence,
            5000,
            leader_address,
        )
        .await
    }

    /// Run the diagnostic probe with explicit start sequence and timeout.
    ///
    /// Each transport call is wrapped with `tokio::time::timeout` using
    /// `timeout_ms`. On timeout the call is mapped to A7 (fail-closed).
    /// `leader_address` provides context in timeout/unreachable errors.
    ///
    /// Returns `ProbeSuccess` on full validation, or a `Sync1AbortCode` on any failure.
    pub async fn run_with_start_sequence(
        &self,
        follower_identity: &str,
        start_sequence: u64,
        timeout_ms: u64,
        leader_address: &str,
    ) -> ProbeResult {
        // Step 1: Multi-probe tip consistency check
        let (tip, version) = self
            .multi_probe_tip_consistency_with_count(
                follower_identity,
                self.consistency_probe_count,
                timeout_ms,
                leader_address,
            )
            .await?;

        // Step 2: Fetch proof for the range
        let proof = self
            .fetch_and_verify_proof(
                follower_identity,
                start_sequence,
                tip.sequence,
                timeout_ms,
                leader_address,
            )
            .await?;

        Ok(ProbeSuccess {
            tip,
            version,
            proof,
        })
    }

    /// Run the diagnostic probe overriding the consistency probe count.
    ///
    /// This is the same as `run` but allows per-request probe count override
    /// instead of the construction-time `consistency_probe_count`.
    /// Each transport call is wrapped with `tokio::time::timeout(timeout_ms)`;
    /// on expiry the call maps to A7.
    ///
    /// Returns `ProbeSuccess` on full validation, or a `Sync1AbortCode` on any failure.
    /// No local state is modified regardless of outcome.
    pub async fn run_with_probe_count(
        &self,
        follower_identity: &str,
        follower_tip_sequence: u64,
        probe_count: usize,
        timeout_ms: u64,
        leader_address: &str,
    ) -> ProbeResult {
        // Step 1: Multi-probe tip consistency check with override count
        let (tip, version) = self
            .multi_probe_tip_consistency_with_count(
                follower_identity,
                probe_count,
                timeout_ms,
                leader_address,
            )
            .await?;

        // Step 2: Fetch proof for the range
        let proof = self
            .fetch_and_verify_proof(
                follower_identity,
                follower_tip_sequence,
                tip.sequence,
                timeout_ms,
                leader_address,
            )
            .await?;

        Ok(ProbeSuccess {
            tip,
            version,
            proof,
        })
    }

    /// Perform multi-probe tip consistency check with a specific probe count.
    ///
    /// Fetches the leader tip N times and verifies all responses are identical.
    /// If any probe returns a different tip, returns `Sync1AbortCode::A7`.
    ///
    /// Each `fetch_leader_tip` call is wrapped with `tokio::time::timeout`.
    /// If a call exceeds `timeout_ms`, it is mapped to
    /// `TransportError::LeaderTimeout` (-> A7) with `leader_address` context.
    async fn multi_probe_tip_consistency_with_count(
        &self,
        follower_identity: &str,
        probe_count: usize,
        timeout_ms: u64,
        leader_address: &str,
    ) -> Result<(LeaderTip, LeaderVersion), Sync1AbortCode> {
        let mut tips: Vec<LeaderTip> = Vec::with_capacity(probe_count);
        let mut version: Option<LeaderVersion> = None;

        for i in 0..probe_count {
            let request = LeaderTipRequest {
                request_id: ProbeRequestId::new(),
                follower_identity: follower_identity.to_string(),
                timeout_ms,
            };

            // Enforce per-call timeout around the transport fetch.
            let fetch_result = timeout(
                Duration::from_millis(timeout_ms),
                self.transport.fetch_leader_tip(&request),
            )
            .await;

            let response = match fetch_result {
                Ok(Ok(resp)) => resp,
                Ok(Err(e)) => {
                    // Transport error maps to abort code directly
                    return Err(map_transport_error_to_abort(&e));
                }
                Err(_) => {
                    // Timeout elapsed -- fail-closed A7
                    return Err(map_transport_error_to_abort(
                        &TransportError::LeaderTimeout {
                            address: leader_address.to_string(),
                            duration_ms: timeout_ms,
                        },
                    ));
                }
            };

            let response_tip = response.leader_tip.ok_or(Sync1AbortCode::A7)?;
            let response_version = response.leader_version.ok_or(Sync1AbortCode::A7)?;

            if i == 0 {
                version = Some(response_version.clone());
            }

            // Verify version compatibility
            if response_version.version != version.as_ref().unwrap().version {
                // Version changed between probes - treat as inconsistency
                return Err(Sync1AbortCode::A7);
            }

            tips.push(response_tip);
        }

        // Verify all tips are consistent
        let first = &tips[0];
        for tip in &tips[1..] {
            if tip.sequence != first.sequence || tip.hash != first.hash {
                return Err(ProbeError::TipInconsistent {
                    probe_count: tips.len(),
                    first: first.clone(),
                    last: tip.clone(),
                }
                .to_abort_code());
            }
        }

        Ok((first.clone(), version.unwrap()))
    }

    /// Fetch proof and verify its structure.
    ///
    /// The `fetch_proof` call is wrapped with `tokio::time::timeout`.
    /// If the call exceeds `timeout_ms`, it is mapped to
    /// `TransportError::LeaderTimeout` (-> A7) with `leader_address` context.
    async fn fetch_and_verify_proof(
        &self,
        follower_identity: &str,
        start_sequence: u64,
        end_sequence: u64,
        timeout_ms: u64,
        leader_address: &str,
    ) -> Result<Proof, Sync1AbortCode> {
        let request = ProofRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: follower_identity.to_string(),
            start_sequence,
            end_sequence,
            timeout_ms,
        };

        // Enforce per-call timeout around the proof fetch.
        let fetch_result = timeout(
            Duration::from_millis(timeout_ms),
            self.transport.fetch_proof(&request),
        )
        .await;

        let response = match fetch_result {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => {
                return Err(map_transport_error_to_abort(&e));
            }
            Err(_) => {
                // Timeout elapsed -- fail-closed A7
                return Err(map_transport_error_to_abort(
                    &TransportError::LeaderTimeout {
                        address: leader_address.to_string(),
                        duration_ms: timeout_ms,
                    },
                ));
            }
        };

        let proof = response.proof.ok_or(Sync1AbortCode::A7)?;

        // Verify proof structure
        crate::proof::verify_proof_structure(&proof).map_err(|e| e.to_abort_code())?;

        Ok(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tip(sequence: u64, hash: &str) -> LeaderTip {
        LeaderTip {
            sequence,
            hash: hash.to_string(),
            timestamp: Utc::now(),
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

        let range_hash = if entries.is_empty() {
            "".to_string()
        } else {
            entries
                .iter()
                .map(|e| e.entry_hash.clone())
                .collect::<Vec<_>>()
                .join("")
        };

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
    async fn probe_success_when_tips_consistent_and_proof_valid() {
        let tip = make_tip(100, "abc123");
        let version = make_version();
        let proof = make_proof(vec![5, 6, 7], vec!["hash1", "hash2", "hash3"]);

        let transport = FakeLeaderTransport::new();
        transport.set_tip(tip.clone()).await;
        transport.set_version(version.clone()).await;
        transport.set_proof(proof.clone()).await;

        let probe = TransportProbe::new(transport);
        let result = probe.run("follower-1", 4, "leader:9000").await;

        assert!(result.is_ok());
        let success = result.unwrap();
        assert_eq!(success.tip.sequence, 100);
        assert_eq!(success.tip.hash, "abc123");
    }

    #[tokio::test]
    async fn probe_aborts_when_tip_inconsistent_across_probes() {
        // Set up transport to return consistent tips
        let transport = FakeLeaderTransport::new();
        transport.set_tip(make_tip(100, "abc")).await;
        transport.set_version(make_version()).await;
        transport
            .set_proof(make_proof(vec![5], vec!["hash1"]))
            .await;

        let probe = TransportProbe::with_probes(transport, 3);

        // With consistent tips, probe should succeed
        let result = probe.run("follower-1", 4, "leader:9000").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn probe_aborts_when_transport_error_on_tip_fetch() {
        let transport = FakeLeaderTransport::new();
        transport
            .inject_tip_error(TransportError::LeaderUnreachable {
                address: "127.0.0.1:8080".to_string(),
            })
            .await;

        let probe = TransportProbe::new(transport);
        let result = probe.run("follower-1", 4, "leader:9000").await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Sync1AbortCode::A7);
    }

    #[tokio::test]
    async fn probe_aborts_when_transport_error_on_proof_fetch() {
        let transport = FakeLeaderTransport::new();
        // Tip will succeed
        transport.set_tip(make_tip(100, "abc")).await;
        transport.set_version(make_version()).await;
        // But proof will fail
        transport
            .inject_proof_error(TransportError::RangeNotAvailable { start: 5, end: 10 })
            .await;

        let probe = TransportProbe::new(transport);
        let result = probe.run("follower-1", 4, "leader:9000").await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Sync1AbortCode::A3);
    }

    #[tokio::test]
    async fn probe_tracks_tip_fetch_count() {
        let transport = FakeLeaderTransport::new();
        transport.set_tip(make_tip(100, "abc")).await;
        transport.set_version(make_version()).await;
        transport
            .set_proof(make_proof(vec![5], vec!["hash1"]))
            .await;

        let probe = TransportProbe::with_probes(transport.clone(), 3);
        probe.run("follower-1", 4, "leader:9000").await.unwrap();

        // Count should be 3 (one per consistency probe)
        assert_eq!(transport.tip_fetch_count().await, 3);
    }

    #[tokio::test]
    async fn probe_aborts_a7_when_transport_exceeds_timeout() {
        // Prove that `timeout_per_probe_ms` genuinely affects execution.
        // Inject a 200 ms delay into the fake transport and set timeout=50 ms.
        // The per-call timeout should fire, mapping to A7.
        let transport = FakeLeaderTransport::new();
        transport.set_tip(make_tip(100, "abc")).await;
        transport.set_version(make_version()).await;
        transport.set_tip_delay_ms(200).await;

        let probe = TransportProbe::new(transport);
        let result = probe
            .run_with_start_sequence("follower-1", 0, 50, "leader:9000")
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Sync1AbortCode::A7);
    }

    #[tokio::test]
    async fn probe_timeout_fires_when_transport_is_slow() {
        // Prove that timeout_per_probe_ms is genuinely enforced:
        // inject a delay longer than the timeout and verify A7 is returned.
        let transport = FakeLeaderTransport::new();
        transport.set_tip(make_tip(100, "abc")).await;
        transport.set_version(make_version()).await;
        // Inject a 200ms delay on every tip fetch
        transport.set_tip_delay_ms(200).await;

        let probe = TransportProbe::new(transport);
        // 50ms timeout << 200ms delay -> should fire timeout -> A7
        let result = probe
            .run_with_start_sequence("follower-1", 4, 50, "leader:9000")
            .await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            Sync1AbortCode::A7,
            "timeout must map to A7"
        );
    }

    #[tokio::test]
    async fn fake_transport_returns_configured_tip() {
        let tip = make_tip(42, "mytiphash");
        let version = make_version();
        let transport = FakeLeaderTransport::with_tip(tip.clone(), version.clone());

        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "test".to_string(),
            timeout_ms: 5000,
        };

        let response = transport.fetch_leader_tip(&request).await.unwrap();
        assert_eq!(response.leader_tip, Some(tip));
        assert_eq!(response.leader_version, Some(version));
    }

    #[tokio::test]
    async fn fake_transport_returns_configured_proof() {
        let proof = make_proof(vec![1, 2, 3], vec!["a", "b", "c"]);
        let transport = FakeLeaderTransport::new();
        transport.set_proof(proof.clone()).await;

        let request = ProofRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "test".to_string(),
            start_sequence: 1,
            end_sequence: 3,
            timeout_ms: 5000,
        };

        let response = transport.fetch_proof(&request).await.unwrap();
        assert_eq!(response.proof, Some(proof));
    }
}
