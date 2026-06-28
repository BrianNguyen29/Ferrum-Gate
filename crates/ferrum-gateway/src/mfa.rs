//! MFA verifier seam and cryptographic helpers.
//!
//! Phase 1 (ADR008): defines the `MfaVerifier` trait and a no-op placeholder
//! implementation. No concrete factor verification (TOTP, WebAuthn, etc.) is
//! wired yet. When `approval_mfa_required` is enabled, `resolve_approval` fails
//! closed with `403 mfa_required` because client factor transport is not yet
//! implemented.
//!
//! Phase 2 introduces TOTP helpers (secret generation, AES-256-GCM encryption,
//! and RFC 6238 verification) behind this module. The admin routes use these
//! helpers directly; the `MfaVerifier` trait will be wired to a store-backed
//! implementation in a later slice.

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
    /// Cryptographic operation failed.
    Crypto(String),
    /// The MFA secret key is missing or misconfigured.
    Misconfigured,
}

impl fmt::Display for MfaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MfaError::Required => write!(f, "mfa required"),
            MfaError::Invalid => write!(f, "invalid mfa factor"),
            MfaError::Unsupported => write!(f, "unsupported factor type"),
            MfaError::Crypto(msg) => write!(f, "mfa crypto error: {}", msg),
            MfaError::Misconfigured => write!(f, "mfa misconfigured"),
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

// ── TOTP cryptographic helpers ──

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;

/// Length of a TOTP secret in bytes (20 bytes = 160 bits, RFC 4226/6238 default).
const TOTP_SECRET_LEN: usize = 20;
/// Time step in seconds (30s is the standard).
const TOTP_TIME_STEP: u64 = 30;
/// Number of digits in the TOTP code.
const TOTP_CODE_DIGITS: u32 = 6;
/// Slew window: how many steps before/after current to check.
const TOTP_SLEW_STEPS: i64 = 1;

/// Generate a random TOTP secret and return it as raw bytes.
///
/// The secret is 20 bytes (160 bits), suitable for RFC 4226/6238.
pub fn generate_totp_secret() -> Vec<u8> {
    let mut secret = vec![0u8; TOTP_SECRET_LEN];
    rand::thread_rng().fill_bytes(&mut secret[..]);
    secret
}

/// Build an otpauth URI for enrolling in an authenticator app.
///
/// `secret` should be the raw secret bytes; it is base32-encoded inside the URI.
/// `issuer` is displayed in the authenticator app (e.g. "FerrumGate").
/// `account` is the agent_id or user identifier.
///
/// Example output:
/// `otpauth://totp/FerrumGate:agent-1?secret=JBSWY3DPEHPK3PXP&issuer=FerrumGate`
pub fn build_otpauth_uri(secret: &[u8], issuer: &str, account: &str) -> String {
    let encoded = BASE32_NOPAD.encode(secret);
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}",
        urlencoding::encode(issuer),
        urlencoding::encode(account),
        encoded,
        urlencoding::encode(issuer)
    )
}

/// Encrypt a plaintext secret using AES-256-GCM.
///
/// `key_bytes` must be exactly 32 bytes (256 bits). Returns
/// `(ciphertext_base64, nonce_base64)`.
///
/// # Errors
/// Returns `MfaError::Crypto` if the key length is wrong or encryption fails.
pub fn encrypt_secret(key_bytes: &[u8], plaintext: &[u8]) -> Result<(String, String), MfaError> {
    if key_bytes.len() != 32 {
        return Err(MfaError::Crypto(format!(
            "AES-256 key must be 32 bytes, got {}",
            key_bytes.len()
        )));
    }
    let cipher = Aes256Gcm::new_from_slice(key_bytes)
        .map_err(|e| MfaError::Crypto(format!("invalid AES key: {}", e)))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes[..]);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| MfaError::Crypto(format!("encryption failed: {}", e)))?;

    Ok((
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, ciphertext),
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce_bytes),
    ))
}

