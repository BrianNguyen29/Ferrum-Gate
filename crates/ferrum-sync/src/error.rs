//! Error types for ferrum-sync, kept internal to this crate.

use thiserror::Error;

/// Result type alias for ferrum-sync operations.
pub type Result<T> = std::result::Result<T, SyncError>;

/// Errors that can occur in sync operations.
///
/// All errors are internal to ferrum-sync; no transport-specific
/// errors leak out of this crate's public API.
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("probe error: {0}")]
    Probe(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl SyncError {
    pub fn transport(msg: impl Into<String>) -> Self {
        Self::Transport(msg.into())
    }

    pub fn probe(msg: impl Into<String>) -> Self {
        Self::Probe(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

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
