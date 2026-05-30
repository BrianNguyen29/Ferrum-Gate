//! Sync-owned read-only preflight port (trait + test double).
//!
//! This module defines the `SyncPreflightRepo` trait and supporting types for
//! repo-backed preflight reads in Sync-2. It lives in `ferrum-sync` (not
//! `ferrum-store`) because the sync crate owns the preflight contract; the store
//! provides concrete implementations.
//!
//! ## What Is Here
//!
//! - `SyncPreflightRepo` trait: read-only methods that a concrete repo (SQLite,
//!   in-memory test double, etc.) must implement so that Sync-2 preflight can
//!   query local state without depending on `ferrum-store`.
//! - `LocalPreflightState`: a local-state snapshot carrying the follower tip
//!   and PF2/PF6/PF7 booleans.
//! - `SyncRepoError`: error type for repo operations.
//! - `InMemorySyncPreflightRepo`: an in-memory test double (only for tests).
//!
//! ## Concrete Implementations
//!
//! | Implementation | Location | Supported Checks |
//! |----------------|----------|-------------------|
//! | `InMemorySyncPreflightRepo` | `ferrum-sync` (here) | All (manual stub for tests) |
//! | `SqliteSyncPreflightRepo` | `ferrum-store` | PF1, PF5 only; others return Err (fail-closed) |

use crate::transport::TipId;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error type for `SyncPreflightRepo` operations.
///
/// This covers all failure modes that can occur when the preflight port
/// queries the local repo. Callers map these to the appropriate preflight
/// failure code (PF1/PF5) and fail-closed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SyncRepoError {
    /// The local chain failed integrity verification.
    #[error("local chain integrity check failed: {reason}")]
    ChainIntegrityFailed { reason: String },

    /// The local ledger / repo is not readable.
    #[error("local ledger not readable: {reason}")]
    LedgerNotReadable { reason: String },

    /// An internal repo error occurred (e.g., I/O, SQLite, serialization).
    #[error("internal repo error: {reason}")]
    InternalError { reason: String },
}

impl SyncRepoError {
    /// Returns `true` if this error indicates a chain integrity failure (PF1).
    pub fn is_chain_integrity_failure(&self) -> bool {
        matches!(self, SyncRepoError::ChainIntegrityFailed { .. })
    }

    /// Returns `true` if this error indicates the ledger is not readable (PF5).
    pub fn is_ledger_not_readable(&self) -> bool {
        matches!(self, SyncRepoError::LedgerNotReadable { .. })
    }
}

// ---------------------------------------------------------------------------
// LocalPreflightState — snapshot of repo-backed local state
// ---------------------------------------------------------------------------

/// Snapshot of local state relevant to Sync-2 preflight.
///
/// This struct aggregates the repo-queryable booleans and the follower tip
/// into a single read. A concrete `SyncPreflightRepo` implementation fills
/// this from one or more store queries.
///
/// Fields map to preflight checks as follows:
///
/// | Field                      | PF Check | Meaning                                 |
/// |----------------------------|----------|-----------------------------------------|
/// | `follower_tip`             | -        | Current tip of the local ledger         |
/// | `has_inflight_commits`     | PF2      | True means in-flight commits exist      |
/// | `has_uncommitted_entries`  | PF6      | True means uncommitted local entries    |
/// | `sync_in_progress`         | PF7      | True means a sync session is active     |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPreflightState {
    /// Current tip of the local (follower) ledger.
    /// `None` means the ledger is empty (valid bootstrap target).
    pub follower_tip: Option<TipId>,

    /// `true` if there are in-flight commits on the local node (PF2).
    pub has_inflight_commits: bool,

    /// `true` if the local ledger has uncommitted local entries (PF6).
    pub has_uncommitted_entries: bool,

    /// `true` if the local node is currently syncing (PF7).
    pub sync_in_progress: bool,
}

impl LocalPreflightState {
    /// Create a `LocalPreflightState` representing a clean, idle ledger.
    ///
    /// - No in-flight commits
    /// - No uncommitted entries
    /// - No sync in progress
    /// - Follower tip is `None` (empty ledger)
    pub fn clean_empty() -> Self {
        Self {
            follower_tip: None,
            has_inflight_commits: false,
            has_uncommitted_entries: false,
            sync_in_progress: false,
        }
    }

    /// Create a `LocalPreflightState` representing a clean, idle ledger
    /// with the given follower tip.
    pub fn clean_with_tip(tip: TipId) -> Self {
        Self {
            follower_tip: Some(tip),
            has_inflight_commits: false,
            has_uncommitted_entries: false,
            sync_in_progress: false,
        }
    }

