//! State transition validation for store entities.
//!
//! This module provides pure transition helpers that enforce valid state
//! transitions for capabilities, approvals, and executions.
//!
//! ## Execution State Transition Matrix
//!
//! The `is_valid_execution_transition` function enforces a strict matrix at the
//! store seam. Self-transitions are allowed only for idempotent states:
//! Authorized, Prepared, Running, AwaitingVerification.
//!
//! Valid transitions (behavior-preserving for current handler sites):
//!
//! | From               | To (valid)                                                  |
//! |--------------------|-------------------------------------------------------------|
//! | Proposed           | Authorized, Running, Canceled                               |
//! | Authorized         | Running, Canceled, Authorized (self)                        |
//! | Prepared           | Running, Canceled, Prepared (self)                        |
//! | Running            | AwaitingVerification, Committed, Failed, Compensated, RecoveryRequired, Running (self) |
//! | AwaitingVerification | Committed, Failed, Compensated, RecoveryRequired, AwaitingVerification (self) |
//! | RecoveryRequired   | AwaitingVerification, Compensated, Failed, RecoveryRequired (self) |
//! | AwaitingApproval   | Canceled                                                    |
//! | Terminal           | none                                                        |
//!
//! Terminal states: Committed, Compensated, RolledBack, Denied, Quarantined,
//! Failed, Canceled.
//!
//! Rollback/full-execution workflow strictness (e.g., specific compensation paths)
//! is enforced at the handler layer; this matrix is the store seam guard.
//!
//! ## Deferred: Rollback/Full Execution Strictness
//!
//! The rollback and full-execution workflow strictness (e.g., enforcing that
//! Committed executions can only transition through specific compensation paths)
//! is **deferred** to a future slice. This module currently enforces:
//!   - Capability: Active → {Used, Expired, Revoked, Quarantined}; terminal states absorbing
//!   - Approval:  Pending → {Granted, Denied, Expired}; terminal states absorbing
//!   - Execution: strict matrix above; terminal states absorbing
//!
//! A future slice will add rollback-specific transition graphs and full strictness.

use ferrum_proto::{ApprovalState, CapabilityStatus, ExecutionState, RollbackState};

/// Returns true if the CapabilityStatus is terminal (absorbing).
///
/// Terminal states: Used, Expired, Revoked, Quarantined
pub fn capability_status_is_terminal(status: &CapabilityStatus) -> bool {
    matches!(
        status,
        CapabilityStatus::Used
            | CapabilityStatus::Expired
            | CapabilityStatus::Revoked
            | CapabilityStatus::Quarantined
    )
}

/// Returns true if transitioning FROM `from` TO `to` is valid for CapabilityStatus.
///
/// Valid transitions:
///   - Active → {Used, Expired, Revoked, Quarantined}
///   - All other FROM states are terminal (absorbing) → no valid transitions out
pub fn is_valid_capability_transition(from: &CapabilityStatus, to: &CapabilityStatus) -> bool {
    match from {
        CapabilityStatus::Active => matches!(
            to,
            CapabilityStatus::Used
                | CapabilityStatus::Expired
                | CapabilityStatus::Revoked
                | CapabilityStatus::Quarantined
        ),
        // Terminal states are absorbing
        _ => false,
    }
}

/// Returns true if the ApprovalState is terminal (absorbing).
///
/// Terminal states: Granted, Denied, Expired
pub fn approval_state_is_terminal(state: &ApprovalState) -> bool {
    matches!(
        state,
        ApprovalState::Granted | ApprovalState::Denied | ApprovalState::Expired
    )
}

/// Returns true if transitioning FROM `from` TO `to` is valid for ApprovalState.
///
/// Valid transitions:
///   - Pending → {Granted, Denied, Expired}
///   - All other FROM states are terminal (absorbing) → no valid transitions out
pub fn is_valid_approval_transition(from: &ApprovalState, to: &ApprovalState) -> bool {
    match from {
        ApprovalState::Pending => {
            matches!(
                to,
                ApprovalState::Granted | ApprovalState::Denied | ApprovalState::Expired
            )
        }
        // Terminal states are absorbing
        _ => false,
    }
}

