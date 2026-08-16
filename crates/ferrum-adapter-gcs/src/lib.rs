//! GCS adapter for mutation and recovery.
//!
//! This adapter implements the `RollbackAdapter` trait for Google Cloud Storage
//! object operations, modeling generation/metageneration for rollback and using
//! fail-closed precondition semantics.
//!
//! # Recovery Slice (P2-4)
//!
//! The adapter supports bounded GCS object operations with generation-based
//! rollback:
//! - `prepare`: validates bucket/key against the allowlist and captures the
//!   object's current `generation` (if provided) as `before_generation`.
//! - `execute`: performs the GCS operation (put, delete, get). For mutating
//!   operations, captures the resulting `generation` / `metageneration` as
//!   `after_generation` / `after_metageneration`.
//! - `rollback`/`compensate`: for mutating operations, fails closed unless both
//!   `before_generation` and `after_generation` are present. Live GCS recovery
//!   is gated by the `gcs-client` feature and `live: true`.
//!
//! # Limitations
//!
//! - Live GCS execution is implemented behind the `gcs-client` feature and the
//!   `live` config flag. When both are enabled, real GCS calls would be made.
//!   This slice ships a shape-only implementation; the live client path returns
//!   a clear "not implemented" error so it cannot silently execute.
//! - `live: false` is the default. In this mode all phases perform shape-only
//!   validation and return `execution_groundwork=true`.
//! - Single-bucket allowlist only; no bucket creation/IAM/ACL admin.
//! - Max object size is enforced at the adapter boundary, not by GCS itself.
//! - No resumable upload, multi-bucket allowlists, signed URLs, retention, or
//!   lifecycle management.
//! - Azure Blob is deferred; see ADR-018.

use async_trait::async_trait;
use chrono::Utc;
use ferrum_proto::{
    ActionType, CheckType, JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use thiserror::Error;

pub mod planner;
pub use planner::PlannableGcsAdapter;

pub const ADAPTER_KIND: &str = "ferrum-adapter-gcs";

const PHASE_PREPARE: &str = "prepare";
const PHASE_EXECUTE: &str = "execute";
const PHASE_VERIFY: &str = "verify";
const PHASE_ROLLBACK: &str = "rollback";
const PHASE_COMPENSATE: &str = "compensate";

/// Configuration for GCS operation bounds.
///
/// Provides safety limits: single-bucket allowlist, max object size, and
/// optional endpoint override for emulator/local testing.
#[derive(Debug, Clone)]
pub struct GcsConfig {
    /// Allowed bucket name (exact match). All operations must target this bucket.
    pub allowed_bucket: String,
    /// Maximum object size in bytes (default 100 MB).
    pub max_object_size: u64,
    /// Whether to enable live GCS SDK calls. Default false for safe unit testing.
    /// When true, the adapter would use GCS credentials from the environment.
    /// When false, all phases fall back to shape-only validation.
    pub live: bool,
    /// Optional custom endpoint URL (e.g., `http://localhost:8080` for an emulator).
    pub endpoint_url: Option<String>,
    /// Optional GCP project ID. Not logged as a secret.
    pub project_id: Option<String>,
    /// Optional path to a service-account credentials file. The contents are
    /// never read or logged by this adapter; the path itself is not a secret.
    pub credentials_path: Option<String>,
}

impl Default for GcsConfig {
    fn default() -> Self {
        Self {
            allowed_bucket: String::new(),
            max_object_size: 100 * 1024 * 1024, // 100MB
            live: false,
            endpoint_url: None,
            project_id: None,
            credentials_path: None,
        }
    }
}

impl GcsConfig {
    /// Validates the configuration.
    /// Returns Ok if valid, or Err with a validation message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.allowed_bucket.is_empty() {
            return Err("allowed_bucket must be non-empty".to_string());
        }
        if !Self::is_valid_bucket_name(&self.allowed_bucket) {
            return Err(format!(
                "allowed_bucket '{}' is not a valid GCS bucket name",
                self.allowed_bucket
            ));
        }
        if self.max_object_size == 0 {
            return Err("max_object_size must be greater than 0".to_string());
        }
        if self.max_object_size > 5 * 1024 * 1024 * 1024 {
            return Err("max_object_size must be at most 5GB".to_string());
        }
        if let Some(ref endpoint) = self.endpoint_url {
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(format!(
                    "endpoint_url must start with http:// or https://, got: {}",
                    endpoint
                ));
            }
        }
        Ok(())
    }

    /// Validates that a string is a valid GCS bucket name.
    ///
    /// GCS bucket names follow DNS naming conventions similar to S3:
    /// - 3-63 characters
    /// - lowercase letters, numbers, hyphens, and periods only
    /// - must start and end with a letter or number
    /// - must not be a valid IP address
    /// - must not contain consecutive periods
    /// - must not contain a period adjacent to a hyphen
    pub fn is_valid_bucket_name(name: &str) -> bool {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9\.\-]{1,61}[a-z0-9]$").unwrap());
        if !re.is_match(name) {
            return false;
        }
        if name.contains("..") {
            return false;
        }
        if name.contains(".-") || name.contains("-.") {
            return false;
        }
        let ip_like = name.split('.').all(|part| part.parse::<u8>().is_ok());
        if ip_like && name.split('.').count() == 4 {
            return false;
        }
        true
    }

    /// Validates that an object key is safe for FerrumGate use.
    ///
    /// Rules enforced:
    /// - non-empty
    /// - max 1024 characters
    /// - does not contain `..` or start with `/` (prevents path traversal hints)
    /// - does not contain null bytes or control characters
    pub fn is_valid_object_key(key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        if key.len() > 1024 {
            return false;
        }
        if key.starts_with('/') {
            return false;
        }
        if key.contains("..") {
            return false;
        }
        for ch in key.chars() {
            if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Error)]
