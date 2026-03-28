//! Sync-2 Read-Only Preflight + Diff Classifier Groundwork.
//!
//! This module implements the read-only preflight checks (PF1-PF8) and diff
//! classifier from `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`.
//!
//! **This is groundwork / partial Sync-2**, not the full Sync-2 implementation.
//! It provides:
//!
//! - A pure `DiffClass` classifier that maps two ledger tips to a classification
//! - A pure `run_preflight()` function that checks PF1-PF8 preconditions
//! - A bridging function `diff_class_to_decision()` that maps `DiffClass` to
//!   the Sync-1 decision kernel's `Sync1Decision`
//!
//! ## What Is NOT Here (Deferred)
//!
//! - Actual ledger queries (PF1/PF5/PF6 require repo access; deferred to P3)
//! - Transport-based tip acquisition (PF3/PF8; Sync-3 territory)
//! - Sync session tracking (PF7; stateful, not read-only groundwork)
//! - Capability model enforcement (PF4; deferred to P3)
//! - In-flight commit detection (PF2; deferred to P3)
//!
//! ## Fail-Closed Guarantees
//!
//! - `classify()` returns `Unknown` on any ambiguity (never guesses)
//! - `run_preflight()` returns `Fail` on the first failing check
//! - `diff_class_to_decision()` maps `Unknown` to `Abort(A0)`
//! - No network calls, no repo queries, no mutation

use crate::decision::TipId;
use crate::error::Sync1AbortCode;

// ---------------------------------------------------------------------------
// DiffClass — read-only diff classifier output
// ---------------------------------------------------------------------------

/// Classification of the relationship between follower and leader ledger tips.
///
/// This is the output of the Sync-2 read-only diff classifier, as defined in
/// `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`.
///
/// Each variant maps to exactly one row in the Sync-2 decision table, which
/// in turn feeds the Sync-1 decision kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffClass {
    /// Leader and follower are at the same tip (including both None).
    InSync,
    /// Follower's sequence is ahead of leader's.
    FollowerAhead,
    /// Leader's sequence is ahead of follower's; follower has a non-empty ledger.
    LeaderAhead,
    /// Leader's sequence is ahead of follower's; follower's ledger is empty.
    ///
    /// NOTE: In practice, `classify()` produces `Bootstrap` for this case.
    /// This variant exists for callers who distinguish the two scenarios
    /// explicitly (e.g., observability), but the classify logic below
    /// uses `Bootstrap` when `follower_tip` is `None` and `leader_tip`
    /// exists, consistent with doc 20's pseudocode.
    LeaderAheadEmpty,
    /// Follower's ledger is empty and leader has entries; bootstrap required.
    Bootstrap,
    /// Follower and leader are at the same sequence but hashes differ.
    Divergent,
    /// Insufficient information to classify; fail-closed default.
    Unknown,
}

impl DiffClass {
    /// Returns `true` if this classification allows sync to proceed.
    ///
    /// Only `InSync`, `LeaderAhead`, `LeaderAheadEmpty`, and `Bootstrap`
    /// are "proceed" classifications.
    pub fn is_proceed(&self) -> bool {
        matches!(
            self,
            DiffClass::InSync
                | DiffClass::LeaderAhead
                | DiffClass::LeaderAheadEmpty
                | DiffClass::Bootstrap
        )
    }

    /// Returns `true` if this classification blocks sync.
    ///
    /// `FollowerAhead`, `Divergent`, and `Unknown` are blocking.
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            DiffClass::FollowerAhead | DiffClass::Divergent | DiffClass::Unknown
        )
    }
}