/// Returns true if the ExecutionState is terminal.
///
/// Terminal states: Committed, Compensated, RolledBack, Denied, Quarantined, Failed, Canceled
pub fn execution_state_is_terminal(state: &ExecutionState) -> bool {
    matches!(
        state,
        ExecutionState::Committed
            | ExecutionState::Compensated
            | ExecutionState::RolledBack
            | ExecutionState::Denied
            | ExecutionState::Quarantined
            | ExecutionState::Failed
            | ExecutionState::Canceled
    )
}

/// Returns true if the RollbackState is terminal.
///
/// Terminal states: Committed, Compensated, RolledBack, Failed, Expired.
/// Verified is non-terminal (must reach Committed) and CompensationPending is
/// reserved/unwritten in this slice.
pub fn rollback_state_is_terminal(state: &RollbackState) -> bool {
    matches!(
        state,
        RollbackState::Committed
            | RollbackState::Compensated
            | RollbackState::RolledBack
            | RollbackState::Failed
            | RollbackState::Expired
    )
}

/// Returns true if transitioning FROM `from` TO `to` is valid for RollbackState.
///
/// Valid transitions (store seam guard, behavior-preserving for current handlers):
///
/// | From                 | To (valid)                                              |
/// |----------------------|---------------------------------------------------------|
/// | PendingPrepare       | Prepared                                                |
/// | Prepared             | ExecutedAwaitingVerify, RecoveryRequired, Prepared (self) |
/// | ExecutedAwaitingVerify | Verified, Failed, Compensated, RecoveryRequired, ExecutedAwaitingVerify (self) |
/// | RecoveryRequired     | ExecutedAwaitingVerify, Compensated, Failed, RecoveryRequired (self) |
/// | Verified             | Committed                                               |
/// | Terminal             | none                                                    |
///
/// CompensationPending is reserved/unwritten in this slice and has no transitions.
pub fn is_valid_rollback_transition(from: &RollbackState, to: &RollbackState) -> bool {
    if rollback_state_is_terminal(from) {
        return false;
    }
    match from {
        RollbackState::PendingPrepare => {
            matches!(to, RollbackState::Prepared | RollbackState::PendingPrepare)
        }
        RollbackState::Prepared => matches!(
            to,
            RollbackState::ExecutedAwaitingVerify
                | RollbackState::RecoveryRequired
                | RollbackState::Prepared
        ),
        RollbackState::ExecutedAwaitingVerify => matches!(
            to,
            RollbackState::Verified
                | RollbackState::Failed
                | RollbackState::Compensated
                | RollbackState::RecoveryRequired
                | RollbackState::ExecutedAwaitingVerify
        ),
        RollbackState::RecoveryRequired => matches!(
            to,
            RollbackState::ExecutedAwaitingVerify
                | RollbackState::Compensated
                | RollbackState::Failed
                | RollbackState::RecoveryRequired
        ),
        RollbackState::Verified => matches!(to, RollbackState::Committed),
        _ => false,
    }
}