/// Decrypt a ciphertext using AES-256-GCM.
///
/// `key_bytes` must be exactly 32 bytes. `ciphertext_base64` and `nonce_base64`
/// are the values returned by [`encrypt_secret`].
///
/// # Errors
/// Returns `MfaError::Crypto` on decoding or decryption failure.
pub fn decrypt_secret(
    key_bytes: &[u8],
    ciphertext_base64: &str,
    nonce_base64: &str,
) -> Result<Vec<u8>, MfaError> {
    if key_bytes.len() != 32 {
        return Err(MfaError::Crypto(format!(
            "AES-256 key must be 32 bytes, got {}",
            key_bytes.len()
        )));
    }
    let cipher = Aes256Gcm::new_from_slice(key_bytes)
        .map_err(|e| MfaError::Crypto(format!("invalid AES key: {}", e)))?;

    let ciphertext = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        ciphertext_base64,
    )
    .map_err(|e| MfaError::Crypto(format!("invalid ciphertext base64: {}", e)))?;

    let nonce_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, nonce_base64)
            .map_err(|e| MfaError::Crypto(format!("invalid nonce base64: {}", e)))?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| MfaError::Crypto(format!("decryption failed: {}", e)))
}

/// Generate a TOTP code for the given secret and timestamp.
///
/// `secret` is the raw secret bytes (not base32-encoded). `timestamp` is a
/// Unix timestamp in seconds. Returns a 6-digit code as a string.
pub fn generate_totp_code(secret: &[u8], timestamp: u64) -> String {
    let counter = timestamp / TOTP_TIME_STEP;
    hotp(secret, counter)
}

/// Verify a TOTP code against a secret with ±1 step slew.
///
/// Returns `Ok(())` if the code matches the current step, previous step, or
/// next step. Returns `Err(MfaError::Invalid)` otherwise.
pub fn verify_totp_code(secret: &[u8], code: &str, timestamp: u64) -> Result<(), MfaError> {
    verify_totp_code_with_counter(secret, code, timestamp).map(|_| ())
}

/// Verify a TOTP code and return the matched counter for replay protection.
///
/// Returns `Ok(counter)` if the code matches, where `counter` is the time step
/// that matched. Returns `Err(MfaError::Invalid)` otherwise.
pub fn verify_totp_code_with_counter(
    secret: &[u8],
    code: &str,
    timestamp: u64,
) -> Result<u64, MfaError> {
    let counter = (timestamp / TOTP_TIME_STEP) as i64;
    for delta in -TOTP_SLEW_STEPS..=TOTP_SLEW_STEPS {
        let expected = hotp(secret, (counter + delta) as u64);
        if constant_time_eq::constant_time_eq(expected.as_bytes(), code.as_bytes()) {
            return Ok((counter + delta) as u64);
        }
    }
    Err(MfaError::Invalid)
}

/// HMAC-based One-Time Password (HOTP) per RFC 4226.
///
/// `secret` is the shared secret bytes. `counter` is the moving factor.
fn hotp(secret: &[u8], counter: u64) -> String {
    let mut mac: Hmac<Sha1> =
        hmac::Mac::new_from_slice(secret).expect("HMAC can accept any key length");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize();
    let digest = result.into_bytes();

    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let code = ((digest[offset] & 0x7f) as u32) << 24
        | (digest[offset + 1] as u32) << 16
        | (digest[offset + 2] as u32) << 8
        | digest[offset + 3] as u32;
    let code = code % 10_u32.pow(TOTP_CODE_DIGITS);
    format!("{:0width$}", code, width = TOTP_CODE_DIGITS as usize)
}

// ── Key format helpers ──