impl std::fmt::Display for DiffClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffClass::InSync => write!(f, "InSync"),
            DiffClass::FollowerAhead => write!(f, "FollowerAhead"),
            DiffClass::LeaderAhead => write!(f, "LeaderAhead"),
            DiffClass::LeaderAheadEmpty => write!(f, "LeaderAheadEmpty"),
            DiffClass::Bootstrap => write!(f, "Bootstrap"),
            DiffClass::Divergent => write!(f, "Divergent"),
            DiffClass::Unknown => write!(f, "Unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Preflight Check Codes (PF1-PF8)
// ---------------------------------------------------------------------------

/// Preflight check codes from Sync-2 doc 20.
///
/// PF1-PF4 are defined in Sync-1 doc 19; PF5-PF8 are added by Sync-2.
/// All checks are local-only queries; no network calls, no mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightCheckCode {
    /// PF1: Local chain passes `verify_chain()`.
    PF1,
    /// PF2: No in-flight commits on local node.
    PF2,
    /// PF3: Leader address/identity known.
    PF3,
    /// PF4: Leader is authorized for sync (capability check).
    PF4,
    /// PF5: Local ledger is readable (can query tip).
    PF5,
    /// PF6: Local ledger has no uncommitted local entries.
    PF6,
    /// PF7: Local node is not currently syncing.
    PF7,
    /// PF8: Leader tip is available (local query; out of scope: how obtained).
    PF8,
}

impl std::fmt::Display for PreflightCheckCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreflightCheckCode::PF1 => write!(f, "PF1"),
            PreflightCheckCode::PF2 => write!(f, "PF2"),
            PreflightCheckCode::PF3 => write!(f, "PF3"),
            PreflightCheckCode::PF4 => write!(f, "PF4"),
            PreflightCheckCode::PF5 => write!(f, "PF5"),
            PreflightCheckCode::PF6 => write!(f, "PF6"),
            PreflightCheckCode::PF7 => write!(f, "PF7"),
            PreflightCheckCode::PF8 => write!(f, "PF8"),
        }
    }
}

/// Result of running preflight checks.
///
/// `Pass` means all PF1-PF8 checks succeeded.
/// `Fail` carries the first check code that failed (fail-fast semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightResult {
    /// All preflight checks passed.
    Pass,
    /// First failing preflight check.
    Fail(PreflightCheckCode),
}

impl PreflightResult {
    /// Returns `true` if all checks passed.
    pub fn is_pass(&self) -> bool {
        matches!(self, PreflightResult::Pass)
    }

    /// Returns `true` if any check failed.
    pub fn is_fail(&self) -> bool {
        matches!(self, PreflightResult::Fail(_))
    }

    /// Returns the failing check code, if any.
    pub fn failed_check(&self) -> Option<PreflightCheckCode> {
        match self {
            PreflightResult::Pass => None,
            PreflightResult::Fail(code) => Some(*code),
        }
    }
}

/// Explicit input for the preflight checker.
///
/// All fields are caller-provided booleans. The checker does not derive or
/// fetch any of them — it is a pure function over explicitly supplied values.
///
/// In production use, the caller would query the local ledger, config, and
/// sync state to populate each field. This groundwork module defines the
/// shape and fail-closed evaluation order; actual query wiring is P3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightInput {
    /// PF1: Does the local chain pass `verify_chain()`?
    pub chain_integrity_ok: bool,
    /// PF2: Are there no in-flight commits on the local node?
    pub no_inflight_commits: bool,
    /// PF3: Is the leader address/identity known?
    pub leader_identity_known: bool,
    /// PF4: Is the leader authorized for sync?
    pub leader_authorized: bool,
    /// PF5: Is the local ledger readable (can query tip)?
    pub ledger_readable: bool,
    /// PF6: Does the local ledger have no uncommitted entries?
    pub no_uncommitted_entries: bool,
    /// PF7: Is the local node NOT currently syncing?
    pub not_currently_syncing: bool,
    /// PF8: Is the leader tip available?
    pub leader_tip_available: bool,
}

