//! Sync-2 Read-Only Preflight + Diff Classifier.
//!
//! This module implements the read-only preflight checks (PF1-PF8) and diff
//! classifier. It provides:
//!
//! - A pure `DiffClass` classifier that maps two ledger tips to a classification
//! - A pure `run_preflight()` function that checks PF1-PF8 preconditions
//! - A bridging function `diff_class_to_decision()` that maps `DiffClass` to
//!   the Sync-1 decision kernel's `Sync1Decision`
//!
//! ## Fail-Closed Guarantees
//!
//! - `classify()` returns `Unknown` on any ambiguity (never guesses)
//! - `run_preflight()` returns `Fail` on the first failing check
//! - `diff_class_to_decision()` maps `Unknown` to `Abort(A0)`
//! - No network calls, no repo queries, no mutation

use crate::decision::{Sync1Decision, TipId};
use crate::error::Sync1AbortCode;
use crate::repo::LocalPreflightState;

// ---------------------------------------------------------------------------
// DiffClass — read-only diff classifier output
// ---------------------------------------------------------------------------

/// Classification of the relationship between follower and leader ledger tips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffClass {
    /// Leader and follower are at the same tip (including both None).
    InSync,
    /// Follower's sequence is ahead of leader's.
    FollowerAhead,
    /// Leader's sequence is ahead of follower's; follower has a non-empty ledger.
    LeaderAhead,
    /// Follower's ledger is empty and leader has entries; bootstrap required.
    Bootstrap,
    /// Follower and leader are at the same sequence but hashes differ.
    Divergent,
    /// Insufficient information to classify; fail-closed default.
    Unknown,
}

impl DiffClass {
    /// Returns `true` if this classification allows sync to proceed.
    pub fn is_proceed(&self) -> bool {
        matches!(
            self,
            DiffClass::InSync | DiffClass::LeaderAhead | DiffClass::Bootstrap
        )
    }

    /// Returns `true` if this classification blocks sync.
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
            DiffClass::Bootstrap => write!(f, "Bootstrap"),
            DiffClass::Divergent => write!(f, "Divergent"),
            DiffClass::Unknown => write!(f, "Unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Preflight Check Codes (PF1-PF8)
// ---------------------------------------------------------------------------

/// Preflight check codes from Sync-2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightCheckCode {
    PF1,
    PF2,
    PF3,
    PF4,
    PF5,
    PF6,
    PF7,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightResult {
    Pass,
    Fail(PreflightCheckCode),
}

impl PreflightResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, PreflightResult::Pass)
    }

    pub fn is_fail(&self) -> bool {
        matches!(self, PreflightResult::Fail(_))
    }

    pub fn failed_check(&self) -> Option<PreflightCheckCode> {
        match self {
            PreflightResult::Pass => None,
            PreflightResult::Fail(code) => Some(*code),
        }
    }
}

/// Explicit input for the preflight checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightInput {
    pub chain_integrity_ok: bool,
    pub no_inflight_commits: bool,
    pub leader_identity_known: bool,
    pub leader_authorized: bool,
    pub ledger_readable: bool,
    pub no_uncommitted_entries: bool,
    pub not_currently_syncing: bool,
    pub leader_tip_available: bool,
}

impl PreflightInput {
    /// Create a `PreflightInput` with all checks passing.
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
/// check code.
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
        None => return DiffClass::Unknown,
    };
    let leader = match leader_tip {
        Some(t) => t,
        None => return DiffClass::Unknown,
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
pub fn diff_class_to_decision(class: DiffClass) -> Sync1Decision {
    match class {
        DiffClass::InSync => Sync1Decision::Done,
        DiffClass::FollowerAhead => Sync1Decision::Abort(Sync1AbortCode::A4),
        DiffClass::LeaderAhead => Sync1Decision::Sync,
        DiffClass::Bootstrap => Sync1Decision::FastForward,
        DiffClass::Divergent => Sync1Decision::Abort(Sync1AbortCode::A3),
        DiffClass::Unknown => Sync1Decision::Abort(Sync1AbortCode::A0),
    }
}

/// Build a `PreflightInput` from repo-backed state and externally supplied flags.
pub fn build_preflight_input(
    local_state: &LocalPreflightState,
    chain_verified: bool,
    leader_identity_known: bool,
    leader_authorized: bool,
    leader_tip_available: bool,
) -> PreflightInput {
    PreflightInput {
        chain_integrity_ok: chain_verified,
        no_inflight_commits: !local_state.has_inflight_commits,
        leader_identity_known,
        leader_authorized,
        ledger_readable: true,
        no_uncommitted_entries: !local_state.has_uncommitted_entries,
        not_currently_syncing: !local_state.sync_in_progress,
        leader_tip_available,
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

    // classify
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
    fn classify_follower_ahead() {
        let f = tip(200, "h1");
        let l = tip(100, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::FollowerAhead);
    }

    #[test]
    fn classify_leader_ahead() {
        let f = tip(100, "h1");
        let l = tip(200, "h2");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::LeaderAhead);
    }

    #[test]
    fn classify_bootstrap() {
        let l = tip(500, "leader_hash");
        assert_eq!(classify(None, Some(&l)), DiffClass::Bootstrap);
    }

    #[test]
    fn classify_divergent() {
        let f = tip(100, "hash_alpha");
        let l = tip(100, "hash_beta");
        assert_eq!(classify(Some(&f), Some(&l)), DiffClass::Divergent);
    }

    // run_preflight
    #[test]
    fn preflight_all_pass() {
        let input = PreflightInput::all_pass();
        assert_eq!(run_preflight(&input), PreflightResult::Pass);
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
    fn preflight_pf8_fails() {
        let mut input = PreflightInput::all_pass();
        input.leader_tip_available = false;
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF8)
        );
    }

    // diff_class_to_decision
    #[test]
    fn bridge_insync_to_done() {
        assert_eq!(
            diff_class_to_decision(DiffClass::InSync),
            Sync1Decision::Done
        );
    }

    #[test]
    fn bridge_leader_ahead_to_sync() {
        assert_eq!(
            diff_class_to_decision(DiffClass::LeaderAhead),
            Sync1Decision::Sync
        );
    }

    #[test]
    fn bridge_follower_ahead_to_abort_a4() {
        let d = diff_class_to_decision(DiffClass::FollowerAhead);
        assert!(d.is_abort());
        assert_eq!(d.abort_code(), Some(Sync1AbortCode::A4));
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
        assert_eq!(d.abort_code(), Some(Sync1AbortCode::A3));
    }

    // build_preflight_input
    #[test]
    fn build_preflight_input_all_passing() {
        let state = LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, true, true, true, true);
        assert_eq!(input, PreflightInput::all_pass());
        assert_eq!(run_preflight(&input), PreflightResult::Pass);
    }

    #[test]
    fn build_preflight_input_leader_not_authorized_fails_pf4() {
        let state = LocalPreflightState::clean_empty();
        let input = build_preflight_input(&state, true, true, false, true);
        assert_eq!(
            run_preflight(&input),
            PreflightResult::Fail(PreflightCheckCode::PF4)
        );
    }
}
