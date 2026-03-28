//! Sync-1 Decision Kernel — pure, read-only decision logic for one-way fast-forward sync.
//!
//! This module implements the decision table from `docs/implementation-path/19-sync-1-protocol-sketch.md`
//! Phase 2. It is a **pure function** with no transport, network, or mutation side effects.
//!
//! ## Decision Table
//!
//! | Condition                                            | Decision                   |
//! |------------------------------------------------------|----------------------------|
//! | leader_tip == follower_tip                           | DONE (no sync needed)      |
//! | leader_tip < follower_tip                            | ABORT A4 (follower ahead)  |
//! | leader_tip > follower_tip AND hash_path valid        | SYNC (fetch N+1..M)        |
//! | leader_tip > follower_tip AND hash_path INVALID      | ABORT A3 (C2 violation)    |
//! | follower_tip is NONE AND leader_tip exists            | FAST_FORWARD (bootstrap)   |
//!
//! ## Fail-Closed Semantics
//!
//! Any ambiguous or unexpected input state results in an abort. The kernel never
//! returns a "proceed" decision when it cannot confirm a safe path.

use crate::error::Sync1AbortCode;

/// A tip identity: sequence number + hash.
///
/// This is a lightweight, transport-independent representation of a ledger tip.
/// It deliberately mirrors `transport::LeaderTip` fields but is independent of
/// that type so the decision kernel has zero transport coupling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipId {
    /// Sequence number of the tip entry.
    pub sequence: u64,
    /// Hash of the tip entry.
    pub hash: String,
}

/// Input to the Sync-1 decision kernel.
///
/// All fields are explicit, caller-provided values. The kernel does not
/// derive or fetch any of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionInput {
    /// The follower's current tip. `None` means the follower ledger is empty.
    pub follower_tip: Option<TipId>,
    /// The leader's current tip. `None` means the leader could not provide a tip,
    /// which triggers a fail-closed abort (A7).
    pub leader_tip: Option<TipId>,
    /// Whether the hash-path continuity check passed between follower tip and leader tip.
    /// Only meaningful when both tips exist and leader is ahead.
    pub hash_path_valid: bool,
}

/// Outcome of the Sync-1 decision kernel.
///
/// Each variant corresponds to exactly one row in the Sync-1 decision table.
/// `Abort` carries the specific `Sync1AbortCode` for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sync1Decision {
    /// Leader and follower are at the same tip — no sync needed.
    Done,
    /// Sync should proceed: fetch entries follower_tip+1 .. leader_tip.
    Sync,
    /// Follower has no ledger — bootstrap from genesis to leader_tip.
    FastForward,
    /// Unsafe to proceed — abort with the given code. Local chain unchanged.
    Abort(Sync1AbortCode),
}

impl Sync1Decision {
    /// Returns `true` if this decision allows progress (Done, Sync, or FastForward).
    pub fn is_proceed(&self) -> bool {
        !matches!(self, Sync1Decision::Abort(_))
    }

    /// Returns `true` if this decision is an abort.
    pub fn is_abort(&self) -> bool {
        matches!(self, Sync1Decision::Abort(_))
    }

    /// Returns the abort code if this is an abort decision.
    pub fn abort_code(&self) -> Option<Sync1AbortCode> {
        match self {
            Sync1Decision::Abort(code) => Some(*code),
            _ => None,
        }
    }
}

impl std::fmt::Display for Sync1Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sync1Decision::Done => write!(f, "DONE"),
            Sync1Decision::Sync => write!(f, "SYNC"),
            Sync1Decision::FastForward => write!(f, "FAST_FORWARD"),
            Sync1Decision::Abort(code) => write!(f, "ABORT({})", code),
        }
    }
}

