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
//!
//! ## What Is NOT Here
//!
//! - **PF3 (leader identity)**: leader address/identity discovery is a transport
//!   or config concern, not a repo query. It stays outside this trait. Callers
//!   supply it via the `leader_identity_known` parameter of `build_preflight_input()`.
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
/// The trait is implemented by two types:
///
/// - **`InMemorySyncPreflightRepo`** (in `ferrum-sync`): an in-memory test double
///   for unit tests. All methods return errors by default; configure with builder
///   methods before use.
/// - **`SqliteSyncPreflightRepo`** (in `ferrum-store`): the real SQLite backend.
///   Currently supports `verify_local_chain()` (PF1 + PF5) only. All other
///   methods return `Err` (fail-closed) because the schema does not yet contain
///   tables for PF2/PF6/PF7 (inflight commits, uncommitted entries, sync sessions)
///   or PF4/PF8 (leader authorization, leader tip cache).
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

    /// Read the cached/known leader tip for the given leader address (PF8).
    ///
    /// `leader_address` is the stable identifier used at the transport boundary
    /// (e.g., "leader:9000"). This is the same key used when caching a tip
    /// via `write_leader_tip_test_only`.
    ///
    /// Returns `Ok(Some(tip))` if a leader tip is available, `Ok(None)` if
    /// no tip is known yet. Returns `Err` for repo-level failures.
    fn read_leader_tip(&self, leader_address: &str) -> Result<Option<TipId>, SyncRepoError>;
}

// ---------------------------------------------------------------------------
// InMemorySyncPreflightRepo — test double
// ---------------------------------------------------------------------------

/// In-memory test double for `SyncPreflightRepo`.
///
/// This is intended **only** for tests. It holds pre-configured results for
/// each trait method, allowing test code to drive `build_preflight_input()`
/// and `run_preflight()` without a real database.
///
/// ## Fail-Closed Defaults
///
/// By default, all methods return errors. You must explicitly configure the
/// results you want. This ensures tests are intentional about what they assert
/// and prevents accidentally passing tests on permissive defaults.
///
/// ## Usage
///
/// ```ignore
/// use ferrum_sync::repo::{InMemorySyncPreflightRepo, LocalPreflightState, SyncRepoError};
///
/// let repo = InMemorySyncPreflightRepo::new()
///     .with_verify_local_chain(Ok(()))
///     .with_read_local_state(Ok(LocalPreflightState::clean_empty()));
///
/// assert!(repo.verify_local_chain().is_ok());
/// ```
#[derive(Debug, Clone)]
pub struct InMemorySyncPreflightRepo {
    /// Pre-configured result for `verify_local_chain()`.
    verify_local_chain_result: Result<(), SyncRepoError>,
    /// Pre-configured result for `read_local_state()`.
    read_local_state_result: Result<LocalPreflightState, SyncRepoError>,
    /// Pre-configured result for `is_leader_authorized()`.
    is_leader_authorized_result: Result<bool, SyncRepoError>,
    /// Pre-configured result for `read_leader_tip()`.
    read_leader_tip_result: Result<Option<TipId>, SyncRepoError>,
}