pub enum GcsAdapterError {
    #[error("invalid target: expected GcsObject, got {0}")]
    InvalidTarget(String),
    #[error("unsupported action type: {0}")]
    UnsupportedAction(String),
    #[error("unsupported check type: {0}")]
    UnsupportedCheck(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("bucket '{bucket}' is not in the allowlist (allowed: {allowed})")]
    BucketNotAllowed { bucket: String, allowed: String },
    #[error("invalid object key: {0}")]
    InvalidObjectKey(String),
    #[error("object size {size} exceeds maximum allowed size {max}")]
    ObjectSizeExceedsLimit { size: u64, max: u64 },
    #[error("rollback/compensate requires a before_generation in metadata")]
    MissingBeforeGeneration,
    #[error("rollback/compensate requires an after_generation in metadata")]
    MissingAfterGeneration,
    #[error("live GCS client is not implemented in this slice")]
    LiveClientNotImplemented,
}

impl From<GcsAdapterError> for AdapterError {
    fn from(err: GcsAdapterError) -> Self {
        match err {
            GcsAdapterError::InvalidTarget(msg) => AdapterError::Validation(msg),
            GcsAdapterError::UnsupportedAction(msg) => AdapterError::Unsupported(msg),
            GcsAdapterError::UnsupportedCheck(msg) => AdapterError::Unsupported(msg),
            GcsAdapterError::Validation(msg) => AdapterError::Validation(msg),
            GcsAdapterError::BucketNotAllowed { bucket, allowed } => {
                AdapterError::Validation(format!(
                    "bucket '{}' is not in allowlist (allowed: {})",
                    bucket, allowed
                ))
            }
            GcsAdapterError::InvalidObjectKey(key) => {
                AdapterError::Validation(format!("invalid object key: {}", key))
            }
            GcsAdapterError::ObjectSizeExceedsLimit { size, max } => {
                AdapterError::Validation(format!("object size {} exceeds limit {}", size, max))
            }
            GcsAdapterError::MissingBeforeGeneration => AdapterError::Validation(
                "rollback/compensate requires before_generation in metadata".into(),
            ),
            GcsAdapterError::MissingAfterGeneration => AdapterError::Validation(
                "rollback/compensate requires after_generation in metadata".into(),
            ),
            GcsAdapterError::LiveClientNotImplemented => {
                AdapterError::Internal("live GCS client is not implemented in this slice".into())
            }
        }
    }
}

/// Rollback metadata for GCS mutating operations.
///
/// Captures the generation/metageneration state before and after execution to
/// enable compensation by restoring the previous generation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GcsRollbackMetadata {
    /// Generation of the object before the mutating operation (if any).
    pub before_generation: Option<i64>,
    /// Metageneration of the object before the mutating operation (if any).
    pub before_metageneration: Option<i64>,
    /// Generation of the object after the mutating operation (if any).
    pub after_generation: Option<i64>,
    /// Metageneration of the object after the mutating operation (if any).
    pub after_metageneration: Option<i64>,
    /// Whether the execute operation created a delete marker instead of a generation.
    pub delete_marker: bool,
    /// Object key.
    pub object_key: String,
    /// Bucket name.
    pub bucket: String,
    /// Action type that was executed.
    pub action: String,
}