/// Evaluate the Sync-1 decision table.
///
/// This is a **pure function**: no side effects, no transport calls, no mutation.
/// Given the current follower state and leader state, it returns the correct
/// Sync-1 decision per the fail-closed decision table.
///
/// ## Fail-Closed Guarantees
///
/// - If `leader_tip` is `None`, returns `Abort(A7)` — cannot sync without a leader.
/// - If `follower_tip` is `Some` but sequences are inconsistent (e.g., same sequence
///   but different hashes), returns `Abort(A3)` — potential C2 violation.
/// - Any unexpected state falls through to `Abort(A0)` — unknown/preflight failure.
pub fn decide(input: &DecisionInput) -> Sync1Decision {
    // A leader tip is required for any sync decision.
    let leader = match &input.leader_tip {
        Some(tip) => tip,
        None => return Sync1Decision::Abort(Sync1AbortCode::A7),
    };

    match &input.follower_tip {
        None => {
            // Follower ledger is empty — bootstrap via fast-forward.
            // Decision: FAST_FORWARD (fetch entries 0..M, apply genesis + all)
            Sync1Decision::FastForward
        }
        Some(follower) => {
            // Both tips present — apply the decision table.

            // Row: leader_tip == follower_tip -> DONE
            if leader.sequence == follower.sequence {
                if leader.hash == follower.hash {
                    return Sync1Decision::Done;
                } else {
                    // Same sequence, different hash — C2 violation (hash continuity broken).
                    return Sync1Decision::Abort(Sync1AbortCode::A3);
                }
            }

            // Row: leader_tip < follower_tip -> ABORT A4 (follower ahead)
            if leader.sequence < follower.sequence {
                return Sync1Decision::Abort(Sync1AbortCode::A4);
            }

            // leader.sequence > follower.sequence — leader is ahead.

            if input.hash_path_valid {
                // Row: hash_path valid -> SYNC
                Sync1Decision::Sync
            } else {
                // Row: hash_path INVALID -> ABORT A3 (C2 violation)
                Sync1Decision::Abort(Sync1AbortCode::A3)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a TipId.
    fn tip(seq: u64, hash: &str) -> TipId {
        TipId {
            sequence: seq,
            hash: hash.to_string(),
        }
    }

    /// Helper: create a DecisionInput with both tips present.
    fn both_tips(
        follower_seq: u64,
        follower_hash: &str,
        leader_seq: u64,
        leader_hash: &str,
        hash_path_valid: bool,
    ) -> DecisionInput {
        DecisionInput {
            follower_tip: Some(tip(follower_seq, follower_hash)),
            leader_tip: Some(tip(leader_seq, leader_hash)),
            hash_path_valid,
        }
    }

    // =========================================================================
    // DONE decision
    // =========================================================================

    #[test]
    fn done_when_tips_equal() {
        let input = both_tips(100, "hash_a", 100, "hash_a", false);
        assert_eq!(decide(&input), Sync1Decision::Done);
    }

    #[test]
    fn done_when_tips_equal_hash_path_irrelevant() {
        // hash_path_valid doesn't matter when tips are equal
        let input = both_tips(50, "h", 50, "h", true);
        assert_eq!(decide(&input), Sync1Decision::Done);
    }

    #[test]
    fn done_at_sequence_zero() {
        let input = both_tips(0, "genesis", 0, "genesis", false);
        assert_eq!(decide(&input), Sync1Decision::Done);
    }

    // =========================================================================
    // ABORT A4 — follower ahead
    // =========================================================================

    #[test]
    fn abort_a4_when_follower_ahead() {
        let input = both_tips(200, "h1", 100, "h2", false);
        let decision = decide(&input);
        assert!(decision.is_abort());
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A4));
    }

    #[test]
    fn abort_a4_when_follower_exactly_one_ahead() {
        let input = both_tips(101, "h1", 100, "h2", false);
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A4));
    }

    #[test]
    fn abort_a4_when_follower_far_ahead() {
        let input = both_tips(10000, "h1", 100, "h2", false);
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A4));
    }

    // =========================================================================
    // SYNC decision — leader ahead, hash_path valid
    // =========================================================================

    #[test]
    fn sync_when_leader_ahead_hash_path_valid() {
        let input = both_tips(100, "h1", 200, "h2", true);
        assert_eq!(decide(&input), Sync1Decision::Sync);
    }

    #[test]
    fn sync_when_leader_one_ahead() {
        let input = both_tips(99, "h1", 100, "h2", true);
        assert_eq!(decide(&input), Sync1Decision::Sync);
    }

    #[test]
    fn sync_when_leader_far_ahead() {
        let input = both_tips(1, "h1", 1000000, "h2", true);
        assert_eq!(decide(&input), Sync1Decision::Sync);
    }

    // =========================================================================
    // ABORT A3 — leader ahead, hash_path INVALID (C2 violation)
    // =========================================================================

    #[test]
    fn abort_a3_when_leader_ahead_hash_path_invalid() {
        let input = both_tips(100, "h1", 200, "h2", false);
        let decision = decide(&input);
        assert!(decision.is_abort());
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A3));
    }

    #[test]
    fn abort_a3_when_leader_one_ahead_hash_path_invalid() {
        let input = both_tips(99, "h1", 100, "h2", false);
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A3));
    }

    // =========================================================================
    // ABORT A3 — same sequence, different hash (C2 divergence)
    // =========================================================================

    #[test]
    fn abort_a3_when_same_sequence_different_hash() {
        // Both at seq=100 but hashes differ -> C2 violation
        let input = both_tips(100, "hash_alpha", 100, "hash_beta", false);
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A3));
    }

    #[test]
    fn abort_a3_when_same_sequence_different_hash_even_with_valid_path() {
        // hash_path_valid=true doesn't save us if sequences are equal but hashes differ
        let input = both_tips(100, "hash_alpha", 100, "hash_beta", true);
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A3));
    }

    // =========================================================================
    // FAST_FORWARD — follower empty, leader has tip
    // =========================================================================

    #[test]
    fn fast_forward_when_follower_empty() {
        let input = DecisionInput {
            follower_tip: None,
            leader_tip: Some(tip(500, "leader_hash")),
            hash_path_valid: false,
        };
        assert_eq!(decide(&input), Sync1Decision::FastForward);
    }

    #[test]
    fn fast_forward_when_follower_empty_hash_path_irrelevant() {
        // hash_path_valid doesn't matter when follower is empty
        let input = DecisionInput {
            follower_tip: None,
            leader_tip: Some(tip(500, "leader_hash")),
            hash_path_valid: true,
        };
        assert_eq!(decide(&input), Sync1Decision::FastForward);
    }

    #[test]
    fn fast_forward_when_leader_at_sequence_one() {
        let input = DecisionInput {
            follower_tip: None,
            leader_tip: Some(tip(1, "genesis")),
            hash_path_valid: false,
        };
        assert_eq!(decide(&input), Sync1Decision::FastForward);
    }

    // =========================================================================
    // ABORT A7 — leader tip missing
    // =========================================================================

    #[test]
    fn abort_a7_when_leader_tip_missing_follower_empty() {
        let input = DecisionInput {
            follower_tip: None,
            leader_tip: None,
            hash_path_valid: false,
        };
        let decision = decide(&input);
        assert!(decision.is_abort());
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A7));
    }

    #[test]
    fn abort_a7_when_leader_tip_missing_follower_has_tip() {
        let input = DecisionInput {
            follower_tip: Some(tip(100, "h")),
            leader_tip: None,
            hash_path_valid: false,
        };
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A7));
    }

    #[test]
    fn abort_a7_when_leader_tip_missing_hash_path_claimed_valid() {
        // Even if hash_path_valid=true, no leader tip means abort
        let input = DecisionInput {
            follower_tip: Some(tip(100, "h")),
            leader_tip: None,
            hash_path_valid: true,
        };
        let decision = decide(&input);
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A7));
    }

    // =========================================================================
    // Decision helpers
    // =========================================================================

    #[test]
    fn is_proceed_true_for_done() {
        assert!(Sync1Decision::Done.is_proceed());
    }

    #[test]
    fn is_proceed_true_for_sync() {
        assert!(Sync1Decision::Sync.is_proceed());
    }

    #[test]
    fn is_proceed_true_for_fast_forward() {
        assert!(Sync1Decision::FastForward.is_proceed());
    }

    #[test]
    fn is_proceed_false_for_abort() {
        assert!(!Sync1Decision::Abort(Sync1AbortCode::A0).is_proceed());
    }

    #[test]
    fn is_abort_true_for_abort() {
        assert!(Sync1Decision::Abort(Sync1AbortCode::A3).is_abort());
    }

    #[test]
    fn is_abort_false_for_proceed_decisions() {
        assert!(!Sync1Decision::Done.is_abort());
        assert!(!Sync1Decision::Sync.is_abort());
        assert!(!Sync1Decision::FastForward.is_abort());
    }

    #[test]
    fn abort_code_returns_code_for_abort() {
        assert_eq!(
            Sync1Decision::Abort(Sync1AbortCode::A4).abort_code(),
            Some(Sync1AbortCode::A4)
        );
    }

    #[test]
    fn abort_code_returns_none_for_proceed_decisions() {
        assert_eq!(Sync1Decision::Done.abort_code(), None);
        assert_eq!(Sync1Decision::Sync.abort_code(), None);
        assert_eq!(Sync1Decision::FastForward.abort_code(), None);
    }

    // =========================================================================
    // Display
    // =========================================================================

    #[test]
    fn display_formats() {
        assert_eq!(format!("{}", Sync1Decision::Done), "DONE");
        assert_eq!(format!("{}", Sync1Decision::Sync), "SYNC");
        assert_eq!(format!("{}", Sync1Decision::FastForward), "FAST_FORWARD");
        assert_eq!(
            format!("{}", Sync1Decision::Abort(Sync1AbortCode::A3)),
            "ABORT(A3)"
        );
    }

    // =========================================================================
    // Exhaustive boundary / edge cases
    // =========================================================================

    #[test]
    fn boundary_sequence_zero_sync() {
        // Follower at 0, leader at 1 — minimal sync
        let input = both_tips(0, "h0", 1, "h1", true);
        assert_eq!(decide(&input), Sync1Decision::Sync);
    }

    #[test]
    fn boundary_sequence_zero_abort_a3() {
        // Follower at 0, leader at 1, hash_path invalid
        let input = both_tips(0, "h0", 1, "h1", false);
        assert_eq!(decide(&input).abort_code(), Some(Sync1AbortCode::A3));
    }

    #[test]
    fn all_abort_codes_surfaceable() {
        // Verify each relevant abort code can be produced by the kernel
        let a3_diverge = both_tips(100, "a", 100, "b", false);
        assert_eq!(decide(&a3_diverge).abort_code(), Some(Sync1AbortCode::A3));

        let a3_invalid_path = both_tips(100, "a", 200, "b", false);
        assert_eq!(
            decide(&a3_invalid_path).abort_code(),
            Some(Sync1AbortCode::A3)
        );

        let a4 = both_tips(200, "a", 100, "b", false);
        assert_eq!(decide(&a4).abort_code(), Some(Sync1AbortCode::A4));

        let a7_no_leader = DecisionInput {
            follower_tip: None,
            leader_tip: None,
            hash_path_valid: false,
        };
        assert_eq!(decide(&a7_no_leader).abort_code(), Some(Sync1AbortCode::A7));
    }

    #[test]
    fn every_decision_variant_reachable() {
        // Done
        let done = both_tips(10, "h", 10, "h", false);
        assert_eq!(decide(&done), Sync1Decision::Done);

        // Sync
        let sync = both_tips(10, "h", 20, "h2", true);
        assert_eq!(decide(&sync), Sync1Decision::Sync);

        // FastForward
        let ff = DecisionInput {
            follower_tip: None,
            leader_tip: Some(tip(10, "h")),
            hash_path_valid: false,
        };
        assert_eq!(decide(&ff), Sync1Decision::FastForward);

        // Abort
        let abort = DecisionInput {
            follower_tip: Some(tip(10, "h")),
            leader_tip: None,
            hash_path_valid: false,
        };
        assert!(decide(&abort).is_abort());
    }
}