impl InMemorySyncPreflightRepo {
    /// Create a new test double with all methods returning
    /// `Err(SyncRepoError::InternalError)` (fail-closed default).
    ///
    /// Use the `with_*` builder methods to configure each method's result.
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
        }
    }

    /// Configure the result for `verify_local_chain()`.
    pub fn with_verify_local_chain(mut self, result: Result<(), SyncRepoError>) -> Self {
        self.verify_local_chain_result = result;
        self
    }

    /// Configure the result for `read_local_state()`.
    pub fn with_read_local_state(
        mut self,
        result: Result<LocalPreflightState, SyncRepoError>,
    ) -> Self {
        self.read_local_state_result = result;
        self
    }

    /// Configure the result for `is_leader_authorized()`.
    pub fn with_is_leader_authorized(mut self, result: Result<bool, SyncRepoError>) -> Self {
        self.is_leader_authorized_result = result;
        self
    }

    /// Configure the result for `read_leader_tip()`.
    pub fn with_read_leader_tip(mut self, result: Result<Option<TipId>, SyncRepoError>) -> Self {
        self.read_leader_tip_result = result;
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

    // =========================================================================
    // InMemorySyncPreflightRepo — fail-closed defaults
    // =========================================================================

    #[test]
    fn in_memory_default_verify_local_chain_returns_err() {
        let repo = InMemorySyncPreflightRepo::new();
        let result = repo.verify_local_chain();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{err}").contains("not configured"));
    }

    #[test]
    fn in_memory_default_read_local_state_returns_err() {
        let repo = InMemorySyncPreflightRepo::new();
        let result = repo.read_local_state();
        assert!(result.is_err());
    }

    #[test]
    fn in_memory_default_is_leader_authorized_returns_err() {
        let repo = InMemorySyncPreflightRepo::new();
        let result = repo.is_leader_authorized("leader-1");
        assert!(result.is_err());
    }

    #[test]
    fn in_memory_default_read_leader_tip_returns_err() {
        let repo = InMemorySyncPreflightRepo::new();
        let result = repo.read_leader_tip("leader-1");
        assert!(result.is_err());
    }

    // =========================================================================
    // InMemorySyncPreflightRepo — configured values
    // =========================================================================

    #[test]
    fn in_memory_configured_verify_ok() {
        let repo = InMemorySyncPreflightRepo::new().with_verify_local_chain(Ok(()));
        assert!(repo.verify_local_chain().is_ok());
    }

    #[test]
    fn in_memory_configured_verify_chain_integrity_failed() {
        let repo = InMemorySyncPreflightRepo::new().with_verify_local_chain(Err(
            SyncRepoError::ChainIntegrityFailed {
                reason: "test: broken chain".to_string(),
            },
        ));
        let result = repo.verify_local_chain();
        assert!(result.is_err());
        assert!(result.unwrap_err().is_chain_integrity_failure());
    }

    #[test]
    fn in_memory_configured_read_local_state_clean() {
        let repo = InMemorySyncPreflightRepo::new()
            .with_read_local_state(Ok(LocalPreflightState::clean_empty()));
        let state = repo.read_local_state().unwrap();
        assert!(state.is_local_state_clean());
        assert!(state.follower_tip.is_none());
    }

    #[test]
    fn in_memory_configured_read_local_state_with_tip() {
        let t = tip(99, "abc");
        let repo = InMemorySyncPreflightRepo::new()
            .with_read_local_state(Ok(LocalPreflightState::clean_with_tip(t.clone())));
        let state = repo.read_local_state().unwrap();
        assert_eq!(state.follower_tip, Some(t));
    }

    #[test]
    fn in_memory_configured_is_leader_authorized_true() {
        let repo = InMemorySyncPreflightRepo::new().with_is_leader_authorized(Ok(true));
        assert!(repo.is_leader_authorized("leader-1").unwrap());
    }

    #[test]
    fn in_memory_configured_is_leader_authorized_false() {
        let repo = InMemorySyncPreflightRepo::new().with_is_leader_authorized(Ok(false));
        assert!(!repo.is_leader_authorized("unknown").unwrap());
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

    // =========================================================================
    // Contract wiring: InMemorySyncPreflightRepo -> build_preflight_input -> run_preflight
    // =========================================================================

    /// Helper: fully-wired test double that simulates a clean, passing repo.
    fn clean_passing_repo() -> InMemorySyncPreflightRepo {
        InMemorySyncPreflightRepo::new()
            .with_verify_local_chain(Ok(()))
            .with_read_local_state(Ok(LocalPreflightState::clean_empty()))
            .with_is_leader_authorized(Ok(true))
            .with_read_leader_tip(Ok(None)) // no leader tip known yet
    }

    #[test]
    fn contract_wiring_all_passing_repo_produces_passing_preflight() {
        let repo = clean_passing_repo();

        // Step 1: verify chain
        let chain_ok = repo.verify_local_chain().is_ok();
        assert!(chain_ok);

        // Step 2: read local state
        let local_state = repo.read_local_state().unwrap();
        assert!(local_state.is_local_state_clean());

        // Step 3: query leader authorization
        let leader_authorized = repo.is_leader_authorized("leader-1").unwrap();

        // Step 4: query leader tip
        let leader_tip = repo.read_leader_tip("leader-1").unwrap();
        let leader_tip_available = leader_tip.is_some();

        // Step 5: build preflight input (PF3 is external, supply true)
        let input = crate::preflight::build_preflight_input(
            &local_state,
            chain_ok,
            true, // leader_identity_known (external)
            leader_authorized,
            leader_tip_available,
        );

        // Step 6: run preflight -- should pass because we configured
        // clean state with a known leader. PF8 fails because no tip,
        // so we need to provide a tip for PF8 to pass.
        let result = crate::preflight::run_preflight(&input);
        // PF8 fails because leader_tip is None -> leader_tip_available = false
        assert_eq!(
            result,
            crate::preflight::PreflightResult::Fail(crate::preflight::PreflightCheckCode::PF8)
        );
    }

    #[test]
    fn contract_wiring_all_passing_with_leader_tip_produces_pass() {
        let t = tip(50, "leaderhash");
        let repo = InMemorySyncPreflightRepo::new()
            .with_verify_local_chain(Ok(()))
            .with_read_local_state(Ok(LocalPreflightState::clean_empty()))
            .with_is_leader_authorized(Ok(true))
            .with_read_leader_tip(Ok(Some(t)));

        let chain_ok = repo.verify_local_chain().is_ok();
        let local_state = repo.read_local_state().unwrap();
        let leader_authorized = repo.is_leader_authorized("leader-1").unwrap();
        let leader_tip = repo.read_leader_tip("leader-1").unwrap();
        let leader_tip_available = leader_tip.is_some();

        let input = crate::preflight::build_preflight_input(
            &local_state,
            chain_ok,
            true,
            leader_authorized,
            leader_tip_available,
        );

        let result = crate::preflight::run_preflight(&input);
        assert_eq!(result, crate::preflight::PreflightResult::Pass);
    }

    #[test]
    fn contract_wiring_chain_failure_fails_pf1() {
        let repo = InMemorySyncPreflightRepo::new()
            .with_verify_local_chain(Err(SyncRepoError::ChainIntegrityFailed {
                reason: "broken".to_string(),
            }))
            .with_read_local_state(Ok(LocalPreflightState::clean_empty()))
            .with_is_leader_authorized(Ok(true))
            .with_read_leader_tip(Ok(Some(tip(1, "h"))));

        // Caller maps verify error -> chain_ok = false
        let chain_ok = repo.verify_local_chain().is_ok();
        assert!(!chain_ok);

        let local_state = repo.read_local_state().unwrap();
        let leader_authorized = repo.is_leader_authorized("leader-1").unwrap();
        let leader_tip = repo.read_leader_tip("leader-1").unwrap();

        let input = crate::preflight::build_preflight_input(
            &local_state,
            chain_ok, // false
            true,
            leader_authorized,
            leader_tip.is_some(),
        );

        let result = crate::preflight::run_preflight(&input);
        assert_eq!(
            result,
            crate::preflight::PreflightResult::Fail(crate::preflight::PreflightCheckCode::PF1)
        );
    }

    #[test]
    fn contract_wiring_inflight_commits_fails_pf2() {
        let repo = InMemorySyncPreflightRepo::new()
            .with_verify_local_chain(Ok(()))
            .with_read_local_state(Ok(LocalPreflightState {
                follower_tip: None,
                has_inflight_commits: true,
                has_uncommitted_entries: false,
                sync_in_progress: false,
            }))
            .with_is_leader_authorized(Ok(true))
            .with_read_leader_tip(Ok(Some(tip(1, "h"))));

        let chain_ok = repo.verify_local_chain().is_ok();
        let local_state = repo.read_local_state().unwrap();
        assert!(local_state.has_inflight_commits);

        let input = crate::preflight::build_preflight_input(
            &local_state,
            chain_ok,
            true,
            repo.is_leader_authorized("leader-1").unwrap(),
            repo.read_leader_tip("leader-1").unwrap().is_some(),
        );

        let result = crate::preflight::run_preflight(&input);
        assert_eq!(
            result,
            crate::preflight::PreflightResult::Fail(crate::preflight::PreflightCheckCode::PF2)
        );
    }

    #[test]
    fn contract_wiring_unsupported_read_local_state_propagates_err() {
        let repo = InMemorySyncPreflightRepo::new()
            .with_verify_local_chain(Ok(()))
            .with_read_local_state(Err(SyncRepoError::InternalError {
                reason: "unsupported: no PF2/PF6/PF7 tables".to_string(),
            }));

        let chain_ok = repo.verify_local_chain().is_ok();
        assert!(chain_ok);

        let state_result = repo.read_local_state();
        assert!(state_result.is_err());
        // Cannot build preflight input without local state.
        // Caller must treat this as a preflight failure.
    }

    #[test]
    fn contract_wiring_unsupported_is_leader_authorized_propagates_err() {
        let repo = InMemorySyncPreflightRepo::new()
            .with_verify_local_chain(Ok(()))
            .with_read_local_state(Ok(LocalPreflightState::clean_empty()))
            .with_is_leader_authorized(Err(SyncRepoError::InternalError {
                reason: "unsupported: no capability model".to_string(),
            }));

        // Cannot determine leader authorization.
        // Caller must treat this as a preflight failure (PF4).
        let auth_result = repo.is_leader_authorized("leader-1");
        assert!(auth_result.is_err());
    }

    #[test]
    fn contract_wiring_dyn_dispatch_works() {
        // Prove that InMemorySyncPreflightRepo can be used through dyn trait.
        let repo: Box<dyn SyncPreflightRepo> = Box::new(
            InMemorySyncPreflightRepo::new()
                .with_verify_local_chain(Ok(()))
                .with_read_local_state(Ok(LocalPreflightState::clean_empty())),
        );

        assert!(repo.verify_local_chain().is_ok());
        assert!(repo.read_local_state().unwrap().is_local_state_clean());
    }

    // =========================================================================
    // InMemorySyncPreflightRepo — default trait
    // =========================================================================

    #[test]
    fn in_memory_default_trait_matches_new() {
        let a = InMemorySyncPreflightRepo::new();
        let b = InMemorySyncPreflightRepo::default();
        // Both should fail on every method
        assert!(a.verify_local_chain().is_err());
        assert!(b.verify_local_chain().is_err());
    }
}