    /// Returns `true` if all PF2/PF6/PF7 boolean flags are in a passing state.
    pub fn is_local_state_clean(&self) -> bool {
        !self.has_inflight_commits && !self.has_uncommitted_entries && !self.sync_in_progress
    }
}

// ---------------------------------------------------------------------------
// SyncPreflightRepo trait — read-only preflight port
// ---------------------------------------------------------------------------

/// Read-only repo port for Sync-2 preflight queries.
///
/// This trait defines the minimal set of repo operations that Sync-2 preflight
/// needs from the local store. It lives in `ferrum-sync` (not `ferrum-store`)
/// because the sync crate owns the preflight contract; the store provides
/// implementations.
///
/// ## Method-to-PF Mapping
///
/// | Method                  | PF Check(s) | Notes                                   |
/// |-------------------------|-------------|-----------------------------------------|
/// | `verify_local_chain()`  | PF1, PF5   | Covers chain integrity + ledger readability |
/// | `read_local_state()`    | PF2, PF6, PF7 + tip | Aggregated snapshot query        |
/// | `is_leader_authorized()`| PF4         | Capability check for given leader       |
/// | `read_leader_tip()`     | PF8         | Read cached/known leader tip            |
///
/// ## PF3 Is Deliberately Excluded
///
/// PF3 (leader address/identity known) is a transport/config concern, not a
/// repo query. It does not belong on this trait. Callers supply it
/// externally via the `leader_identity_known` parameter when building
/// `PreflightInput`.
pub trait SyncPreflightRepo: Send + Sync {
    /// Verify the local chain's integrity and readability (PF1 + PF5).
    ///
    /// Returns `Ok(())` if the chain passes `verify_chain()` and the ledger
    /// is readable (i.e., `get_tip()` succeeds, even if it returns `None`
    /// for an empty ledger).
    fn verify_local_chain(&self) -> Result<(), SyncRepoError>;

    /// Read a snapshot of the local state needed for PF2/PF6/PF7 and the
    /// follower tip.
    fn read_local_state(&self) -> Result<LocalPreflightState, SyncRepoError>;

    /// Check whether the given leader identity is authorized for sync (PF4).
    ///
    /// Returns `Ok(true)` if the leader is authorized, `Ok(false)` if not.
    /// Returns `Err` only for repo-level failures (e.g., capability store
    /// unreadable), not for "not authorized".
    fn is_leader_authorized(&self, leader_identity: &str) -> Result<bool, SyncRepoError>;

    /// Read the cached/known leader tip for the given leader address (PF8).
    ///
    /// Returns `Ok(Some(tip))` if a leader tip is available, `Ok(None)` if
    /// no tip is known yet. Returns `Err` for repo-level failures.
    fn read_leader_tip(&self, leader_address: &str) -> Result<Option<TipId>, SyncRepoError>;

    /// Compute `hash_path_valid` for Sync-1 decision by verifying local chain integrity.
    ///
    /// Fail-closed: returns `false` on any repo error.
    fn verify_local_chain_for_hash_path_valid(&self) -> bool;
}

// ---------------------------------------------------------------------------
// InMemorySyncPreflightRepo — test double
// ---------------------------------------------------------------------------

/// In-memory test double for `SyncPreflightRepo`.
///
/// This is intended **only** for tests. It holds pre-configured results for
/// each trait method.
#[derive(Debug, Clone)]
pub struct InMemorySyncPreflightRepo {
    verify_local_chain_result: Result<(), SyncRepoError>,
    read_local_state_result: Result<LocalPreflightState, SyncRepoError>,
    is_leader_authorized_result: Result<bool, SyncRepoError>,
    read_leader_tip_result: Result<Option<TipId>, SyncRepoError>,
    hash_path_valid_result: bool,
}

impl InMemorySyncPreflightRepo {
    /// Create a new test double with all methods returning errors (fail-closed).
    pub fn new() -> Self {
        Self {
            verify_local_chain_result: Err(SyncRepoError::InternalError {
                reason: "InMemorySyncPreflightRepo: verify_local_chain not configured".to_string(),
            }),
            read_local_state_result: Err(SyncRepoError::InternalError {
                reason: "InMemorySyncPreflightRepo: read_local_state not configured".to_string(),
            }),
            is_leader_authorized_result: Err(SyncRepoError::InternalError {
                reason: "InMemorySyncPreflightRepo: is_leader_authorized not configured"
                    .to_string(),
            }),
            read_leader_tip_result: Err(SyncRepoError::InternalError {
                reason: "InMemorySyncPreflightRepo: read_leader_tip not configured".to_string(),
            }),
            hash_path_valid_result: false,
        }
    }