/// GCS adapter implementing the `RollbackAdapter` trait.
///
/// Provides validation, planning, and live GCS network execution with
/// generation-based rollback semantics.
///
/// The `gcs-client` feature enables the live GCS client path. Without it, the
/// adapter falls back to shape-only validation.
pub struct GcsAdapter {
    key: &'static str,
    config: GcsConfig,
    #[cfg(feature = "gcs-client")]
    client: std::sync::Mutex<Option<GcsLiveClient>>,
    #[cfg(not(feature = "gcs-client"))]
    _no_client: (),
}

/// Placeholder live GCS client.
///
/// The concrete GCS SDK integration is intentionally left as a follow-up slice
/// so this crate does not pull in heavy cloud SDK dependencies by default.
#[cfg(feature = "gcs-client")]
#[derive(Debug, Clone)]
pub struct GcsLiveClient;

impl GcsAdapter {
    /// Creates a new GcsAdapter with the given key and default configuration.
    ///
    /// **Important**: `allowed_bucket` must be set before use; the default is empty.
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            config: GcsConfig::default(),
            #[cfg(feature = "gcs-client")]
            client: std::sync::Mutex::new(None),
            #[cfg(not(feature = "gcs-client"))]
            _no_client: (),
        }
    }

    /// Creates a new GcsAdapter with explicit configuration.
    pub fn new_with_config(key: &'static str, config: GcsConfig) -> Self {
        Self {
            key,
            config,
            #[cfg(feature = "gcs-client")]
            client: std::sync::Mutex::new(None),
            #[cfg(not(feature = "gcs-client"))]
            _no_client: (),
        }
    }

    /// Returns a reference to the GCS configuration.
    pub fn config(&self) -> &GcsConfig {
        &self.config
    }

    /// Extracts the GCS object details from a `RollbackTarget::GcsObject` variant.
    fn extract_gcs_target(
        target: &RollbackTarget,
    ) -> Result<(&str, &str, Option<i64>), AdapterError> {
        match target {
            RollbackTarget::GcsObject {
                bucket,
                key,
                generation,
            } => Ok((bucket.as_str(), key.as_str(), *generation)),
            _ => Err(AdapterError::Validation(format!(
                "invalid target: expected GcsObject, got {:?}",
                target
            ))),
        }
    }

    /// Validates that the target bucket matches the allowlist.
    fn validate_bucket_allowlist(&self, bucket: &str) -> Result<(), GcsAdapterError> {
        if bucket != self.config.allowed_bucket {
            return Err(GcsAdapterError::BucketNotAllowed {
                bucket: bucket.to_string(),
                allowed: self.config.allowed_bucket.clone(),
            });
        }
        Ok(())
    }

    /// Validates an object key against FerrumGate safety rules.
    fn validate_object_key(key: &str) -> Result<(), GcsAdapterError> {
        if !GcsConfig::is_valid_object_key(key) {
            return Err(GcsAdapterError::InvalidObjectKey(key.to_string()));
        }
        Ok(())
    }

    /// Validates that an object size is within the configured limit.
    fn validate_object_size(&self, size: u64) -> Result<(), GcsAdapterError> {
        if size > self.config.max_object_size {
            return Err(GcsAdapterError::ObjectSizeExceedsLimit {
                size,
                max: self.config.max_object_size,
            });
        }
        Ok(())
    }

    /// Normalizes a validation error with phase context.
    fn phase_wrap_validation(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Validation(format!("[{}] {}", phase, msg))
    }

    /// Computes a content hash for payload integrity.
    fn compute_content_hash(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    /// Builds or returns the cached live GCS client.
    ///
    /// Returns Err if `live` is false. This allows unit tests to run without
    /// real credentials while still failing closed when a live client is
    /// configured but the GCS call is not yet implemented.
    #[cfg(feature = "gcs-client")]
    #[allow(dead_code)]
    async fn client(&self) -> Result<GcsLiveClient, AdapterError> {
        if !self.config.live {
            return Err(AdapterError::Validation(
                "GCS live mode is not enabled; set live=true to use live GCS calls".into(),
            ));
        }
        {
            let lock = self.client.lock().unwrap();
            if let Some(client) = lock.clone() {
                return Ok(client);
            }
        }
        // Shape-only slice: the live client is declared but not wired to a real SDK.
        Err(GcsAdapterError::LiveClientNotImplemented.into())
    }

    /// Fails closed if live mode is enabled.
    ///
    /// The live GCS SDK is not implemented in this slice; when live=true all
    /// phases must error instead of returning shape-only success receipts.
    fn ensure_shape_only(&self) -> Result<(), AdapterError> {
        if self.config.live {
            return Err(GcsAdapterError::LiveClientNotImplemented.into());
        }
        Ok(())
    }

    /// Runs a single check spec and returns an error if it fails.
    ///
    /// For live checks, uses the GCS client when available. In this slice the
    /// live path is not implemented, so only shape validation is performed.
    #[cfg(feature = "gcs-client")]
    async fn run_check_live(
        &self,
        check: &ferrum_proto::CheckSpec,
        bucket: &str,
        key: &str,
        _phase: &'static str,
    ) -> Result<(), AdapterError> {
        match check.check_type {
            CheckType::GcsObjectExists => {
                self.ensure_shape_only()?;
                Self::validate_check_bucket_key(check, bucket, key)?;
                Ok(())
            }
            CheckType::GcsGenerationMatches => {
                self.ensure_shape_only()?;
                Self::validate_check_bucket_key(check, bucket, key)?;
                let _expected = check.config.get("expected_generation").ok_or_else(|| {
                    AdapterError::Validation(
                        "GcsGenerationMatches check requires 'expected_generation' config".into(),
                    )
                })?;
                Ok(())
            }
            _ => Err(AdapterError::Unsupported(format!(
                "unsupported check type: {:?}",
                check.check_type
            ))),
        }
    }

    /// Shape-only fallback when `gcs-client` feature is disabled.
    #[cfg(not(feature = "gcs-client"))]
    async fn run_check_live(
        &self,
        check: &ferrum_proto::CheckSpec,
        bucket: &str,
        key: &str,
        _phase: &'static str,
    ) -> Result<(), AdapterError> {
        match check.check_type {
            CheckType::GcsObjectExists | CheckType::GcsGenerationMatches => {
                self.ensure_shape_only()?;
                Self::validate_check_bucket_key(check, bucket, key)?;
                Ok(())
            }
            _ => Err(AdapterError::Unsupported(format!(
                "unsupported check type: {:?}",
                check.check_type
            ))),
        }
    }

    /// Validates that a check's bucket/key fields match the target if present.
    fn validate_check_bucket_key(
        check: &ferrum_proto::CheckSpec,
        bucket: &str,
        key: &str,
    ) -> Result<(), AdapterError> {
        if let Some(serde_json::Value::String(check_bucket)) = check.config.get("bucket") {
            if check_bucket != bucket {
                return Err(AdapterError::Validation(format!(
                    "GCS check bucket mismatch: check targets '{}', expected '{}'",
                    check_bucket, bucket
                )));
            }
        }
        if let Some(serde_json::Value::String(check_key)) = check.config.get("key") {
            if check_key != key {
                return Err(AdapterError::Validation(format!(
                    "GCS check key mismatch: check targets '{}', expected '{}'",
                    check_key, key
                )));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl RollbackAdapter for GcsAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        self.ensure_shape_only()?;

        let (bucket, key, generation) = Self::extract_gcs_target(&request.target)?;

        match request.action_type {
            ActionType::GcsPutObject | ActionType::GcsDeleteObject | ActionType::GcsGetObject => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported action type: {:?}",
                    request.action_type
                )));
            }
        }

        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        for check in &request.prepare_checks {
            self.run_check_live(check, bucket, key, PHASE_PREPARE)
                .await?;
        }

        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "prepared_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "bucket".to_string(),
            serde_json::Value::String(bucket.to_string()),
        );
        metadata.insert(
            "object_key".to_string(),
            serde_json::Value::String(key.to_string()),
        );

        let before_generation = generation;
        metadata.insert(
            "before_generation".to_string(),
            before_generation
                .map(|g| serde_json::Value::Number(g.into()))
                .unwrap_or(serde_json::Value::Null),
        );
        metadata.insert("before_metageneration".to_string(), serde_json::Value::Null);
        metadata.insert(
            "action".to_string(),
            serde_json::Value::String(format!("{:?}", request.action_type)),
        );

        let rollback_meta = GcsRollbackMetadata {
            before_generation,
            before_metageneration: None,
            after_generation: None,
            after_metageneration: None,
            delete_marker: false,
            object_key: key.to_string(),
            bucket: bucket.to_string(),
            action: format!("{:?}", request.action_type),
        };
        metadata.insert(
            "rollback_metadata_v1".to_string(),
            serde_json::to_value(&rollback_meta).map_err(|e| {
                Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("failed to serialize rollback metadata: {}", e),
                )
            })?,
        );
        metadata.insert(
            "execution_groundwork".to_string(),
            serde_json::Value::Bool(true),
        );

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: metadata,
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        self.ensure_shape_only()?;

        let (bucket, key, _generation) = Self::extract_gcs_target(&contract.target)?;

        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
        }

        match contract.action_type {
            ActionType::GcsPutObject => {
                let content = Self::extract_payload_content(payload)?;
                if let Err(e) = self.validate_object_size(content.len() as u64) {
                    return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
                }

                let content_hash = Self::compute_content_hash(&content);
                let mut metadata = JsonMap::new();
                metadata.insert(
                    "adapter_kind".to_string(),
                    serde_json::Value::String(ADAPTER_KIND.to_string()),
                );
                metadata.insert(
                    "executed_at".to_string(),
                    serde_json::Value::String(Utc::now().to_rfc3339()),
                );
                metadata.insert(
                    "bucket".to_string(),
                    serde_json::Value::String(bucket.to_string()),
                );
                metadata.insert(
                    "object_key".to_string(),
                    serde_json::Value::String(key.to_string()),
                );
                metadata.insert(
                    "content_hash".to_string(),
                    serde_json::Value::String(content_hash.clone()),
                );
                metadata.insert(
                    "bytes_written".to_string(),
                    serde_json::Value::Number(content.len().into()),
                );

                // Live execution is not implemented in this slice.
                metadata.insert("after_generation".to_string(), serde_json::Value::Null);
                metadata.insert("after_metageneration".to_string(), serde_json::Value::Null);
                metadata.insert(
                    "execution_groundwork".to_string(),
                    serde_json::Value::Bool(true),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: Some(content_hash),
                    adapter_metadata: metadata,
                })
            }
            ActionType::GcsDeleteObject => {
                let mut metadata = JsonMap::new();
                metadata.insert(
                    "adapter_kind".to_string(),
                    serde_json::Value::String(ADAPTER_KIND.to_string()),
                );
                metadata.insert(
                    "executed_at".to_string(),
                    serde_json::Value::String(Utc::now().to_rfc3339()),
                );
                metadata.insert(
                    "bucket".to_string(),
                    serde_json::Value::String(bucket.to_string()),
                );
                metadata.insert(
                    "object_key".to_string(),
                    serde_json::Value::String(key.to_string()),
                );
                metadata.insert(
                    "delete_marker_generation".to_string(),
                    serde_json::Value::Null,
                );
                metadata.insert(
                    "execution_groundwork".to_string(),
                    serde_json::Value::Bool(true),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            ActionType::GcsGetObject => {
                let mut metadata = JsonMap::new();
                metadata.insert(
                    "adapter_kind".to_string(),
                    serde_json::Value::String(ADAPTER_KIND.to_string()),
                );
                metadata.insert(
                    "executed_at".to_string(),
                    serde_json::Value::String(Utc::now().to_rfc3339()),
                );
                metadata.insert(
                    "bucket".to_string(),
                    serde_json::Value::String(bucket.to_string()),
                );
                metadata.insert(
                    "object_key".to_string(),
                    serde_json::Value::String(key.to_string()),
                );
                metadata.insert(
                    "execution_groundwork".to_string(),
                    serde_json::Value::Bool(true),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            _ => Err(AdapterError::Unsupported(format!(
                "[execute] unsupported action type: {:?}",
                contract.action_type
            ))),
        }
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        self.ensure_shape_only()?;

        let (bucket, key, _generation) = Self::extract_gcs_target(&contract.target)?;

        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_VERIFY, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(PHASE_VERIFY, e.to_string()));
        }

        if contract.verify_checks.is_empty() {
            return Err(Self::phase_wrap_validation(
                PHASE_VERIFY,
                "no verify_checks provided and no default verification available for GCS operations. \
                 Provide verify_checks with GcsObjectExists or GcsGenerationMatches to confirm \
                 the operation had the expected effect."
                    .to_string(),
            ));
        }

        for check in &contract.verify_checks {
            self.run_check_live(check, bucket, key, PHASE_VERIFY)
                .await?;
        }

        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        self.rollback_or_compensate(contract, PHASE_COMPENSATE)
            .await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        self.rollback_or_compensate(contract, PHASE_ROLLBACK).await
    }
}

