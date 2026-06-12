//! State transition validation for store entities.
//!
//! This module provides pure transition helpers that enforce valid state
//! transitions for capabilities, approvals, and executions.
//!
//! ## Deferred: Rollback/Full Execution Strictness
//!
//! The rollback and full-execution workflow strictness (e.g., enforcing that
//! Committed executions can only transition through specific compensation paths)
//! is **deferred** to a future slice. This module currently only enforces:
//!   - Capability: Active → {Used, Expired, Revoked, Quarantined}; terminal states absorbing
//!   - Approval:  Pending → {Granted, Denied, Expired}; terminal states absorbing
//!   - Execution: block transitions OUT of terminal states; allow all other transitions
//!
//! A future slice will add rollback-specific transition graphs and full strictness.

use ferrum_proto::{ApprovalState, CapabilityStatus, ExecutionState};

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

/// Returns true if transitioning FROM `from` TO `to` is valid for ExecutionState.
///
/// This only blocks transitions OUT of terminal states. All other transitions are allowed.
/// Rollback/full-execution workflow strictness (specific compensation paths, etc.) is deferred.
pub fn is_valid_execution_transition(from: &ExecutionState, _to: &ExecutionState) -> bool {
    // Block transitions out of terminal states
    if execution_state_is_terminal(from) {
        return false;
    }
    // All other transitions are allowed (full strictness deferred)
    true
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
    fn execution_authorized_to_prepared_valid() {
        assert!(is_valid_execution_transition(
            &ExecutionState::Authorized,
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
    fn execution_all_non_terminal_transitions_allowed() {
        // All transitions from non-terminal states are allowed
        let non_terminal = [
            ExecutionState::Proposed,
            ExecutionState::Authorized,
            ExecutionState::Prepared,
            ExecutionState::Running,
            ExecutionState::AwaitingApproval,
            ExecutionState::AwaitingVerification,
        ];
        let targets = [
            ExecutionState::Proposed,
            ExecutionState::Authorized,
            ExecutionState::Prepared,
            ExecutionState::Running,
            ExecutionState::AwaitingApproval,
            ExecutionState::AwaitingVerification,
            ExecutionState::Committed,
            ExecutionState::Compensated,
            ExecutionState::RolledBack,
            ExecutionState::Denied,
            ExecutionState::Quarantined,
            ExecutionState::Failed,
            ExecutionState::Canceled,
        ];
        for from in &non_terminal {
            for to in &targets {
                // Self-transitions are allowed for non-terminal states
                assert!(
                    is_valid_execution_transition(from, to),
                    "Expected transition from {:?} to {:?} to be allowed",
                    from,
                    to
                );
            }
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
}
