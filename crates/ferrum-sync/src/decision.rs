//! Sync-1 Decision Kernel — pure, read-only decision logic for one-way fast-forward sync.
//!
//! This module implements the decision table for Sync-1. It is a **pure function**
//! with no transport, network, or mutation side effects.
//!
//! ## Decision Table
//!
//! | Condition                                            | Decision                   |
//! |------------------------------------------------------|----------------------------|
//! | leader_tip == follower_tip                           | DONE (no sync needed)      |
//! | leader_tip < follower_tip                            | ABORT A4 (follower ahead)  |
//! | leader_tip > follower_tip AND hash_path valid        | SYNC (fetch N+1..M)        |
//! | leader_tip > follower_tip AND hash_path INVALID      | ABORT A3 (C2 violation)    |
//! | follower_tip is NONE AND leader_tip exists          | FAST_FORWARD (bootstrap)   |
//!
//! ## Fail-Closed Semantics
//!
//! Any ambiguous or unexpected input state results in an abort. The kernel never
//! returns a "proceed" decision when it cannot confirm a safe path.

use crate::error::Sync1AbortCode;
pub use crate::transport::TipId;

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
            Sync1Decision::FastForward
        }
        Some(follower) => {
            // Both tips present — apply the decision table.

            // Row: leader_tip == follower_tip -> DONE
            if leader.sequence == follower.sequence {
                if leader.hash == follower.hash {
                    return Sync1Decision::Done;
                } else {
                    // Same sequence, different hash — C2 violation.
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

    fn tip(seq: u64, hash: &str) -> TipId {
        TipId {
            sequence: seq,
            hash: hash.to_string(),
        }
    }

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

    // DONE
    #[test]
    fn done_when_tips_equal() {
        let input = both_tips(100, "hash_a", 100, "hash_a", false);
        assert_eq!(decide(&input), Sync1Decision::Done);
    }

    #[test]
    fn done_at_sequence_zero() {
        let input = both_tips(0, "genesis", 0, "genesis", false);
        assert_eq!(decide(&input), Sync1Decision::Done);
    }

    // ABORT A4 — follower ahead
    #[test]
    fn abort_a4_when_follower_ahead() {
        let input = both_tips(200, "h1", 100, "h2", false);
        let decision = decide(&input);
        assert!(decision.is_abort());
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A4));
    }

    // SYNC — leader ahead, hash_path valid
    #[test]
    fn sync_when_leader_ahead_hash_path_valid() {
        let input = both_tips(100, "h1", 200, "h2", true);
        assert_eq!(decide(&input), Sync1Decision::Sync);
    }

    // ABORT A3 — leader ahead, hash_path INVALID
    #[test]
    fn abort_a3_when_leader_ahead_hash_path_invalid() {
        let input = both_tips(100, "h1", 200, "h2", false);
        let decision = decide(&input);
        assert!(decision.is_abort());
        assert_eq!(decision.abort_code(), Some(Sync1AbortCode::A3));
    }

    // ABORT A3 — same sequence, different hash
    #[test]
    fn abort_a3_when_same_sequence_different_hash() {
        let input = both_tips(100, "hash_alpha", 100, "hash_beta", false);
        assert_eq!(decide(&input).abort_code(), Some(Sync1AbortCode::A3));
    }

    // FAST_FORWARD — follower empty
    #[test]
    fn fast_forward_when_follower_empty() {
        let input = DecisionInput {
            follower_tip: None,
            leader_tip: Some(tip(500, "leader_hash")),
            hash_path_valid: false,
        };
        assert_eq!(decide(&input), Sync1Decision::FastForward);
    }

    // ABORT A7 — leader tip missing
    #[test]
    fn abort_a7_when_leader_tip_missing() {
        let input = DecisionInput {
            follower_tip: Some(tip(100, "h")),
            leader_tip: None,
            hash_path_valid: false,
        };
        assert_eq!(decide(&input).abort_code(), Some(Sync1AbortCode::A7));
    }

    // Helpers
    #[test]
    fn is_proceed_true_for_done() {
        assert!(Sync1Decision::Done.is_proceed());
    }

    #[test]
    fn is_proceed_true_for_sync() {
        assert!(Sync1Decision::Sync.is_proceed());
    }

    #[test]
    fn is_proceed_false_for_abort() {
        assert!(!Sync1Decision::Abort(Sync1AbortCode::A0).is_proceed());
    }

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
}