/// Returns true if transitioning FROM `from` TO `to` is valid for ExecutionState.
///
/// Strict matrix enforced at the store seam. Self-transitions are allowed only
/// for idempotent non-terminal states: Authorized, Prepared, Running,
/// AwaitingVerification, RecoveryRequired.
///
/// Valid transitions (behavior-preserving for current handler sites):
///
/// | From                 | To (valid)                                                  |
/// |----------------------|-------------------------------------------------------------|
/// | Proposed             | Authorized, Running, Canceled                               |
/// | Authorized           | Running, Canceled, Authorized (self)                      |
/// | Prepared             | Running, Canceled, Prepared (self)                        |
/// | Running              | Committed, Failed, Compensated, RecoveryRequired, Running (self) |
/// | AwaitingVerification   | Committed, Failed, Compensated, RecoveryRequired, AwaitingVerification (self) |
/// | RecoveryRequired     | AwaitingVerification, Compensated, Failed, RecoveryRequired (self) |
/// | AwaitingApproval     | Canceled                                                    |
/// | Terminal             | none                                                        |
///
/// Rollback/full-execution workflow strictness (e.g., specific compensation paths)
/// is enforced at the handler layer; this matrix is the store seam guard.
pub fn is_valid_execution_transition(from: &ExecutionState, to: &ExecutionState) -> bool {
    if execution_state_is_terminal(from) {
        return false;
    }
    match from {
        ExecutionState::Proposed => matches!(
            to,
            ExecutionState::Authorized | ExecutionState::Running | ExecutionState::Canceled
        ),
        ExecutionState::Authorized => matches!(
            to,
            ExecutionState::Running | ExecutionState::Canceled | ExecutionState::Authorized
        ),
        ExecutionState::Prepared => matches!(
            to,
            ExecutionState::Running | ExecutionState::Canceled | ExecutionState::Prepared
        ),
        ExecutionState::Running => matches!(
            to,
            ExecutionState::Committed
                | ExecutionState::Failed
                | ExecutionState::Compensated
                | ExecutionState::RecoveryRequired
                | ExecutionState::AwaitingVerification
                | ExecutionState::Running
        ),
        ExecutionState::AwaitingVerification => matches!(
            to,
            ExecutionState::Committed
                | ExecutionState::Failed
                | ExecutionState::Compensated
                | ExecutionState::RecoveryRequired
                | ExecutionState::AwaitingVerification
        ),
        ExecutionState::RecoveryRequired => matches!(
            to,
            ExecutionState::AwaitingVerification
                | ExecutionState::Compensated
                | ExecutionState::Failed
                | ExecutionState::RecoveryRequired
        ),
        ExecutionState::AwaitingApproval => matches!(to, ExecutionState::Canceled),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Capability tests =====

    #[test]
    fn capability_active_to_used_valid() {
        assert!(is_valid_capability_transition(
            &CapabilityStatus::Active,
            &CapabilityStatus::Used,
        ));
    }

    #[test]
    fn capability_active_to_expired_valid() {
        assert!(is_valid_capability_transition(
            &CapabilityStatus::Active,
            &CapabilityStatus::Expired,
        ));
    }

    #[test]
    fn capability_active_to_revoked_valid() {
        assert!(is_valid_capability_transition(
            &CapabilityStatus::Active,
            &CapabilityStatus::Revoked,
        ));
    }

    #[test]
    fn capability_active_to_quarantined_valid() {
        assert!(is_valid_capability_transition(
            &CapabilityStatus::Active,
            &CapabilityStatus::Quarantined,
        ));
    }

    #[test]
    fn capability_used_is_terminal() {
        assert!(capability_status_is_terminal(&CapabilityStatus::Used));
    }

    #[test]
    fn capability_terminal_no_transitions() {
        // Cannot transition FROM terminal states
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Used,
            &CapabilityStatus::Active,
        ));
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Expired,
            &CapabilityStatus::Active,
        ));
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Revoked,
            &CapabilityStatus::Active,
        ));
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Quarantined,
            &CapabilityStatus::Active,
        ));
        // Cannot transition between terminal states
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Used,
            &CapabilityStatus::Expired,
        ));
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Revoked,
            &CapabilityStatus::Quarantined,
        ));
    }

    #[test]
    fn capability_active_to_active_invalid() {
        // Self-transition from Active is not valid (must leave Active)
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Active,
            &CapabilityStatus::Active,
        ));
    }

    // ===== Approval tests =====

    #[test]
    fn approval_pending_to_granted_valid() {
        assert!(is_valid_approval_transition(
            &ApprovalState::Pending,
            &ApprovalState::Granted,
        ));
    }

    #[test]
    fn approval_pending_to_denied_valid() {
        assert!(is_valid_approval_transition(
            &ApprovalState::Pending,
            &ApprovalState::Denied,
        ));
    }

    #[test]
    fn approval_pending_to_expired_valid() {
        assert!(is_valid_approval_transition(
            &ApprovalState::Pending,
            &ApprovalState::Expired,
        ));
    }

    #[test]
    fn approval_granted_is_terminal() {
        assert!(approval_state_is_terminal(&ApprovalState::Granted));
    }

    #[test]
    fn approval_denied_is_terminal() {
        assert!(approval_state_is_terminal(&ApprovalState::Denied));
    }

    #[test]
    fn approval_expired_is_terminal() {
        assert!(approval_state_is_terminal(&ApprovalState::Expired));
    }

    #[test]
    fn approval_terminal_no_transitions() {
        assert!(!is_valid_approval_transition(
            &ApprovalState::Granted,
            &ApprovalState::Pending,
        ));
        assert!(!is_valid_approval_transition(
            &ApprovalState::Denied,
            &ApprovalState::Pending,
        ));
        assert!(!is_valid_approval_transition(
            &ApprovalState::Expired,
            &ApprovalState::Pending,
        ));
        assert!(!is_valid_approval_transition(
            &ApprovalState::Granted,
            &ApprovalState::Denied,
        ));
    }

    #[test]
    fn approval_pending_to_pending_invalid() {
        assert!(!is_valid_approval_transition(
            &ApprovalState::Pending,
            &ApprovalState::Pending,
        ));
    }

    // ===== Execution tests =====

    #[test]
    fn execution_proposed_to_authorized_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Proposed,
            &ExecutionState::Authorized,
        ));
    }

    #[test]
    fn execution_proposed_to_running_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Proposed,
            &ExecutionState::Running,
        ));
    }

    #[test]
    fn execution_proposed_to_canceled_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Proposed,
            &ExecutionState::Canceled,
        ));
    }

    #[test]
    fn execution_authorized_to_running_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Authorized,
            &ExecutionState::Running,
        ));
    }

    #[test]
    fn execution_authorized_to_canceled_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Authorized,
            &ExecutionState::Canceled,
        ));
    }

    #[test]
    fn execution_authorized_self_transition_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Authorized,
            &ExecutionState::Authorized,
        ));
    }

    #[test]
    fn execution_prepared_to_running_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Prepared,
            &ExecutionState::Running,
        ));
    }

    #[test]
    fn execution_prepared_to_canceled_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Prepared,
            &ExecutionState::Canceled,
        ));
    }

    #[test]
    fn execution_prepared_self_transition_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Prepared,
            &ExecutionState::Prepared,
        ));
    }

    #[test]
    fn execution_running_to_committed_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Running,
            &ExecutionState::Committed,
        ));
    }

    #[test]
    fn execution_running_to_failed_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Running,
            &ExecutionState::Failed,
        ));
    }

    #[test]
    fn execution_running_to_compensated_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Running,
            &ExecutionState::Compensated,
        ));
    }

    #[test]
    fn execution_running_self_transition_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Running,
            &ExecutionState::Running,
        ));
    }

    #[test]
    fn execution_awaiting_verification_to_committed_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::AwaitingVerification,
            &ExecutionState::Committed,
        ));
    }

    #[test]
    fn execution_awaiting_verification_to_failed_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::AwaitingVerification,
            &ExecutionState::Failed,
        ));
    }

    #[test]
    fn execution_awaiting_verification_to_compensated_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::AwaitingVerification,
            &ExecutionState::Compensated,
        ));
    }

    #[test]
    fn execution_awaiting_verification_self_transition_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::AwaitingVerification,
            &ExecutionState::AwaitingVerification,
        ));
    }

    #[test]
    fn execution_awaiting_approval_to_canceled_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::AwaitingApproval,
            &ExecutionState::Canceled,
        ));
    }

    #[test]
    fn execution_authorized_to_prepared_invalid() {
        assert!(!is_valid_execution_transition(
            &ExecutionState::Authorized,
            &ExecutionState::Prepared,
        ));
    }

    #[test]
    fn execution_prepared_to_authorized_invalid() {
        assert!(!is_valid_execution_transition(
            &ExecutionState::Prepared,
            &ExecutionState::Authorized,
        ));
    }

    #[test]
    fn execution_running_to_authorized_invalid() {
        assert!(!is_valid_execution_transition(
            &ExecutionState::Running,
            &ExecutionState::Authorized,
        ));
    }

    #[test]
    fn execution_committed_is_terminal() {
        assert!(execution_state_is_terminal(&ExecutionState::Committed));
    }

    #[test]
    fn execution_canceled_is_terminal() {
        assert!(execution_state_is_terminal(&ExecutionState::Canceled));
    }

    #[test]
    fn execution_terminal_no_transitions() {
        // Cannot transition out of any terminal state
        assert!(!is_valid_execution_transition(
            &ExecutionState::Committed,
            &ExecutionState::Running,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::Compensated,
            &ExecutionState::Running,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::RolledBack,
            &ExecutionState::Running,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::Denied,
            &ExecutionState::Running,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::Quarantined,
            &ExecutionState::Running,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::Failed,
            &ExecutionState::Running,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::Canceled,
            &ExecutionState::Running,
        ));
        // Cannot transition between terminal states
        assert!(!is_valid_execution_transition(
            &ExecutionState::Committed,
            &ExecutionState::Failed,
        ));
        assert!(!is_valid_execution_transition(
            &ExecutionState::Failed,
            &ExecutionState::Committed,
        ));
    }

    #[test]
    fn execution_invalid_non_terminal_transitions_blocked() {
        let invalid_pairs = [
            (ExecutionState::Proposed, ExecutionState::Prepared),
            (ExecutionState::Proposed, ExecutionState::Committed),
            (ExecutionState::Proposed, ExecutionState::Failed),
            (ExecutionState::Proposed, ExecutionState::Compensated),
            (ExecutionState::Proposed, ExecutionState::AwaitingApproval),
            (
                ExecutionState::Proposed,
                ExecutionState::AwaitingVerification,
            ),
            (ExecutionState::Authorized, ExecutionState::Proposed),
            (ExecutionState::Authorized, ExecutionState::Prepared),
            (ExecutionState::Authorized, ExecutionState::Committed),
            (ExecutionState::Prepared, ExecutionState::Proposed),
            (ExecutionState::Prepared, ExecutionState::Committed),
            (ExecutionState::Running, ExecutionState::Proposed),
            (ExecutionState::Running, ExecutionState::Prepared),
            (ExecutionState::Running, ExecutionState::AwaitingApproval),
            (ExecutionState::Running, ExecutionState::Canceled),
            (
                ExecutionState::AwaitingVerification,
                ExecutionState::Proposed,
            ),
            (
                ExecutionState::AwaitingVerification,
                ExecutionState::Authorized,
            ),
            (
                ExecutionState::AwaitingVerification,
                ExecutionState::Prepared,
            ),
            (
                ExecutionState::AwaitingVerification,
                ExecutionState::Running,
            ),
            (
                ExecutionState::AwaitingVerification,
                ExecutionState::Canceled,
            ),
            (ExecutionState::AwaitingApproval, ExecutionState::Proposed),
            (ExecutionState::AwaitingApproval, ExecutionState::Authorized),
            (ExecutionState::AwaitingApproval, ExecutionState::Prepared),
            (ExecutionState::AwaitingApproval, ExecutionState::Running),
            (
                ExecutionState::AwaitingApproval,
                ExecutionState::AwaitingVerification,
            ),
            (ExecutionState::AwaitingApproval, ExecutionState::Committed),
            (ExecutionState::AwaitingApproval, ExecutionState::Failed),
            (
                ExecutionState::AwaitingApproval,
                ExecutionState::Compensated,
            ),
        ];
        for (from, to) in invalid_pairs {
            assert!(
                !is_valid_execution_transition(&from, &to),
                "Expected transition from {:?} to {:?} to be blocked",
                from,
                to
            );
        }
    }

    // ===== Adversarial tests =====

    #[test]
    fn capability_cannot_reuse_used() {
        // Trying to set Used back to Active should fail
        assert!(!is_valid_capability_transition(
            &CapabilityStatus::Used,
            &CapabilityStatus::Active,
        ));
    }

    #[test]
    fn approval_cannot_regrant_denied() {
        // Trying to set Denied back to Granted should fail
        assert!(!is_valid_approval_transition(
            &ApprovalState::Denied,
            &ApprovalState::Granted,
        ));
    }

    #[test]
    fn execution_cannot_recommit_completed() {
        // Trying to transition from Committed back to Running should fail
        assert!(!is_valid_execution_transition(
            &ExecutionState::Committed,
            &ExecutionState::Running,
        ));
    }

    // ===== Recovery-required execution tests (Slice 2) =====

    #[test]
    fn execution_recovery_required_non_terminal() {
        assert!(!execution_state_is_terminal(
            &ExecutionState::RecoveryRequired
        ));
    }

    #[test]
    fn execution_running_to_recovery_required_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Running,
            &ExecutionState::RecoveryRequired,
        ));
    }

    #[test]
    fn execution_awaiting_verification_to_recovery_required_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::AwaitingVerification,
            &ExecutionState::RecoveryRequired,
        ));
    }

    #[test]
    fn execution_recovery_required_to_awaiting_verification_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::RecoveryRequired,
            &ExecutionState::AwaitingVerification,
        ));
    }

    #[test]
    fn execution_recovery_required_to_compensated_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::RecoveryRequired,
            &ExecutionState::Compensated,
        ));
    }

    #[test]
    fn execution_recovery_required_to_failed_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::RecoveryRequired,
            &ExecutionState::Failed,
        ));
    }

    #[test]
    fn execution_recovery_required_self_transition_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::RecoveryRequired,
            &ExecutionState::RecoveryRequired,
        ));
    }

    #[test]
    fn execution_recovery_required_invalid_transitions_blocked() {
        let invalid_pairs = [
            (ExecutionState::RecoveryRequired, ExecutionState::Proposed),
            (ExecutionState::RecoveryRequired, ExecutionState::Authorized),
            (ExecutionState::RecoveryRequired, ExecutionState::Prepared),
            (ExecutionState::RecoveryRequired, ExecutionState::Running),
            (ExecutionState::RecoveryRequired, ExecutionState::Committed),
            (ExecutionState::RecoveryRequired, ExecutionState::RolledBack),
            (ExecutionState::RecoveryRequired, ExecutionState::Canceled),
            (
                ExecutionState::RecoveryRequired,
                ExecutionState::AwaitingApproval,
            ),
            (ExecutionState::Proposed, ExecutionState::RecoveryRequired),
            (ExecutionState::Authorized, ExecutionState::RecoveryRequired),
            (ExecutionState::Prepared, ExecutionState::RecoveryRequired),
            (
                ExecutionState::AwaitingApproval,
                ExecutionState::RecoveryRequired,
            ),
        ];
        for (from, to) in invalid_pairs {
            assert!(
                !is_valid_execution_transition(&from, &to),
                "Expected transition from {:?} to {:?} to be blocked",
                from,
                to
            );
        }
    }

    // ===== Rollback state tests (Slice 2) =====

    #[test]
    fn rollback_rolled_back_is_terminal() {
        assert!(rollback_state_is_terminal(&RollbackState::RolledBack));
    }

    #[test]
    fn rollback_recovery_required_non_terminal() {
        assert!(!rollback_state_is_terminal(
            &RollbackState::RecoveryRequired
        ));
    }

    #[test]
    fn rollback_pending_prepare_to_prepared_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::PendingPrepare,
            &RollbackState::Prepared,
        ));
    }

    #[test]
    fn rollback_prepared_to_executed_awaiting_verify_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::Prepared,
            &RollbackState::ExecutedAwaitingVerify,
        ));
    }

    #[test]
    fn rollback_prepared_to_recovery_required_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::Prepared,
            &RollbackState::RecoveryRequired,
        ));
    }

    #[test]
    fn rollback_executed_awaiting_verify_to_recovery_required_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::ExecutedAwaitingVerify,
            &RollbackState::RecoveryRequired,
        ));
    }

    #[test]
    fn rollback_recovery_required_to_executed_awaiting_verify_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::RecoveryRequired,
            &RollbackState::ExecutedAwaitingVerify,
        ));
    }

    #[test]
    fn rollback_recovery_required_to_compensated_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::RecoveryRequired,
            &RollbackState::Compensated,
        ));
    }

    #[test]
    fn rollback_recovery_required_to_failed_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::RecoveryRequired,
            &RollbackState::Failed,
        ));
    }

    #[test]
    fn rollback_recovery_required_self_transition_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::RecoveryRequired,
            &RollbackState::RecoveryRequired,
        ));
    }

    #[test]
    fn rollback_verified_to_committed_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::Verified,
            &RollbackState::Committed,
        ));
    }

    #[test]
    fn rollback_executed_awaiting_verify_to_verified_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::ExecutedAwaitingVerify,
            &RollbackState::Verified,
        ));
    }

    #[test]
    fn rollback_executed_awaiting_verify_to_compensated_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::ExecutedAwaitingVerify,
            &RollbackState::Compensated,
        ));
    }

    #[test]
    fn rollback_executed_awaiting_verify_to_failed_valid() {
        assert!(is_valid_rollback_transition(
            &RollbackState::ExecutedAwaitingVerify,
            &RollbackState::Failed,
        ));
    }

    #[test]
    fn rollback_compensation_pending_reserved_no_transitions() {
        // CompensationPending is reserved/unwritten in this slice.
        assert!(!is_valid_rollback_transition(
            &RollbackState::CompensationPending,
            &RollbackState::Compensated,
        ));
        assert!(!is_valid_rollback_transition(
            &RollbackState::Prepared,
            &RollbackState::CompensationPending,
        ));
    }

    #[test]
    fn rollback_terminal_no_transitions() {
        assert!(!is_valid_rollback_transition(
            &RollbackState::Committed,
            &RollbackState::Prepared,
        ));
        assert!(!is_valid_rollback_transition(
            &RollbackState::Compensated,
            &RollbackState::Prepared,
        ));
        assert!(!is_valid_rollback_transition(
            &RollbackState::RolledBack,
            &RollbackState::Prepared,
        ));
        assert!(!is_valid_rollback_transition(
            &RollbackState::Failed,
            &RollbackState::Prepared,
        ));
        assert!(!is_valid_rollback_transition(
            &RollbackState::Expired,
            &RollbackState::Prepared,
        ));
        // Cannot transition between terminal states
        assert!(!is_valid_rollback_transition(
            &RollbackState::Committed,
            &RollbackState::Failed,
        ));
        assert!(!is_valid_rollback_transition(
            &RollbackState::Failed,
            &RollbackState::Committed,
        ));
    }

    #[test]
    fn rollback_invalid_transitions_blocked() {
        let invalid_pairs = [
            (
                RollbackState::PendingPrepare,
                RollbackState::ExecutedAwaitingVerify,
            ),
            (RollbackState::PendingPrepare, RollbackState::Verified),
            (RollbackState::PendingPrepare, RollbackState::Committed),
            (RollbackState::PendingPrepare, RollbackState::Compensated),
            (RollbackState::PendingPrepare, RollbackState::Failed),
            (RollbackState::PendingPrepare, RollbackState::RolledBack),
            (RollbackState::PendingPrepare, RollbackState::Expired),
            (RollbackState::Prepared, RollbackState::PendingPrepare),
            (RollbackState::Prepared, RollbackState::Verified),
            (RollbackState::Prepared, RollbackState::Committed),
            (RollbackState::Prepared, RollbackState::Compensated),
            (RollbackState::Prepared, RollbackState::Failed),
            (RollbackState::Prepared, RollbackState::RolledBack),
            (
                RollbackState::ExecutedAwaitingVerify,
                RollbackState::PendingPrepare,
            ),
            (
                RollbackState::ExecutedAwaitingVerify,
                RollbackState::Prepared,
            ),
            (
                RollbackState::ExecutedAwaitingVerify,
                RollbackState::Committed,
            ),
            (
                RollbackState::ExecutedAwaitingVerify,
                RollbackState::RolledBack,
            ),
            (
                RollbackState::ExecutedAwaitingVerify,
                RollbackState::Expired,
            ),
            (RollbackState::Verified, RollbackState::Prepared),
            (
                RollbackState::Verified,
                RollbackState::ExecutedAwaitingVerify,
            ),
            (RollbackState::Verified, RollbackState::Compensated),
            (RollbackState::Verified, RollbackState::Failed),
            (RollbackState::Verified, RollbackState::RolledBack),
            (RollbackState::Verified, RollbackState::Expired),
        ];
        for (from, to) in invalid_pairs {
            assert!(
                !is_valid_rollback_transition(&from, &to),
                "Expected rollback transition from {:?} to {:?} to be blocked",
                from,
                to
            );
        }
    }
}
