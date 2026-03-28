//! Sync-owned read-only preflight port (trait-only slice).
//!
//! This module defines the `SyncPreflightRepo` trait and supporting types for
//! repo-backed preflight reads in Sync-2. It is the **trait-only groundwork**
//! for P3 repo integration; no SQLite/store implementations live here.
//!
//! ## What Is Here
//!
//! - `SyncPreflightRepo` trait: read-only methods that a concrete repo (SQLite,
//!   in-memory test double, etc.) must implement so that Sync-2 preflight can
//!   query local state without depending on `ferrum-store`.
//! - `LocalPreflightState`: a local-state snapshot carrying the follower tip
//!   and PF2/PF6/PF7 booleans.
//! - `SyncRepoError`: error type for repo operations.
//!
//! ## What Is NOT Here (Explicitly Deferred)
//!
//! - **PF3 (leader identity)**: leader address/identity discovery is a transport
//!   or config concern, not a repo query. It stays outside this trait. Callers
//!   supply it via the `leader_identity_known` parameter of `build_preflight_input()`.
//! - **SQLite/store wiring**: deferred to P3.
//! - **Write/apply path**: out of scope.
//! - **Network/transport code**: out of scope.
//! - **Session state implementation**: out of scope.
//!
//! ## Fail-Closed Guarantees
//!
//! Any repo error during trait method invocation should be surfaced as
//! `SyncRepoError`. The caller must treat errors as preflight failures
//! (map to PF1/PF5 aborts), never silently continue.

use crate::decision::TipId;

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
    ChainIntegrityFailed {
        /// Human-readable reason.
        reason: String,
    },

    /// The local ledger / repo is not readable.
    #[error("local ledger not readable: {reason}")]
    LedgerNotReadable {
        /// Human-readable reason.
        reason: String,
    },

    /// An internal repo error occurred (e.g., I/O, SQLite, serialization).
    #[error("internal repo error: {reason}")]
    InternalError {
        /// Human-readable reason.
        reason: String,
    },
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
///
/// PF3 (leader identity known) is deliberately excluded. It is not a repo
/// concern; it comes from transport/config and is supplied externally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPreflightState {
    /// Current tip of the local (follower) ledger.
    ///
    /// `None` means the ledger is empty (valid bootstrap target).
    pub follower_tip: Option<TipId>,

    /// `true` if there are in-flight commits on the local node (PF2).
    ///
    /// When `true`, `run_preflight()` will fail PF2.
    pub has_inflight_commits: bool,

    /// `true` if the local ledger has uncommitted local entries (PF6).
    ///
    /// When `true`, `run_preflight()` will fail PF6.
    pub has_uncommitted_entries: bool,

    /// `true` if the local node is currently syncing (PF7).
    ///
    /// When `true`, `run_preflight()` will fail PF7.
    pub sync_in_progress: bool,
}

impl LocalPreflightState {
    /// Create a `LocalPreflightState` representing a clean, idle ledger.
    ///
    /// - No in-flight commits
    /// - No uncommitted entries
    /// - No sync in progress
    /// - Follower tip is `None` (empty ledger)
    ///
    /// Useful as a test baseline.
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
    ///
    /// (PF1/PF5 are derived from `verify_local_chain()` success, not this struct.)
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
///
/// ## Concrete Implementations
///
/// This is a **trait-only slice**. Concrete implementations (SQLite, in-memory
/// test doubles) are deferred to P3. For now, test code can use a manual stub
/// or the `build_preflight_input()` helper with hand-crafted `LocalPreflightState`.
///
/// ## Fail-Closed Contract
///
/// All methods return `Result`. An `Err` must be treated as a preflight failure
/// by the caller; it must never be silently ignored or mapped to a passing state.
pub trait SyncPreflightRepo: Send + Sync {
    /// Verify the local chain's integrity and readability (PF1 + PF5).
    ///
    /// Returns `Ok(())` if the chain passes `verify_chain()` and the ledger
    /// is readable (i.e., `get_tip()` succeeds, even if it returns `None`
    /// for an empty ledger).
    ///
    /// Returns `Err(SyncRepoError)` if:
    /// - Chain integrity verification fails (PF1)
    /// - The ledger is not readable (PF5)
    fn verify_local_chain(&self) -> Result<(), SyncRepoError>;

    /// Read a snapshot of the local state needed for PF2/PF6/PF7 and the
    /// follower tip.
    ///
    /// This aggregates:
    /// - `follower_tip`: current local ledger tip (None if empty)
    /// - `has_inflight_commits`: PF2 flag
    /// - `has_uncommitted_entries`: PF6 flag
    /// - `sync_in_progress`: PF7 flag
    fn read_local_state(&self) -> Result<LocalPreflightState, SyncRepoError>;