impl GcsAdapter {
    /// Extracts a string content payload from the execute payload.
    fn extract_payload_content(payload: &serde_json::Value) -> Result<Vec<u8>, AdapterError> {
        if let Some(obj) = payload.as_object() {
            if let Some(content_val) = obj.get("content") {
                if let Some(content_str) = content_val.as_str() {
                    return Ok(content_str.as_bytes().to_vec());
                }
                return Err(AdapterError::Validation(
                    "GcsPutObject payload 'content' must be a string".into(),
                ));
            } else if let Some(body_val) = obj.get("body") {
                if let Some(body_str) = body_val.as_str() {
                    return Ok(body_str.as_bytes().to_vec());
                }
                return Err(AdapterError::Validation(
                    "GcsPutObject payload 'body' must be a string".into(),
                ));
            }
            return Err(AdapterError::Validation(
                "GcsPutObject payload must contain 'content' or 'body'".into(),
            ));
        } else if let Some(content_str) = payload.as_str() {
            return Ok(content_str.as_bytes().to_vec());
        }
        Err(AdapterError::Validation(
            "GcsPutObject payload must be a string or object with 'content'/'body'".into(),
        ))
    }

    /// Shared rollback/compensate logic for generation-based recovery.
    ///
    /// For mutating operations (PutObject, DeleteObject), the recovery strategy
    /// is to restore the object to `before_generation`. When `live: true`, the
    /// adapter would issue a GCS copy or delete with `ifGenerationMatch`
    /// preconditions. In this slice, the live path is not implemented, so the
    /// method always returns `recovered=false` unless the action is read-only.
    async fn rollback_or_compensate(
        &self,
        contract: &RollbackContract,
        phase: &'static str,
    ) -> Result<RecoveryReceipt, AdapterError> {
        self.ensure_shape_only()?;

        let (bucket, key, _generation) = Self::extract_gcs_target(&contract.target)?;

        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(phase, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(phase, e.to_string()));
        }

