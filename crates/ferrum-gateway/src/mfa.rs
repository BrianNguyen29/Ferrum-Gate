//! MFA verifier seam and no-op implementation.
//!
//! Phase 1 (ADR008): defines the `MfaVerifier` trait and a no-op placeholder
//! implementation. No concrete factor verification (TOTP, WebAuthn, etc.) is
//! wired yet. When `approval_mfa_required` is enabled, `resolve_approval` fails
//! closed with `403 mfa_required` because client factor transport is not yet
//! implemented.
//!
//! Phase 2 will introduce a TOTP adapter behind the trait.

use std::fmt;

/// Errors produced by MFA verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MfaError {
    /// No second factor was provided.
    Required,
    /// The provided factor is invalid (e.g. wrong TOTP code).
    Invalid,
    /// The factor type is not supported by the current verifier.
    Unsupported,
}

impl fmt::Display for MfaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MfaError::Required => write!(f, "mfa required"),
            MfaError::Invalid => write!(f, "invalid mfa factor"),
            MfaError::Unsupported => write!(f, "unsupported factor type"),
        }
    }
}

impl std::error::Error for MfaError {}

/// Pluggable MFA verifier interface.
///
/// Implementations may perform TOTP, WebAuthn, or out-of-band cryptographic
/// verification. The trait is `Send + Sync` so it can be held in `AppState`.
pub trait MfaVerifier: Send + Sync {
    /// Verify a second factor for the given actor.
    ///
    /// `factor` is an opaque payload whose interpretation is adapter-specific
    /// (e.g. a TOTP code string, a WebAuthn assertion JSON blob, etc.).
    fn verify(&self, actor_id: &str, factor: &str) -> Result<(), MfaError>;
}

/// No-op MFA verifier that accepts any factor.
///
/// This is a placeholder for Phase 1. It allows tests to exercise the seam
/// without a real MFA backend. Production deployments should replace this with
/// a concrete adapter (TOTP, WebAuthn, etc.) in Phase 2+.
pub struct NoopMfaVerifier;

impl MfaVerifier for NoopMfaVerifier {
    fn verify(&self, _actor_id: &str, _factor: &str) -> Result<(), MfaError> {
        // Placeholder: accepts any factor. Future adapters will perform
        // real cryptographic or time-based verification here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_mfa_verifier_accepts_any_factor() {
        let verifier = NoopMfaVerifier;
        assert!(verifier.verify("actor-1", "123456").is_ok());
        assert!(verifier.verify("actor-2", "totp-code").is_ok());
        assert!(verifier.verify("actor-3", "").is_ok());
    }

    #[test]
    fn test_mfa_error_display() {
        assert_eq!(MfaError::Required.to_string(), "mfa required");
        assert_eq!(MfaError::Invalid.to_string(), "invalid mfa factor");
        assert_eq!(MfaError::Unsupported.to_string(), "unsupported factor type");
    }
}