    pub fn with_verify_local_chain(mut self, result: Result<(), SyncRepoError>) -> Self {
        self.verify_local_chain_result = result;
        self
    }

    pub fn with_read_local_state(
        mut self,
        result: Result<LocalPreflightState, SyncRepoError>,
    ) -> Self {
        self.read_local_state_result = result;
        self
    }

    pub fn with_is_leader_authorized(mut self, result: Result<bool, SyncRepoError>) -> Self {
        self.is_leader_authorized_result = result;
        self
    }

    pub fn with_read_leader_tip(mut self, result: Result<Option<TipId>, SyncRepoError>) -> Self {
        self.read_leader_tip_result = result;
        self
    }

    pub fn with_hash_path_valid(mut self, result: bool) -> Self {
        self.hash_path_valid_result = result;
        self
    }
}

impl Default for InMemorySyncPreflightRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncPreflightRepo for InMemorySyncPreflightRepo {
    fn verify_local_chain(&self) -> Result<(), SyncRepoError> {
        self.verify_local_chain_result.clone()
    }

    fn read_local_state(&self) -> Result<LocalPreflightState, SyncRepoError> {
        self.read_local_state_result.clone()
    }

    fn is_leader_authorized(&self, _leader_identity: &str) -> Result<bool, SyncRepoError> {
        self.is_leader_authorized_result.clone()
    }

    fn read_leader_tip(&self, _leader_address: &str) -> Result<Option<TipId>, SyncRepoError> {
        self.read_leader_tip_result.clone()
    }

    fn verify_local_chain_for_hash_path_valid(&self) -> bool {
        self.hash_path_valid_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(seq: u64, hash: &str) -> TipId {
        TipId {
            sequence: seq,
            hash: hash.to_string(),
        }
    }

    #[test]
    fn clean_empty_state() {
        let state = LocalPreflightState::clean_empty();
        assert!(state.follower_tip.is_none());
        assert!(!state.has_inflight_commits);
        assert!(!state.has_uncommitted_entries);
        assert!(!state.sync_in_progress);
        assert!(state.is_local_state_clean());
    }

    #[test]
    fn clean_with_tip_state() {
        let t = tip(42, "abc");
        let state = LocalPreflightState::clean_with_tip(t.clone());
        assert_eq!(state.follower_tip, Some(t));
        assert!(state.is_local_state_clean());
    }

    #[test]
    fn in_memory_default_returns_err() {
        let repo = InMemorySyncPreflightRepo::new();
        assert!(repo.verify_local_chain().is_err());
        assert!(repo.read_local_state().is_err());
        assert!(repo.is_leader_authorized("x").is_err());
        assert!(repo.read_leader_tip("x").is_err());
        assert!(!repo.verify_local_chain_for_hash_path_valid());
    }

    #[test]
    fn in_memory_configured_verify_ok() {
        let repo = InMemorySyncPreflightRepo::new().with_verify_local_chain(Ok(()));
        assert!(repo.verify_local_chain().is_ok());
    }

    #[test]
    fn in_memory_configured_read_local_state() {
        let repo = InMemorySyncPreflightRepo::new()
            .with_read_local_state(Ok(LocalPreflightState::clean_empty()));
        let state = repo.read_local_state().unwrap();
        assert!(state.is_local_state_clean());
    }

    #[test]
    fn in_memory_configured_is_leader_authorized_true() {
        let repo = InMemorySyncPreflightRepo::new().with_is_leader_authorized(Ok(true));
        assert!(repo.is_leader_authorized("leader-1").unwrap());
    }

    #[test]
    fn in_memory_configured_read_leader_tip_some() {
        let t = tip(10, "leaderhash");
        let repo = InMemorySyncPreflightRepo::new().with_read_leader_tip(Ok(Some(t.clone())));
        assert_eq!(repo.read_leader_tip("leader-1").unwrap(), Some(t));
    }

    #[test]
    fn in_memory_configured_read_leader_tip_none() {
        let repo = InMemorySyncPreflightRepo::new().with_read_leader_tip(Ok(None));
        assert_eq!(repo.read_leader_tip("leader-1").unwrap(), None);
    }
}
