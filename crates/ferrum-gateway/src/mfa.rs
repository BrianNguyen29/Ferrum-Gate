//! MFA cryptographic helpers.
//!
//! TOTP support is implemented (PR #209): the module provides RFC 6238 TOTP
//! helpers (secret generation, AES-256-GCM encryption, verification, and
//! otpauth URI building). Admin routes use these helpers directly. WebAuthn,
//! backup codes, key rotation, and lockout remain deferred.

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

// ── TOTP cryptographic helpers ──

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use rand::Rng;
use sha1::Sha1;

// Re-export store/repo types needed by the shared lockout helpers.
use ferrum_proto::MfaCredentialRecord;
use ferrum_store::MfaCredentialRepo;
use std::sync::Arc;

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
    rand::rng().fill_bytes(&mut secret[..]);
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
    rand::rng().fill_bytes(&mut nonce_bytes[..]);
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

// ── Agent-scoped MFA lockout helpers ──

/// Result of attempting to verify a TOTP code against a factor while enforcing
/// both agent-level and factor-level lockout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpVerifyResult {
    /// Verification succeeded; the matched counter is returned. The caller must
    /// still atomically record the counter via `record_use` for replay protection.
    Success { counter: u64 },
    /// The agent or factor is locked. `retry_after_seconds` is the remaining time.
    Locked { retry_after_seconds: u64 },
    /// The code was invalid or the factor cannot be used.
    Invalid,
    /// An internal error occurred.
    Internal,
}