    /// Check whether the given leader identity is authorized for sync (PF4).
    ///
    /// Returns `Ok(true)` if the leader is authorized, `Ok(false)` if not.
    /// Returns `Err` only for repo-level failures (e.g., capability store
    /// unreadable), not for "not authorized".
    fn is_leader_authorized(&self, leader_identity: &str) -> Result<bool, SyncRepoError>;

    /// Read the cached/known leader tip for the given leader identity (PF8).
    ///
    /// Returns `Ok(Some(tip))` if a leader tip is available, `Ok(None)` if
    /// no tip is known yet. Returns `Err` for repo-level failures.
    fn read_leader_tip(&self, leader_identity: &str) -> Result<Option<TipId>, SyncRepoError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(seq: u64, hash: &str) -> TipId {
        TipId {
            sequence: seq,
            hash: hash.to_string(),
        }
    }

    // =========================================================================
    // SyncRepoError
    // =========================================================================

    #[test]
    fn repo_error_is_chain_integrity_failure() {
        let err = SyncRepoError::ChainIntegrityFailed {
            reason: "broken hash".to_string(),
        };
        assert!(err.is_chain_integrity_failure());
        assert!(!err.is_ledger_not_readable());
    }

    #[test]
    fn repo_error_is_ledger_not_readable() {
        let err = SyncRepoError::LedgerNotReadable {
            reason: "io error".to_string(),
        };
        assert!(err.is_ledger_not_readable());
        assert!(!err.is_chain_integrity_failure());
    }

    #[test]
    fn repo_error_internal_neither() {
        let err = SyncRepoError::InternalError {
            reason: "sqlite".to_string(),
        };
        assert!(!err.is_chain_integrity_failure());
        assert!(!err.is_ledger_not_readable());
    }

    #[test]
    fn repo_error_display() {
        let err = SyncRepoError::ChainIntegrityFailed {
            reason: "bad".to_string(),
        };
        assert!(format!("{err}").contains("bad"));
    }

    // =========================================================================
    // LocalPreflightState — constructors
    // =========================================================================

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

    // =========================================================================
    // LocalPreflightState — is_local_state_clean
    // =========================================================================

    #[test]
    fn state_not_clean_with_inflight_commits() {
        let state = LocalPreflightState {
            has_inflight_commits: true,
            ..LocalPreflightState::clean_empty()
        };
        assert!(!state.is_local_state_clean());
    }

    #[test]
    fn state_not_clean_with_uncommitted_entries() {
        let state = LocalPreflightState {
            has_uncommitted_entries: true,
            ..LocalPreflightState::clean_empty()
        };
        assert!(!state.is_local_state_clean());
    }

    #[test]
    fn state_not_clean_with_sync_in_progress() {
        let state = LocalPreflightState {
            sync_in_progress: true,
            ..LocalPreflightState::clean_empty()
        };
        assert!(!state.is_local_state_clean());
    }

    #[test]
    fn state_not_clean_with_all_flags_set() {
        let state = LocalPreflightState {
            follower_tip: None,
            has_inflight_commits: true,
            has_uncommitted_entries: true,
            sync_in_progress: true,
        };
        assert!(!state.is_local_state_clean());
    }

    // =========================================================================
    // LocalPreflightState — equality
    // =========================================================================

    #[test]
    fn state_equality() {
        let t = tip(10, "h");
        let a = LocalPreflightState::clean_with_tip(t.clone());
        let b = LocalPreflightState::clean_with_tip(t);
        assert_eq!(a, b);
    }

    #[test]
    fn state_inequality_tip() {
        let a = LocalPreflightState::clean_with_tip(tip(10, "h1"));
        let b = LocalPreflightState::clean_with_tip(tip(10, "h2"));
        assert_ne!(a, b);
    }

    #[test]
    fn state_inequality_flags() {
        let a = LocalPreflightState::clean_empty();
        let mut b = LocalPreflightState::clean_empty();
        b.has_inflight_commits = true;
        assert_ne!(a, b);
    }

    // =========================================================================
    // Trait is object-safe (compile-time check)
    // =========================================================================

    #[test]
    fn trait_is_object_safe() {
        // This test exists to prove the trait is object-safe.
        // We create a fn that accepts a dyn trait reference.
        // If the trait were not object-safe, this would not compile.
        fn _accepts_dyn(_: &dyn SyncPreflightRepo) {}
        // We cannot call it without a concrete impl, but compilation is enough.
        assert!(true, "SyncPreflightRepo is object-safe");
    }
}