/// Decode a 64-character hex string into 32 bytes for AES-256.
///
/// Returns `Err` if the string is not exactly 64 hex characters or contains
/// invalid hex.
pub fn decode_hex_key(hex_key: &str) -> Result<Vec<u8>, String> {
    if hex_key.len() != 64 {
        return Err(format!(
            "mfa_secret_key must be exactly 64 hex characters (32 bytes), got {}",
            hex_key.len()
        ));
    }
    let bytes =
        hex::decode(hex_key).map_err(|e| format!("mfa_secret_key contains invalid hex: {}", e))?;
    Ok(bytes)
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
        assert_eq!(
            MfaError::Crypto("foo".to_string()).to_string(),
            "mfa crypto error: foo"
        );
        assert_eq!(MfaError::Misconfigured.to_string(), "mfa misconfigured");
    }

    #[test]
    fn test_generate_totp_secret_length() {
        let secret = generate_totp_secret();
        assert_eq!(secret.len(), TOTP_SECRET_LEN);
    }

    #[test]
    fn test_build_otpauth_uri_format() {
        let secret = vec![0x1a, 0x2b, 0x3c, 0x4d, 0x5e];
        let uri = build_otpauth_uri(&secret, "FerrumGate", "agent-1");
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("FerrumGate"));
        assert!(uri.contains("agent-1"));
        assert!(uri.contains("secret="));
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = vec![0u8; 32];
        let plaintext = b"my-secret-value";
        let (ct, nonce) = encrypt_secret(&key, plaintext).unwrap();
        let decrypted = decrypt_secret(&key, &ct, &nonce).unwrap();
        assert_eq!(&decrypted[..], plaintext);
    }

    #[test]
    fn test_encrypt_rejects_short_key() {
        let key = vec![0u8; 16];
        let result = encrypt_secret(&key, b"test");
        assert!(matches!(result, Err(MfaError::Crypto(_))));
    }

    #[test]
    fn test_decrypt_rejects_short_key() {
        let key = vec![0u8; 16];
        let result = decrypt_secret(&key, "abc", "def");
        assert!(matches!(result, Err(MfaError::Crypto(_))));
    }

    #[test]
    fn test_decrypt_rejects_bad_base64() {
        let key = vec![0u8; 32];
        let result = decrypt_secret(&key, "!!!", "!!!");
        assert!(matches!(result, Err(MfaError::Crypto(_))));
    }

    #[test]
    fn test_hotp_deterministic() {
        let secret = b"12345678901234567890";
        let code1 = hotp(secret, 0);
        let code2 = hotp(secret, 0);
        assert_eq!(code1, code2);
        assert_eq!(code1.len(), 6);
    }

    #[test]
    fn test_generate_totp_code_deterministic() {
        let secret = b"12345678901234567890";
        let ts = 1_234_567_890;
        let code1 = generate_totp_code(secret, ts);
        let code2 = generate_totp_code(secret, ts);
        assert_eq!(code1, code2);
        assert_eq!(code1.len(), 6);
    }

    #[test]
    fn test_verify_totp_code_valid() {
        let secret = b"12345678901234567890";
        let ts = 1_234_567_890;
        let code = generate_totp_code(secret, ts);
        assert!(verify_totp_code(secret, &code, ts).is_ok());
    }

    #[test]
    fn test_verify_totp_code_slew_window() {
        let secret = b"12345678901234567890";
        let ts = 1_234_567_890;
        let code = generate_totp_code(secret, ts + TOTP_TIME_STEP);
        // code is for next step, but should still verify with +1 slew
        assert!(verify_totp_code(secret, &code, ts).is_ok());
    }

    #[test]
    fn test_verify_totp_code_invalid() {
        let secret = b"12345678901234567890";
        let ts = 1_234_567_890;
        assert!(verify_totp_code(secret, "000000", ts).is_err());
    }

    #[test]
    fn test_decode_hex_key_valid() {
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let bytes = decode_hex_key(key).unwrap();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_decode_hex_key_wrong_length() {
        let result = decode_hex_key("0123456789abcdef");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_hex_key_invalid_hex() {
        let result =
            decode_hex_key("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg");
        assert!(result.is_err());
    }
}