/// Check whether an agent is currently MFA-locked.
///
/// Returns `Ok(Some(retry_after_seconds))` if the agent has an active lockout
/// that has not yet expired. Returns `Ok(None)` when the agent is not locked or
/// no lockout record exists. Returns `Err` on a store failure so callers can
/// fail closed rather than treating a read error as unlocked.
pub async fn check_agent_mfa_lockout(
    repo: Arc<dyn MfaCredentialRepo>,
    agent_id: &str,
) -> Result<Option<u64>, ferrum_store::StoreError> {
    match repo.get_agent_lockout(agent_id).await {
        Ok(Some(lockout)) => {
            if let Some(locked_until) = lockout.locked_until {
                let now = chrono::Utc::now();
                if locked_until > now {
                    return Ok(Some((locked_until - now).num_seconds().max(0) as u64));
                }
            }
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err(e) => {
            tracing::warn!(error = %e, agent_id = %agent_id, "failed to read agent lockout");
            Err(e)
        }
    }
}

fn check_factor_lockout(record: &MfaCredentialRecord) -> Option<u64> {
    if let Some(locked_until) = record.locked_until {
        let now = chrono::Utc::now();
        if locked_until > now {
            return Some((locked_until - now).num_seconds().max(0) as u64);
        }
    }
    None
}

/// Reset both agent-level and factor-level MFA lockout state after a
/// successful, non-replayed verification.
///
/// Callers must only invoke this after `record_use(counter)` returns `Ok(true)`,
/// otherwise a replayed valid TOTP could reset lockout counters before the CAS
/// check rejects it.
pub async fn reset_mfa_lockout_after_success(
    repo: Arc<dyn MfaCredentialRepo>,
    agent_id: &str,
    mfa_factor_id: ferrum_proto::MfaFactorId,
) -> Result<(), ferrum_store::StoreError> {
    repo.reset_agent_lockout(agent_id).await?;
    repo.reset_lockout(mfa_factor_id).await?;
    Ok(())
}

/// Verify a TOTP code against the provided factor, enforcing both agent-level
/// and factor-level lockout.
///
/// On failure, records a failed attempt on both the agent and the factor and
/// returns the resulting lockout state. On cryptographic success, returns the
/// matched counter; the caller MUST atomically record the counter via
/// `record_use` and then call `reset_mfa_lockout_after_success` only when
/// `record_use` returns `Ok(true)`. This ordering prevents a replayed valid
/// TOTP from resetting lockout counters before CAS replay rejection.
pub async fn verify_totp_with_lockout(
    repo: Arc<dyn MfaCredentialRepo>,
    key_bytes: &[u8],
    agent_id: &str,
    record: &MfaCredentialRecord,
    code: &str,
    max_attempts: u32,
    lockout_duration_secs: u64,
) -> TotpVerifyResult {
    // Check agent-level lockout before touching the factor secret.
    let agent_retry = match check_agent_mfa_lockout(repo.clone(), agent_id).await {
        Ok(Some(retry_after)) => retry_after,
        Ok(None) => 0,
        Err(e) => {
            tracing::error!(error = %e, agent_id = %agent_id, "failed to check agent lockout");
            return TotpVerifyResult::Internal;
        }
    };

    if agent_retry > 0 {
        return TotpVerifyResult::Locked {
            retry_after_seconds: agent_retry,
        };
    }

    // Check factor-level lockout.
    if let Some(retry_after) = check_factor_lockout(record) {
        return TotpVerifyResult::Locked {
            retry_after_seconds: retry_after,
        };
    }

    let secret = match decrypt_secret(key_bytes, &record.encrypted_secret, &record.secret_nonce) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "mfa decrypt_secret failed");
            return TotpVerifyResult::Internal;
        }
    };

    let now = chrono::Utc::now().timestamp() as u64;
    match verify_totp_code_with_counter(&secret, code, now) {
        Ok(counter) => TotpVerifyResult::Success { counter },
        Err(_) => {
            // Record failure on the agent. If persistence fails, fail closed to
            // prevent an attacker from bypassing the lockout counter.
            match repo
                .record_agent_failed_attempt(agent_id, max_attempts, lockout_duration_secs)
                .await
            {
                Ok(agent_record) => {
                    let agent_locked = agent_record
                        .locked_until
                        .map(|lu| lu > chrono::Utc::now())
                        .unwrap_or(false);

                    // Record failure on the factor. If persistence fails, fail
                    // closed so the factor counter is not silently lost.
                    let factor_locked = match repo
                        .record_failed_attempt(
                            record.mfa_factor_id,
                            max_attempts,
                            lockout_duration_secs,
                        )
                        .await
                    {
                        Ok(locked) => locked,
                        Err(e) => {
                            tracing::error!(error = %e, factor_id = %record.mfa_factor_id, "record_failed_attempt failed");
                            return TotpVerifyResult::Internal;
                        }
                    };

                    // Build the post-update lockout result from the freshly
                    // written state. Use the agent record returned by the store
                    // and the factor lock boolean so threshold-crossing
                    // failures report `MfaLocked` with accurate retry details.
                    let agent_retry_secs = if agent_locked {
                        agent_record
                            .locked_until
                            .map(|lu| (lu - chrono::Utc::now()).num_seconds().max(0) as u64)
                    } else {
                        None
                    };

                    let factor_retry_secs = if factor_locked {
                        // Factor was locked by `record_failed_attempt`. Reload
                        // the factor record to get the fresh `locked_until` value.
                        match repo.get(record.mfa_factor_id).await {
                            Ok(Some(fresh)) => fresh
                                .locked_until
                                .map(|lu| (lu - chrono::Utc::now()).num_seconds().max(0) as u64),
                            Ok(None) => None,
                            Err(e) => {
                                tracing::error!(error = %e, factor_id = %record.mfa_factor_id, "failed to reload factor lockout");
                                return TotpVerifyResult::Internal;
                            }
                        }
                    } else {
                        None
                    };

                    match (agent_retry_secs, factor_retry_secs) {
                        (Some(a), Some(f)) => TotpVerifyResult::Locked {
                            retry_after_seconds: a.max(f),
                        },
                        (Some(r), None) | (None, Some(r)) => TotpVerifyResult::Locked {
                            retry_after_seconds: r,
                        },
                        (None, None) => TotpVerifyResult::Invalid,
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, agent_id = %agent_id, "record_agent_failed_attempt failed");
                    TotpVerifyResult::Internal
                }
            }
        }
    }
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
    use ferrum_proto::{MfaAgentLockoutRecord, MfaCredentialRecord, MfaFactorId, MfaFactorType};
    use ferrum_store::MfaCredentialRepo;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    // A minimal mock MfaCredentialRepo for testing lockout helper behavior.
    struct MockMfaRepo {
        factor: MfaCredentialRecord,
        agent_lockout: Mutex<Option<MfaAgentLockoutRecord>>,
        record_use_results: Mutex<VecDeque<ferrum_store::Result<bool>>>,
        record_use_calls: Mutex<Vec<(MfaFactorId, u64)>>,
        reset_agent_calls: Mutex<Vec<String>>,
        reset_factor_calls: Mutex<Vec<MfaFactorId>>,
        agent_failed_results: Mutex<VecDeque<ferrum_store::Result<MfaAgentLockoutRecord>>>,
        factor_failed_results: Mutex<VecDeque<ferrum_store::Result<bool>>>,
        fail_agent_lockout_read: Mutex<bool>,
    }

    impl MockMfaRepo {
        fn new(factor: MfaCredentialRecord) -> Self {
            Self {
                factor,
                agent_lockout: Mutex::new(None),
                record_use_results: Mutex::new(VecDeque::new()),
                record_use_calls: Mutex::new(Vec::new()),
                reset_agent_calls: Mutex::new(Vec::new()),
                reset_factor_calls: Mutex::new(Vec::new()),
                agent_failed_results: Mutex::new(VecDeque::new()),
                factor_failed_results: Mutex::new(VecDeque::new()),
                fail_agent_lockout_read: Mutex::new(false),
            }
        }

        fn with_factor(factor: MfaCredentialRecord) -> Self {
            Self::new(factor)
        }

        fn record_use_sequence(mut self, results: Vec<ferrum_store::Result<bool>>) -> Self {
            self.record_use_results = Mutex::new(results.into_iter().collect());
            self
        }

        fn factor_locked(
            mut self,
            locked: bool,
            retry_until: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Self {
            self.factor.locked_until = retry_until;
            self.factor.failed_attempts = if locked { 1 } else { 0 };
            self.factor_failed_results =
                Mutex::new(VecDeque::from(vec![ferrum_store::Result::Ok(locked)]));
            self
        }

        fn agent_lockout_error(self) -> Self {
            *self.fail_agent_lockout_read.lock().unwrap() = true;
            self
        }

        fn agent_failed_error(mut self) -> Self {
            self.agent_failed_results = Mutex::new(VecDeque::from(vec![Err(
                ferrum_store::StoreError::Other("agent failed write error".to_string()),
            )]));
            self
        }
    }

    #[async_trait::async_trait]
    impl MfaCredentialRepo for MockMfaRepo {
        async fn insert(&self, _record: &MfaCredentialRecord) -> ferrum_store::Result<()> {
            Ok(())
        }

        async fn get(
            &self,
            _mfa_factor_id: MfaFactorId,
        ) -> ferrum_store::Result<Option<MfaCredentialRecord>> {
            Ok(Some(self.factor.clone()))
        }

        async fn get_active_for_agent(
            &self,
            _agent_id: &str,
        ) -> ferrum_store::Result<Option<MfaCredentialRecord>> {
            Ok(Some(self.factor.clone()))
        }

        async fn list_by_agent(
            &self,
            _agent_id: &str,
        ) -> ferrum_store::Result<Vec<MfaCredentialRecord>> {
            Ok(vec![self.factor.clone()])
        }

        async fn activate(&self, _mfa_factor_id: MfaFactorId) -> ferrum_store::Result<bool> {
            Ok(true)
        }

        async fn record_use(
            &self,
            mfa_factor_id: MfaFactorId,
            counter: u64,
        ) -> ferrum_store::Result<bool> {
            self.record_use_calls
                .lock()
                .unwrap()
                .push((mfa_factor_id, counter));
            self.record_use_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(false))
        }

        async fn record_failed_attempt(
            &self,
            _mfa_factor_id: MfaFactorId,
            _max_attempts: u32,
            _lockout_duration_secs: u64,
        ) -> ferrum_store::Result<bool> {
            self.factor_failed_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(false))
        }

        async fn reset_lockout(&self, mfa_factor_id: MfaFactorId) -> ferrum_store::Result<bool> {
            self.reset_factor_calls.lock().unwrap().push(mfa_factor_id);
            Ok(true)
        }

        async fn revoke(&self, _mfa_factor_id: MfaFactorId) -> ferrum_store::Result<bool> {
            Ok(true)
        }

        async fn get_agent_lockout(
            &self,
            _agent_id: &str,
        ) -> ferrum_store::Result<Option<MfaAgentLockoutRecord>> {
            if *self.fail_agent_lockout_read.lock().unwrap() {
                return Err(ferrum_store::StoreError::Other(
                    "agent lockout read failed".to_string(),
                ));
            }
            Ok(self.agent_lockout.lock().unwrap().clone())
        }

        async fn record_agent_failed_attempt(
            &self,
            _agent_id: &str,
            _max_attempts: u32,
            _lockout_duration_secs: u64,
        ) -> ferrum_store::Result<MfaAgentLockoutRecord> {
            self.agent_failed_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    Ok(MfaAgentLockoutRecord {
                        agent_id: "agent-1".to_string(),
                        failed_attempts: 1,
                        locked_until: None,
                        last_failed_at: Some(chrono::Utc::now()),
                        lockout_count: 0,
                        updated_at: chrono::Utc::now(),
                    })
                })
        }

        async fn reset_agent_lockout(&self, agent_id: &str) -> ferrum_store::Result<bool> {
            self.reset_agent_calls
                .lock()
                .unwrap()
                .push(agent_id.to_string());
            Ok(true)
        }
    }

    fn test_factor(secret: &[u8], key: &[u8]) -> MfaCredentialRecord {
        let (ct, nonce) = encrypt_secret(key, secret).unwrap();
        MfaCredentialRecord::new("agent-1", MfaFactorType::Totp, ct, nonce, "k")
    }

    #[tokio::test]
    async fn replay_of_valid_totp_does_not_reset_lockout() {
        let key = vec![0u8; 32];
        let secret = generate_totp_secret();
        let factor = test_factor(&secret, &key);
        let ts = chrono::Utc::now().timestamp() as u64;
        let code = generate_totp_code(&secret, ts);

        let repo = Arc::new(
            MockMfaRepo::with_factor(factor.clone()).record_use_sequence(vec![Ok(true), Ok(false)]),
        );

        // First verification succeeds.
        let first =
            verify_totp_with_lockout(repo.clone(), &key, "agent-1", &factor, &code, 5, 900).await;
        let counter = match first {
            TotpVerifyResult::Success { counter } => counter,
            other => panic!("expected Success on first use, got {:?}", other),
        };

        // Simulate the caller: record_use succeeds, then reset lockout.
        assert!(
            repo.record_use(factor.mfa_factor_id, counter)
                .await
                .unwrap()
        );
        reset_mfa_lockout_after_success(repo.clone(), "agent-1", factor.mfa_factor_id)
            .await
            .unwrap();
        assert_eq!(repo.reset_agent_calls.lock().unwrap().len(), 1);
        assert_eq!(repo.reset_factor_calls.lock().unwrap().len(), 1);

        // Replay the same code. The helper returns Success because the crypto
        // matches, but the caller's record_use must return false and MUST NOT
        // reset lockout counters.
        let replay =
            verify_totp_with_lockout(repo.clone(), &key, "agent-1", &factor, &code, 5, 900).await;
        match replay {
            TotpVerifyResult::Success {
                counter: replay_counter,
            } => {
                assert_eq!(replay_counter, counter);
                // Simulated caller: record_use returns false for replay.
                assert!(
                    !repo
                        .record_use(factor.mfa_factor_id, replay_counter)
                        .await
                        .unwrap()
                );
                // Reset should NOT be called again.
                assert_eq!(repo.reset_agent_calls.lock().unwrap().len(), 1);
                assert_eq!(repo.reset_factor_calls.lock().unwrap().len(), 1);
            }
            other => panic!("expected Success on crypto match, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn check_agent_lockout_read_failure_fails_closed() {
        let repo = Arc::new(
            MockMfaRepo::new(test_factor(&generate_totp_secret(), &[0u8; 32]))
                .agent_lockout_error(),
        );
        assert!(check_agent_mfa_lockout(repo, "agent-1").await.is_err());
    }

    #[tokio::test]
    async fn verify_totp_agent_lockout_read_failure_fails_closed() {
        let key = vec![0u8; 32];
        let factor = test_factor(&generate_totp_secret(), &key);
        let repo = Arc::new(MockMfaRepo::with_factor(factor.clone()).agent_lockout_error());
        let result =
            verify_totp_with_lockout(repo.clone(), &key, "agent-1", &factor, "000000", 5, 900)
                .await;
        assert_eq!(result, TotpVerifyResult::Internal);
    }

    #[tokio::test]
    async fn verify_totp_agent_failed_write_failure_fails_closed() {
        let key = vec![0u8; 32];
        let factor = test_factor(&generate_totp_secret(), &key);
        let repo = Arc::new(MockMfaRepo::with_factor(factor.clone()).agent_failed_error());
        let result =
            verify_totp_with_lockout(repo.clone(), &key, "agent-1", &factor, "000000", 5, 900)
                .await;
        assert_eq!(result, TotpVerifyResult::Internal);
    }

    #[tokio::test]
    async fn threshold_crossing_invalid_code_returns_locked() {
        let key = vec![0u8; 32];
        let factor = test_factor(&generate_totp_secret(), &key);
        let locked_until = chrono::Utc::now() + chrono::Duration::seconds(900);
        let repo = Arc::new(
            MockMfaRepo::with_factor(factor.clone()).factor_locked(true, Some(locked_until)),
        );
        let result =
            verify_totp_with_lockout(repo.clone(), &key, "agent-1", &factor, "000000", 1, 900)
                .await;
        match result {
            TotpVerifyResult::Locked {
                retry_after_seconds,
            } => {
                assert!(retry_after_seconds > 0 && retry_after_seconds <= 900);
            }
            other => panic!("expected Locked, got {:?}", other),
        }
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