impl PreflightInput {
    /// Create a PreflightInput with all checks passing.
    ///
    /// Useful as a baseline for tests; individual fields can be flipped to
    /// `false` to simulate specific preflight failures.
    pub fn all_pass() -> Self {
        Self {
            chain_integrity_ok: true,
            no_inflight_commits: true,
            leader_identity_known: true,
            leader_authorized: true,
            ledger_readable: true,
            no_uncommitted_entries: true,
            not_currently_syncing: true,
            leader_tip_available: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure Functions
// ---------------------------------------------------------------------------

/// Run all preflight checks in order (PF1 through PF8).
///
/// Returns `Pass` if all checks succeed, or `Fail` with the first failing
/// check code. Checks are evaluated in PF1..PF8 order; the first failure
/// short-circuits.
///
/// This is a **pure function**: no side effects, no transport, no mutation.
pub fn run_preflight(input: &PreflightInput) -> PreflightResult {
    if !input.chain_integrity_ok {
        return PreflightResult::Fail(PreflightCheckCode::PF1);
    }
    if !input.no_inflight_commits {
        return PreflightResult::Fail(PreflightCheckCode::PF2);
    }
    if !input.leader_identity_known {
        return PreflightResult::Fail(PreflightCheckCode::PF3);
    }
    if !input.leader_authorized {
        return PreflightResult::Fail(PreflightCheckCode::PF4);
    }
    if !input.ledger_readable {
        return PreflightResult::Fail(PreflightCheckCode::PF5);
    }
    if !input.no_uncommitted_entries {
        return PreflightResult::Fail(PreflightCheckCode::PF6);
    }
    if !input.not_currently_syncing {
        return PreflightResult::Fail(PreflightCheckCode::PF7);
    }
    if !input.leader_tip_available {
        return PreflightResult::Fail(PreflightCheckCode::PF8);
    }
    PreflightResult::Pass
}

/// Classify the relationship between follower and leader ledger tips.
///
/// Given the follower's current tip and the leader's current tip (obtained
/// out-of-scope for Sync-2), returns a `DiffClass` describing their
/// relationship.
///
/// This is a **pure function**: no side effects, no transport, no mutation.
/// It is fail-closed: any input that cannot be reliably classified returns
/// `Unknown`.
///
/// ## Classification Logic (from doc 20)
///
/// ```text
/// follower=None, leader=None           -> InSync
/// follower == leader                   -> InSync
/// follower=Some, leader=None           -> FollowerAhead
/// follower=None, leader=Some           -> Bootstrap
/// follower.seq > leader.seq            -> FollowerAhead
/// leader.seq > follower.seq            -> LeaderAhead
/// same seq, same hash                  -> InSync
/// same seq, different hash             -> Divergent
/// ```
pub fn classify(follower_tip: Option<&TipId>, leader_tip: Option<&TipId>) -> DiffClass {
    // Both empty -> in sync
    if follower_tip.is_none() && leader_tip.is_none() {
        return DiffClass::InSync;
    }

    // Both present and equal -> in sync
    if let (Some(f), Some(l)) = (follower_tip, leader_tip) {
        if f == l {
            return DiffClass::InSync;
        }
    }

    // Follower has entries but leader does not -> follower ahead
    if follower_tip.is_some() && leader_tip.is_none() {
        return DiffClass::FollowerAhead;
    }

    // Leader has entries but follower does not -> bootstrap
    if follower_tip.is_none() && leader_tip.is_some() {
        return DiffClass::Bootstrap;
    }

    // Both have entries: compare sequences
    let follower = match follower_tip {
        Some(t) => t,
        None => return DiffClass::Unknown, // unreachable given above, but fail-closed
    };
    let leader = match leader_tip {
        Some(t) => t,
        None => return DiffClass::Unknown, // unreachable given above, but fail-closed
    };

    if follower.sequence > leader.sequence {
        return DiffClass::FollowerAhead;
    }

    if leader.sequence > follower.sequence {
        return DiffClass::LeaderAhead;
    }

    // Same sequence: compare hashes
    if follower.hash == leader.hash {
        DiffClass::InSync
    } else {
        DiffClass::Divergent
    }
}

/// Map a `DiffClass` to the Sync-1 decision kernel's `Sync1Decision`.
///
/// This bridges Sync-2 output to Sync-1 input, per the decision table in
/// doc 20:
///
/// | DiffClass        | Sync-1 Decision |
/// |------------------|-----------------|
/// | InSync           | DONE            |
/// | FollowerAhead    | ABORT(A4)       |
/// | LeaderAhead      | SYNC            |
/// | LeaderAheadEmpty | FAST_FORWARD    |
/// | Bootstrap        | FAST_FORWARD    |
/// | Divergent        | ABORT(A3)       |
/// | Unknown          | ABORT(A0)       |
///
/// This is a **pure function**: no side effects, no transport, no mutation.
pub fn diff_class_to_decision(class: DiffClass) -> crate::decision::Sync1Decision {
    use crate::decision::Sync1Decision;

    match class {
        DiffClass::InSync => Sync1Decision::Done,
        DiffClass::FollowerAhead => Sync1Decision::Abort(Sync1AbortCode::A4),
        DiffClass::LeaderAhead => Sync1Decision::Sync,
        DiffClass::LeaderAheadEmpty => Sync1Decision::FastForward,
        DiffClass::Bootstrap => Sync1Decision::FastForward,
        DiffClass::Divergent => Sync1Decision::Abort(Sync1AbortCode::A3),
        DiffClass::Unknown => Sync1Decision::Abort(Sync1AbortCode::A0),
    }
}

// ---------------------------------------------------------------------------
// Pure adapter: LocalPreflightState -> PreflightInput
// ---------------------------------------------------------------------------

/// Build a `PreflightInput` from repo-backed state and externally supplied flags.
///
/// This is a **pure function** that converts a `LocalPreflightState` (obtained
/// from `SyncPreflightRepo::read_local_state()`) plus externally supplied
/// PF3/PF4/PF8 flags into the `PreflightInput` struct that `run_preflight()`
/// expects.
///
/// ## PF3 Is Excluded From the Repo Trait
///
/// PF3 (leader identity known) is not a repo query; it comes from transport
/// or config. The caller supplies it via `leader_identity_known`.
///
/// ## PF4/PF8 Can Come From Repo or Externally
///
/// PF4 (leader authorized) and PF8 (leader tip available) are supplied as
/// external booleans here. Callers with a concrete `SyncPreflightRepo`
/// implementation can query `is_leader_authorized()` and `read_leader_tip()`
/// to populate these.
///
/// ## PF1 Is Derived From `chain_verified`
///
/// PF1 (chain integrity) is not in `LocalPreflightState` because it comes
/// from `SyncPreflightRepo::verify_local_chain()`. The caller passes the
/// result as `chain_verified`.
///
/// ## Fail-Closed Note
///
/// If any parameter is `false`, the resulting `PreflightInput` will cause
/// `run_preflight()` to fail on the corresponding check. The caller is
/// responsible for supplying correct values; this adapter does not guess.
pub fn build_preflight_input(
    local_state: &crate::repo::LocalPreflightState,
    chain_verified: bool,
    leader_identity_known: bool,
    leader_authorized: bool,
    leader_tip_available: bool,
) -> PreflightInput {
    PreflightInput {
        // PF1: chain integrity from verify_local_chain()
        chain_integrity_ok: chain_verified,
        // PF2: no in-flight commits (inverted from has_inflight_commits)
        no_inflight_commits: !local_state.has_inflight_commits,
        // PF3: leader identity known (external, not repo)
        leader_identity_known,
        // PF4: leader authorized (external or from is_leader_authorized())
        leader_authorized,
        // PF5: ledger readable. If local_state was obtained successfully,
        // the ledger was readable. If the caller reaches this point,
        // PF5 is implicitly true.
        ledger_readable: true,
        // PF6: no uncommitted entries (inverted from has_uncommitted_entries)
        no_uncommitted_entries: !local_state.has_uncommitted_entries,
        // PF7: not currently syncing (inverted from sync_in_progress)
        not_currently_syncing: !local_state.sync_in_progress,
        // PF8: leader tip available (external or from read_leader_tip())
        leader_tip_available,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::Sync1Decision;

    /// Helper: create a TipId.
    fn tip(seq: u64, hash: &str) -> TipId {
        TipId {
            sequence: seq,
            hash: hash.to_string(),
        }
    }

    // =========================================================================
    // classify() — InSync
    // =========================================================================

    #[test]
    fn classify_insync_both_none() {
        assert_eq!(classify(None, None), DiffClass::InSync);
    }

    #[test]
    fn classify_insync_equal_tips() {
        let t = tip(100, "abc");
        assert_eq!(classify(Some(&t), Some(&t)), DiffClass::InSync);
    }

    #[test]
    fn classify_insync_same_seq_same_hash() {
        let f = tip(50, "hash_x");
        let l = tip(50, "hash_x");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::InSync);
    }

    #[test]
    fn classify_insync_at_sequence_zero() {
        let t = tip(0, "genesis");
        assert_eq!(classify(Some(&t), Some(&t)), DiffClass::InSync);
    }

    // =========================================================================
    // classify() — FollowerAhead
    // =========================================================================

    #[test]
    fn classify_follower_ahead_follower_has_entries_leader_none() {
        let f = tip(10, "h");
        assert_eq!(classify(Some(&f), None), DiffClass::FollowerAhead);
    }

    #[test]
    fn classify_follower_ahead_higher_sequence() {
        let f = tip(200, "h1");
        let l = tip(100, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::FollowerAhead);
    }

    #[test]
    fn classify_follower_ahead_by_one() {
        let f = tip(101, "h1");
        let l = tip(100, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::FollowerAhead);
    }

    // =========================================================================
    // classify() — LeaderAhead
    // =========================================================================

    #[test]
    fn classify_leader_ahead() {
        let f = tip(100, "h1");
        let l = tip(200, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::LeaderAhead);
    }

    #[test]
    fn classify_leader_ahead_by_one() {
        let f = tip(99, "h1");
        let l = tip(100, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::LeaderAhead);
    }

    #[test]
    fn classify_leader_ahead_far() {
        let f = tip(1, "h1");
        let l = tip(1_000_000, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::LeaderAhead);
    }

    // =========================================================================
    // classify() — Bootstrap
    // =========================================================================

    #[test]
    fn classify_bootstrap_follower_none_leader_has_tip() {
        let l = tip(500, "leader_hash");
        assert_eq!(classify(None, Some(&l)), DiffClass::Bootstrap);
    }

    #[test]
    fn classify_bootstrap_at_sequence_one() {
        let l = tip(1, "genesis");
        assert_eq!(classify(None, Some(&l)), DiffClass::Bootstrap);
    }

    // =========================================================================
    // classify() — Divergent
    // =========================================================================

    #[test]
    fn classify_divergent_same_seq_different_hash() {
        let f = tip(100, "hash_alpha");
        let l = tip(100, "hash_beta");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::Divergent);
    }

    #[test]
    fn classify_divergent_at_sequence_zero() {
        let f = tip(0, "genesis_a");
        let l = tip(0, "genesis_b");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::Divergent);
    }

    // =========================================================================
    // DiffClass helpers
    // =========================================================================

    #[test]
    fn diff_class_is_proceed() {
        assert!(DiffClass::InSync.is_proceed());
        assert!(DiffClass::LeaderAhead.is_proceed());
        assert!(DiffClass::LeaderAheadEmpty.is_proceed());
        assert!(DiffClass::Bootstrap.is_proceed());
    }

    #[test]
    fn diff_class_is_block() {
        assert!(DiffClass::FollowerAhead.is_block());
        assert!(DiffClass::Divergent.is_block());
        assert!(DiffClass::Unknown.is_block());
    }

    #[test]
    fn diff_class_proceed_block_are_mutually_exclusive() {
        let all = [
            DiffClass::InSync,
            DiffClass::FollowerAhead,
            DiffClass::LeaderAhead,
            DiffClass::LeaderAheadEmpty,
            DiffClass::Bootstrap,
            DiffClass::Divergent,
            DiffClass::Unknown,
        ];
        for c in &all {
            assert_ne!(
                c.is_proceed(),
                c.is_block(),
                "{c} cannot be both proceed and block"
            );
        }
    }

    #[test]
    fn diff_class_display() {
        assert_eq!(format!("{}", DiffClass::InSync), "InSync");
        assert_eq!(format!("{}", DiffClass::FollowerAhead), "FollowerAhead");
        assert_eq!(format!("{}", DiffClass::LeaderAhead), "LeaderAhead");
        assert_eq!(
            format!("{}", DiffClass::LeaderAheadEmpty),
            "LeaderAheadEmpty"
        );
        assert_eq!(format!("{}", DiffClass::Bootstrap), "Bootstrap");
        assert_eq!(format!("{}", DiffClass::Divergent), "Divergent");
        assert_eq!(format!("{}", DiffClass::Unknown), "Unknown");
    }

    // =========================================================================
    // run_preflight()
    // =========================================================================

    #[test]
    fn preflight_all_pass() {
        let input = PreflightInput::all_pass();
        assert_eq!(run_preflight(&input), PreflightResult::Pass);
    }

    #[test]
    fn preflight_pf1_fails() {
        let mut input = PreflightInput::all_pass();
        input.chain_integrity_ok = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF1)
        );
    }

    #[test]
    fn preflight_pf2_fails() {
        let mut input = PreflightInput::all_pass();
        input.no_inflight_commits = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF2)
        );
    }

    #[test]
    fn preflight_pf3_fails() {
        let mut input = PreflightInput::all_pass();
        input.leader_identity_known = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF3)
        );
    }

    #[test]
    fn preflight_pf4_fails() {
        let mut input = PreflightInput::all_pass();
        input.leader_authorized = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF4)
        );
    }

    #[test]
    fn preflight_pf5_fails() {
        let mut input = PreflightInput::all_pass();
        input.ledger_readable = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF5)
        );
    }

    #[test]
    fn preflight_pf6_fails() {
        let mut input = PreflightInput::all_pass();
        input.no_uncommitted_entries = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF6)
        );
    }

    #[test]
    fn preflight_pf7_fails() {
        let mut input = PreflightInput::all_pass();
        input.not_currently_syncing = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF7)
        );
    }

    #[test]
    fn preflight_pf8_fails() {
        let mut input = PreflightInput::all_pass();
        input.leader_tip_available = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF8)
        );
    }

    #[test]
    fn preflight_first_failure_wins() {
        // If PF1 and PF3 both fail, PF1 is reported (first-fail semantics)
        let mut input = PreflightInput::all_pass();
        input.chain_integrity_ok = false;
        input.leader_identity_known = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF1)
        );
    }

    #[test]
    fn preflight_all_fail_reports_pf1() {
        let input = PreflightInput {
            chain_integrity_ok: false,
            no_inflight_commits: false,
            leader_identity_known: false,
            leader_authorized: false,
            ledger_readable: false,
            no_uncommitted_entries: false,
            not_currently_syncing: false,
            leader_tip_available: false,
        };
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF1)
        );
    }

    // =========================================================================
    // PreflightResult helpers
    // =========================================================================

    #[test]
    fn preflight_result_is_pass() {
        assert!(PreflightResult::Pass.is_pass());
        assert!(!PreflightResult::Fail(PreflightCheckCode::PF1).is_pass());
    }

    #[test]
    fn preflight_result_is_fail() {
        assert!(PreflightResult::Fail(PreflightCheckCode::PF1).is_fail());
        assert!(!PreflightResult::Pass.is_fail());
    }

    #[test]
    fn preflight_result_failed_check() {
        assert_eq!(PreflightResult::Pass.failed_check(), None);
        assert_eq!(
            PreflightResult::Fail(PreflightCheckCode::PF3).failed_check(),
            Some(PreflightCheckCode::PF3)
        );
    }

    // =========================================================================
    // PreflightCheckCode display
    // =========================================================================

    #[test]
    fn preflight_check_code_display() {
        assert_eq!(format!("{}", PreflightCheckCode::PF1), "PF1");
        assert_eq!(format!("{}", PreflightCheckCode::PF8), "PF8");
    }

    // =========================================================================
    // diff_class_to_decision() — bridge to Sync-1 decision kernel
    // =========================================================================

    #[test]
    fn bridge_insync_to_done() {
        assert_eq!(
            diff_class_to_decision(DiffClass::InSync),
            Sync1Decision::Done
        );
    }

    #[test]
    fn bridge_follower_ahead_to_abort_a4() {
        let d = diff_class_to_decision(DiffClass::FollowerAhead);
        assert!(d.is_abort());
        assert_eq!(d.abort_code(), Some(Sync1AbortCode::A4));
    }

    #[test]
    fn bridge_leader_ahead_to_sync() {
        assert_eq!(
            diff_class_to_decision(DiffClass::LeaderAhead),
            Sync1Decision::Sync
        );
    }

    #[test]
    fn bridge_leader_ahead_empty_to_fast_forward() {
        assert_eq!(
            diff_class_to_decision(DiffClass::LeaderAheadEmpty),
            Sync1Decision::FastForward
        );
    }

    #[test]
    fn bridge_bootstrap_to_fast_forward() {
        assert_eq!(
            diff_class_to_decision(DiffClass::Bootstrap),
            Sync1Decision::FastForward
        );
    }

    #[test]
    fn bridge_divergent_to_abort_a3() {
        let d = diff_class_to_decision(DiffClass::Divergent);
        assert!(d.is_abort());
        assert_eq!(d.abort_code(), Some(Sync1AbortCode::A3));
    }

    #[test]
    fn bridge_unknown_to_abort_a0() {
        let d = diff_class_to_decision(DiffClass::Unknown);
        assert!(d.is_abort());
        assert_eq!(d.abort_code(), Some(Sync1AbortCode::A0));
    }

    // =========================================================================
    // Exhaustive: every DiffClass variant is reachable by classify()
    // =========================================================================

    #[test]
    fn every_diff_class_produced_by_classify() {
        let mut seen = std::collections::HashSet::new();

        // InSync: both None
        seen.insert(classify(None, None));

        // InSync: equal tips
        let t = tip(10, "h");
        seen.insert(classify(Some(&t), Some(&t)));

        // FollowerAhead: follower has entries, leader None
        let f = tip(10, "h");
        seen.insert(classify(Some(&f), None));

        // FollowerAhead: follower seq > leader seq
        let f2 = tip(20, "h");
        let l2 = tip(10, "h");
        seen.insert(classify(Some(&f2), Some(&l2)));

        // LeaderAhead: leader seq > follower seq
        let f3 = tip(10, "h");
        let l3 = tip(20, "h");
        seen.insert(classify(Some(&f3), Some(&l3)));

        // Bootstrap: follower None, leader has entries
        let l4 = tip(100, "h");
        seen.insert(classify(None, Some(&l4)));

        // Divergent: same seq, different hash
        let f5 = tip(50, "alpha");
        let l5 = tip(50, "beta");
        seen.insert(classify(Some(&f5), Some(&l5)));

        // Verify all variants except LeaderAheadEmpty and Unknown are produced
        // (LeaderAheadEmpty is an explicit alias; Unknown is a catch-all for callers)
        assert!(seen.contains(&DiffClass::InSync), "InSync must be produced");
        assert!(
            seen.contains(&DiffClass::FollowerAhead),
            "FollowerAhead must be produced"
        );
        assert!(
            seen.contains(&DiffClass::LeaderAhead),
            "LeaderAhead must be produced"
        );
        assert!(
            seen.contains(&DiffClass::Bootstrap),
            "Bootstrap must be produced"
        );
        assert!(
            seen.contains(&DiffClass::Divergent),
            "Divergent must be produced"
        );
    }

    // =========================================================================
    // Roundtrip: classify() -> diff_class_to_decision() consistency
    // =========================================================================

    #[test]
    fn roundtrip_classify_then_decide_matches_decision_kernel_for_equal_tips() {
        // classify(InSync) -> diff_class_to_decision -> Done
        // This should match decide() for the same tips
        let t = tip(100, "abc");
        let dc = classify(Some(&t), Some(&t));
        assert_eq!(dc, DiffClass::InSync);
        let d1 = diff_class_to_decision(dc);
        let d2 = crate::decision::decide(&crate::decision::DecisionInput {
            follower_tip: Some(t.clone()),
            leader_tip: Some(t),
            hash_path_valid: true,
        });
        assert_eq!(
            d1, d2,
            "Sync-2 classify->bridge must agree with Sync-1 decide for equal tips"
        );
    }

    #[test]
    fn roundtrip_classify_then_decide_matches_decision_kernel_for_leader_ahead() {
        let f = tip(50, "h1");
        let l = tip(100, "h2");
        let dc = classify(Some(&f), Some(&l));
        assert_eq!(dc, DiffClass::LeaderAhead);
        let d1 = diff_class_to_decision(dc);
        // Sync-1 decide with hash_path_valid=true should give SYNC
        let d2 = crate::decision::decide(&crate::decision::DecisionInput {
            follower_tip: Some(f),
            leader_tip: Some(l),
            hash_path_valid: true,
        });
        assert_eq!(
            d1, d2,
            "Sync-2 classify->bridge must agree with Sync-1 decide for leader ahead"
        );
    }

    #[test]
    fn roundtrip_classify_then_decide_matches_for_divergent() {
        let f = tip(100, "alpha");
        let l = tip(100, "beta");
        let dc = classify(Some(&f), Some(&l));
        assert_eq!(dc, DiffClass::Divergent);
        let d1 = diff_class_to_decision(dc);
        // Sync-1 decide sees same seq, different hash -> Abort(A3)
        let d2 = crate::decision::decide(&crate::decision::DecisionInput {
            follower_tip: Some(f),
            leader_tip: Some(l),
            hash_path_valid: false,
        });
        assert_eq!(
            d1, d2,
            "Sync-2 classify->bridge must agree with Sync-1 decide for divergent"
        );
    }

    #[test]
    fn roundtrip_classify_then_decide_matches_for_bootstrap() {
        let l = tip(500, "leader_hash");
        let dc = classify(None, Some(&l));
        assert_eq!(dc, DiffClass::Bootstrap);
        let d1 = diff_class_to_decision(dc);
        let d2 = crate::decision::decide(&crate::decision::DecisionInput {
            follower_tip: None,
            leader_tip: Some(l),
            hash_path_valid: false,
        });
        assert_eq!(
            d1, d2,
            "Sync-2 classify->bridge must agree with Sync-1 decide for bootstrap"
        );
    }

    #[test]
    fn roundtrip_classify_then_decide_matches_for_follower_ahead() {
        let f = tip(200, "h1");
        let l = tip(100, "h2");
        let dc = classify(Some(&f), Some(&l));
        assert_eq!(dc, DiffClass::FollowerAhead);
        let d1 = diff_class_to_decision(dc);
        let d2 = crate::decision::decide(&crate::decision::DecisionInput {
            follower_tip: Some(f),
            leader_tip: Some(l),
            hash_path_valid: false,
        });
        assert_eq!(
            d1, d2,
            "Sync-2 classify->bridge must agree with Sync-1 decide for follower ahead"
        );
    }

    // =========================================================================
    // build_preflight_input() — pure adapter
    // =========================================================================

    #[test]
    fn build_preflight_input_all_passing() {
        let state = crate::repo::LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, true, true, true, true);
        assert_eq!(input, PreflightInput::all_pass());
        assert_eq!(run_preflight(&input), PreflightResult::Pass);
    }

    #[test]
    fn build_preflight_input_chain_not_verified_fails_pf1() {
        let state = crate::repo::LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, false, true, true, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF1)
        );
    }

    #[test]
    fn build_preflight_input_inflight_commits_fails_pf2() {
        let state = crate::repo::LocalPreflightState {
            has_inflight_commits: true,
            ..crate::repo::LocalPreflightState::clean_empty()
        };
        let input = build_preflight_input(&state, true, true, true, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF2)
        );
    }

    #[test]
    fn build_preflight_input_leader_identity_unknown_fails_pf3() {
        let state = crate::repo::LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, true, false, true, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF3)
        );
    }

    #[test]
    fn build_preflight_input_leader_not_authorized_fails_pf4() {
        let state = crate::repo::LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, true, true, false, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF4)
        );
    }

    #[test]
    fn build_preflight_input_uncommitted_entries_fails_pf6() {
        let state = crate::repo::LocalPreflightState {
            has_uncommitted_entries: true,
            ..crate::repo::LocalPreflightState::clean_empty()
        };
        let input = build_preflight_input(&state, true, true, true, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF6)
        );
    }

    #[test]
    fn build_preflight_input_sync_in_progress_fails_pf7() {
        let state = crate::repo::LocalPreflightState {
            sync_in_progress: true,
            ..crate::repo::LocalPreflightState::clean_empty()
        };
        let input = build_preflight_input(&state, true, true, true, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF7)
        );
    }

    #[test]
    fn build_preflight_input_no_leader_tip_fails_pf8() {
        let state = crate::repo::LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, true, true, true, false);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF8)
        );
    }

    #[test]
    fn build_preflight_input_first_failure_wins() {
        // chain not verified + uncommitted entries -> PF1 wins (first-fail)
        let state = crate::repo::LocalPreflightState {
            has_uncommitted_entries: true,
            ..crate::repo::LocalPreflightState::clean_empty()
        };
        let input = build_preflight_input(&state, false, true, true, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF1)
        );
    }

    #[test]
    fn build_preflight_input_with_tip_preserves_ledger_readable() {
        // ledger_readable is always true when build_preflight_input is called
        // (because reaching it implies the repo query succeeded)
        let t = tip(100, "abc");
        let state = crate::repo::LocalPreflightState::clean_with_tip(t);
        let input = build_preflight_input(&state, true, true, true, true);
        assert!(input.ledger_readable);
        assert_eq!(run_preflight(&input), PreflightResult::Pass);
    }
}