        match contract.action_type {
            ActionType::GcsPutObject | ActionType::GcsDeleteObject => {
                let before_generation = contract
                    .metadata
                    .get("before_generation")
                    .and_then(|v| if v.is_null() { None } else { v.as_i64() });
                let after_generation = contract
                    .metadata
                    .get("after_generation")
                    .and_then(|v| if v.is_null() { None } else { v.as_i64() });

                if before_generation.is_none() {
                    return Err(GcsAdapterError::MissingBeforeGeneration.into());
                }
                if after_generation.is_none() {
                    return Err(GcsAdapterError::MissingAfterGeneration.into());
                }

                // Live rollback would use ifGenerationMatch preconditions here.
                // Shape-only slice: report not recovered.
                let mut metadata = JsonMap::new();
                metadata.insert(
                    "adapter_kind".to_string(),
                    serde_json::Value::String(ADAPTER_KIND.to_string()),
                );
                metadata.insert(
                    "phase".to_string(),
                    serde_json::Value::String(phase.to_string()),
                );
                metadata.insert("recovered".to_string(), serde_json::Value::Bool(false));
                metadata.insert(
                    "reason".to_string(),
                    serde_json::Value::String(
                        "GCS rollback/compensate requires a live client; live GCS is not implemented in this slice"
                            .to_string(),
                    ),
                );
                metadata.insert(
                    "before_generation".to_string(),
                    before_generation
                        .map(|g| serde_json::Value::Number(g.into()))
                        .unwrap_or(serde_json::Value::Null),
                );
                metadata.insert(
                    "after_generation".to_string(),
                    after_generation
                        .map(|g| serde_json::Value::Number(g.into()))
                        .unwrap_or(serde_json::Value::Null),
                );
                metadata.insert(
                    "bucket".to_string(),
                    serde_json::Value::String(bucket.to_string()),
                );
                metadata.insert(
                    "object_key".to_string(),
                    serde_json::Value::String(key.to_string()),
                );

                Ok(RecoveryReceipt {
                    recovered: false,
                    adapter_metadata: metadata,
                })
            }
            ActionType::GcsGetObject => Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: JsonMap::new(),
            }),
            _ => Err(AdapterError::Unsupported(format!(
                "[{}] unsupported action type for rollback/compensate: {:?}",
                phase, contract.action_type
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{ExecutionId, IntentId, ProposalId, RollbackClass};

    fn make_test_request(action_type: ActionType) -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "gcs".to_string(),
            target: RollbackTarget::GcsObject {
                bucket: "my-test-bucket".to_string(),
                key: "path/to/object.txt".to_string(),
                generation: Some(1234567890),
            },
            prepare_checks: Vec::new(),
            verify_checks: Vec::new(),
            compensation_plan: Vec::new(),
            auto_commit: false,
            metadata: JsonMap::new(),
        }
    }

    fn make_test_contract(action_type: ActionType) -> RollbackContract {
        RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "gcs".to_string(),
            target: RollbackTarget::GcsObject {
                bucket: "my-test-bucket".to_string(),
                key: "path/to/object.txt".to_string(),
                generation: Some(1234567890),
            },
            prepare_checks: Vec::new(),
            verify_checks: Vec::new(),
            compensation_plan: Vec::new(),
            auto_commit: false,
            state: ferrum_proto::RollbackState::PendingPrepare,
            created_at: Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        }
    }

    #[tokio::test]
    async fn test_gcs_prepare_put_object_accepted() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let request = make_test_request(ActionType::GcsPutObject);
        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
        assert_eq!(
            receipt.adapter_metadata.get("adapter_kind").unwrap(),
            "ferrum-adapter-gcs"
        );
        assert_eq!(
            receipt.adapter_metadata.get("bucket").unwrap(),
            "my-test-bucket"
        );
        assert_eq!(
            receipt.adapter_metadata.get("object_key").unwrap(),
            "path/to/object.txt"
        );
        assert_eq!(
            receipt.adapter_metadata.get("before_generation").unwrap(),
            1234567890_i64
        );
    }

    #[tokio::test]
    async fn test_gcs_prepare_rejects_disallowed_bucket() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "other-bucket".to_string(),
                ..Default::default()
            },
        );
        let request = make_test_request(ActionType::GcsPutObject);
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("other-bucket"),
            "error should mention allowed bucket: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_prepare_rejects_invalid_key() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut request = make_test_request(ActionType::GcsPutObject);
        request.target = RollbackTarget::GcsObject {
            bucket: "my-test-bucket".to_string(),
            key: "../etc/passwd".to_string(),
            generation: None,
        };
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid object key"),
            "error should mention invalid key: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_prepare_rejects_unsupported_action() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut request = make_test_request(ActionType::GcsPutObject);
        request.action_type = ActionType::FileWrite;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unsupported action type"),
            "error should mention unsupported action: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_execute_put_object_returns_metadata() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::GcsPutObject);
        let payload = serde_json::json!({ "content": "hello world" });
        let receipt = adapter.execute(&contract, &payload).await.unwrap();
        assert!(receipt.result_digest.is_some());
        assert_eq!(
            receipt.adapter_metadata.get("bucket").unwrap(),
            "my-test-bucket"
        );
        assert_eq!(
            receipt.adapter_metadata.get("object_key").unwrap(),
            "path/to/object.txt"
        );
        assert_eq!(
            receipt.adapter_metadata.get("bytes_written").unwrap(),
            11 // "hello world"
        );
    }

    #[tokio::test]
    async fn test_gcs_execute_put_object_rejects_oversized_payload() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                max_object_size: 5,
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::GcsPutObject);
        let payload = serde_json::json!({ "content": "hello world" });
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("exceeds maximum allowed size"),
            "error should mention size limit: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_execute_delete_object_returns_metadata() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::GcsDeleteObject);
        let receipt = adapter
            .execute(&contract, &serde_json::Value::Null)
            .await
            .unwrap();
        assert!(receipt.result_digest.is_none());
        assert_eq!(
            receipt.adapter_metadata.get("object_key").unwrap(),
            "path/to/object.txt"
        );
    }

    #[tokio::test]
    async fn test_gcs_verify_fails_closed_without_checks() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut contract = make_test_contract(ActionType::GcsPutObject);
        contract.verify_checks = Vec::new();
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no verify_checks provided"),
            "error should mention missing checks: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_verify_with_gcs_object_exists_check() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut contract = make_test_contract(ActionType::GcsPutObject);
        contract.verify_checks = vec![ferrum_proto::CheckSpec {
            check_type: CheckType::GcsObjectExists,
            config: {
                let mut m = JsonMap::new();
                m.insert(
                    "bucket".to_string(),
                    serde_json::Value::String("my-test-bucket".to_string()),
                );
                m.insert(
                    "key".to_string(),
                    serde_json::Value::String("path/to/object.txt".to_string()),
                );
                m
            },
        }];
        let receipt = adapter.verify(&contract).await.unwrap();
        assert!(receipt.verified);
    }

    #[tokio::test]
    async fn test_gcs_rollback_put_object_fails_closed_without_generations() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::GcsPutObject);
        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("before_generation") || err.contains("after_generation"),
            "error should mention missing generation: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_rollback_get_object_returns_recovered() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::GcsGetObject);
        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
    }

    #[test]
    fn test_config_default_validates() {
        let config = GcsConfig {
            allowed_bucket: "my-bucket".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_rejects_empty_bucket() {
        let config = GcsConfig::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_rejects_invalid_bucket_name() {
        let config = GcsConfig {
            allowed_bucket: "My_Bucket".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_rejects_zero_max_size() {
        let config = GcsConfig {
            allowed_bucket: "my-bucket".to_string(),
            max_object_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_rejects_oversized_max_size() {
        let config = GcsConfig {
            allowed_bucket: "my-bucket".to_string(),
            max_object_size: 6 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_valid_bucket_names() {
        assert!(GcsConfig::is_valid_bucket_name("my-bucket"));
        assert!(GcsConfig::is_valid_bucket_name("my.bucket"));
        assert!(GcsConfig::is_valid_bucket_name("my-bucket-123"));
        assert!(!GcsConfig::is_valid_bucket_name("a"));
        assert!(!GcsConfig::is_valid_bucket_name("ab"));
        assert!(!GcsConfig::is_valid_bucket_name("my_bucket"));
        assert!(!GcsConfig::is_valid_bucket_name("My-Bucket"));
        assert!(!GcsConfig::is_valid_bucket_name("my..bucket"));
        assert!(!GcsConfig::is_valid_bucket_name("my.-bucket"));
        assert!(!GcsConfig::is_valid_bucket_name("192.168.1.1"));
    }

    #[test]
    fn test_valid_object_keys() {
        assert!(GcsConfig::is_valid_object_key("path/to/object.txt"));
        assert!(GcsConfig::is_valid_object_key("a"));
        assert!(!GcsConfig::is_valid_object_key(""));
        assert!(!GcsConfig::is_valid_object_key("/leading-slash"));
        assert!(!GcsConfig::is_valid_object_key("path/../etc"));
        assert!(!GcsConfig::is_valid_object_key(&"a".repeat(1025)));
    }

    #[test]
    fn test_compute_content_hash() {
        let hash1 = GcsAdapter::compute_content_hash(b"hello");
        let hash2 = GcsAdapter::compute_content_hash(b"hello");
        let hash3 = GcsAdapter::compute_content_hash(b"world");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_rollback_metadata_serialization() {
        let meta = GcsRollbackMetadata {
            before_generation: Some(1),
            before_metageneration: Some(2),
            after_generation: Some(3),
            after_metageneration: Some(4),
            delete_marker: false,
            object_key: "test.txt".to_string(),
            bucket: "bucket".to_string(),
            action: "GcsPutObject".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("1"));
        assert!(json.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_gcs_prepare_fails_closed_when_live_true() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                live: true,
                ..Default::default()
            },
        );
        let request = make_test_request(ActionType::GcsPutObject);
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not implemented"),
            "live=true prepare must fail closed: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_execute_fails_closed_when_live_true() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                live: true,
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::GcsPutObject);
        let payload = serde_json::json!({ "content": "hello world" });
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not implemented"),
            "live=true execute must fail closed: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_verify_fails_closed_when_live_true() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                live: true,
                ..Default::default()
            },
        );
        let mut contract = make_test_contract(ActionType::GcsPutObject);
        contract.verify_checks = vec![ferrum_proto::CheckSpec {
            check_type: CheckType::GcsObjectExists,
            config: {
                let mut m = JsonMap::new();
                m.insert(
                    "bucket".to_string(),
                    serde_json::Value::String("my-test-bucket".to_string()),
                );
                m.insert(
                    "key".to_string(),
                    serde_json::Value::String("path/to/object.txt".to_string()),
                );
                m
            },
        }];
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not implemented"),
            "live=true verify must fail closed: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_gcs_rollback_fails_closed_when_live_true() {
        let adapter = GcsAdapter::new_with_config(
            "gcs",
            GcsConfig {
                allowed_bucket: "my-test-bucket".to_string(),
                live: true,
                ..Default::default()
            },
        );
        let mut contract = make_test_contract(ActionType::GcsPutObject);
        contract.metadata.insert(
            "before_generation".to_string(),
            serde_json::Value::Number(1234567890_i64.into()),
        );
        contract.metadata.insert(
            "after_generation".to_string(),
            serde_json::Value::Number(1234567891_i64.into()),
        );
        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not implemented"),
            "live=true rollback must fail closed: {}",
            err
        );
    }
}
