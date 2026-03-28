//! Error types and Sync-1 abort code mapping for Sync-3a diagnostic transport probe.
//!
//! All transport errors are mapped to Sync-1 abort codes in fail-closed fashion:
//! any ambiguity or failure results in an abort, never a sync-able state.

use crate::transport::TransportError;
use thiserror::Error;

/// Sync-1 abort codes used by the diagnostic probe.
///
/// These are the only allowed return values from the probe; raw TransportError
/// is never exposed to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sync1AbortCode {
    /// A0: Unknown/preflight failure
    A0,
    /// A3: Hash path invalid / proof structure invalid
    A3,
    /// A4: Follower ahead (should not reach probe)
    A4,
    /// A5: Entry verification failed
    A5,
    /// A6: Divergent (should not reach probe)
    A6,
    /// A7: Network error / leader unreachable / timeout / version incompatible
    A7,
    /// A8: Capability denied
    A8,
}

impl std::fmt::Display for Sync1AbortCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sync1AbortCode::A0 => write!(f, "A0"),
            Sync1AbortCode::A3 => write!(f, "A3"),
            Sync1AbortCode::A4 => write!(f, "A4"),
            Sync1AbortCode::A5 => write!(f, "A5"),
            Sync1AbortCode::A6 => write!(f, "A6"),
            Sync1AbortCode::A7 => write!(f, "A7"),
            Sync1AbortCode::A8 => write!(f, "A8"),
        }
    }
}

/// Probe-specific errors that occur during diagnostic validation.
///
/// These are distinct from TransportError because they represent failures
/// in the diagnostic layer itself (e.g., inconsistent tips across probes).
#[derive(Debug, Clone, Error)]
pub enum ProbeError {
    /// Multiple tip probes returned inconsistent results.
    #[error("tip inconsistent across {probe_count} probes: first={first:?}, last={last:?}")]
    TipInconsistent {
        probe_count: usize,
        first: crate::transport::LeaderTip,
        last: crate::transport::LeaderTip,
    },

    /// Proof structure validation failed.
    #[error("proof structure invalid: {reason}")]
    ProofStructureInvalid { reason: String },

    /// Range not available on leader.
    #[error("range not available: {start}..{end}")]
    RangeNotAvailable { start: u64, end: u64 },

    /// Internal diagnostic error (e.g., assertion failure).
    #[error("internal error: {details}")]
    InternalError { details: String },
}

impl ProbeError {
    /// Map a ProbeError to a Sync-1 abort code in fail-closed fashion.
    pub fn to_abort_code(&self) -> Sync1AbortCode {
        match self {
            // TipInconsistent maps to A7 (Network error) per Sync-3a spec
            ProbeError::TipInconsistent { .. } => Sync1AbortCode::A7,

            // ProofStructureInvalid maps to A3 (HashPathInvalid)
            ProbeError::ProofStructureInvalid { .. } => Sync1AbortCode::A3,

            // RangeNotAvailable maps to A3 per Sync-3 spec
            ProbeError::RangeNotAvailable { .. } => Sync1AbortCode::A3,

            // InternalError maps to A7 (treat as unreachable)
            ProbeError::InternalError { .. } => Sync1AbortCode::A7,
        }
    }
}

/// Maps a TransportError to a Sync-1 abort code in fail-closed fashion.
///
/// This implements the error mapping table from Sync-3:
/// | Transport Error          | Sync-1 Abort Code |
/// |------------------------|--------------------|
/// | LeaderUnreachable      | A7 (Network error) |
/// | LeaderTimeout          | A7 (Network error) |
/// | LeaderCapabilityDenied | A8 (Capability denied) |
/// | LeaderVersionIncompatible | A7 (Network error) |
/// | RangeNotAvailable      | A3 (HashPathInvalid) |
/// | InternalError          | A7 (Network error) |
pub fn map_transport_error_to_abort(err: &TransportError) -> Sync1AbortCode {
    match err {
        TransportError::LeaderUnreachable { .. } => Sync1AbortCode::A7,
        TransportError::LeaderTimeout { .. } => Sync1AbortCode::A7,
        TransportError::LeaderCapabilityDenied { .. } => Sync1AbortCode::A8,
        TransportError::LeaderVersionIncompatible { .. } => Sync1AbortCode::A7,
        TransportError::RangeNotAvailable { .. } => Sync1AbortCode::A3,
        TransportError::InternalError { .. } => Sync1AbortCode::A7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::LeaderTip;
    use chrono::Utc;

    #[test]
    fn probe_error_to_abort_code_tip_inconsistent() {
        let tip1 = LeaderTip {
            sequence: 10,
            hash: "abc".to_string(),
            timestamp: Utc::now(),
        };
        let tip2 = LeaderTip {
            sequence: 11,
            hash: "def".to_string(),
            timestamp: Utc::now(),
        };

        let err = ProbeError::TipInconsistent {
            probe_count: 3,
            first: tip1.clone(),
            last: tip2.clone(),
        };

        assert_eq!(err.to_abort_code(), Sync1AbortCode::A7);
    }

    #[test]
    fn probe_error_to_abort_code_proof_structure_invalid() {
        let err = ProbeError::ProofStructureInvalid {
            reason: "entries empty".to_string(),
        };

        assert_eq!(err.to_abort_code(), Sync1AbortCode::A3);
    }

    #[test]
    fn probe_error_to_abort_code_range_not_available() {
        let err = ProbeError::RangeNotAvailable { start: 5, end: 10 };

        assert_eq!(err.to_abort_code(), Sync1AbortCode::A3);
    }

    #[test]
    fn map_transport_error_leader_unreachable() {
        let err = TransportError::LeaderUnreachable {
            address: "127.0.0.1:8080".to_string(),
        };
        assert_eq!(map_transport_error_to_abort(&err), Sync1AbortCode::A7);
    }

    #[test]
    fn map_transport_error_leader_timeout() {
        let err = TransportError::LeaderTimeout {
            address: "127.0.0.1:8080".to_string(),
            duration_ms: 5000,
        };
        assert_eq!(map_transport_error_to_abort(&err), Sync1AbortCode::A7);
    }

    #[test]
    fn map_transport_error_capability_denied() {
        let err = TransportError::LeaderCapabilityDenied {
            leader: "node-1".to_string(),
            required_capability: "sync".to_string(),
        };
        assert_eq!(map_transport_error_to_abort(&err), Sync1AbortCode::A8);
    }

    #[test]
    fn map_transport_error_version_incompatible() {
        let err = TransportError::LeaderVersionIncompatible {
            leader_version: "2.0.0".to_string(),
            follower_min_version: "1.0.0".to_string(),
        };
        assert_eq!(map_transport_error_to_abort(&err), Sync1AbortCode::A7);
    }

    #[test]
    fn map_transport_error_range_not_available() {
        let err = TransportError::RangeNotAvailable { start: 5, end: 10 };
        assert_eq!(map_transport_error_to_abort(&err), Sync1AbortCode::A3);
    }

    #[test]
    fn map_transport_error_internal() {
        let err = TransportError::InternalError {
            details: "connection reset".to_string(),
        };
        assert_eq!(map_transport_error_to_abort(&err), Sync1AbortCode::A7);
    }
}
