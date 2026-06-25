//! S3 adapter for mutation and recovery.
//!
//! This adapter implements the `RollbackAdapter` trait for S3-compatible object operations,
//! supporting prepare→verify lifecycle with versioning-based rollback semantics.
//!
//! # Recovery Slice (PR4)
//!
//! The adapter supports bounded S3 object operations with **versioning-based rollback**:
//! - `prepare`: validates bucket/key against allowlist, checks that versioning is enabled,
//!   and captures the current version ID (if the object exists) as `before_version_id`.
//! - `execute`: performs the S3 operation (put, delete, get, copy). For mutating operations,
//!   captures the resulting version ID / delete marker as `after_version_id`.
//! - `rollback`/`compensate`: for **mutating** operations, restores the previous state by
//!   deleting the newly created version / delete marker, making the `before_version_id`
//!   the current version again.
//!
//! # Limitations
//!
//! - Live S3 execution is implemented behind the `live` config flag. When `live: true`,
//!   real AWS SDK calls are made and failures fail closed. When `live: false` (default),
//!   the adapter falls back to shape-only validation for safe unit testing.
//! - Single-bucket allowlist only; no bucket creation/IAM/ACL admin.
//! - Max object size is enforced at the adapter boundary, not by S3 itself.
//! - Multipart upload, lifecycle, presigned URLs, replication, and batch deletion are out of scope.
//! - MinIO integration tests exist but are gated (`#[ignored]`) and require a local Docker container.

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
pub use planner::PlannableS3Adapter;

pub const ADAPTER_KIND: &str = "ferrum-adapter-s3";

/// Phase context for error normalization.
const PHASE_PREPARE: &str = "prepare";
#[allow(dead_code)]
const PHASE_VERIFY: &str = "verify";
const PHASE_EXECUTE: &str = "execute";
const PHASE_ROLLBACK: &str = "rollback";
#[allow(dead_code)]
const PHASE_COMPENSATE: &str = "compensate";

/// Configuration for S3 operation bounds.
///
/// Provides safety limits: single-bucket allowlist, max object size,
/// versioning requirement, and optional custom endpoint for MinIO/local testing.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Allowed bucket name (exact match). All operations must target this bucket.
    pub allowed_bucket: String,
    /// Maximum object size in bytes (default 100 MB).
    pub max_object_size: u64,
    /// Whether to require versioning on the target bucket (default true).
    pub require_versioning: bool,
    /// Optional custom endpoint URL (e.g., `http://localhost:9000` for MinIO).
    pub endpoint_url: Option<String>,
    /// Region (default "us-east-1").
    pub region: String,
    /// Whether to enable live S3 SDK calls. Default false for safe unit testing.
    /// When true, the adapter uses aws-config from_env (supports env vars, IAM, etc.).
    /// When false, all phases fall back to shape-only validation.
    pub live: bool,
    /// Optional AWS access key ID for static credentials (MinIO/dev).
    pub access_key_id: Option<String>,
    /// Optional AWS secret access key for static credentials (MinIO/dev).
    pub secret_access_key: Option<String>,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            allowed_bucket: String::new(),
            max_object_size: 100 * 1024 * 1024, // 100MB
            require_versioning: true,
            endpoint_url: None,
            region: "us-east-1".to_string(),
            live: false,
            access_key_id: None,
            secret_access_key: None,
        }
    }
}

impl S3Config {
    /// Validates the configuration.
    /// Returns Ok if valid, or Err with validation message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.allowed_bucket.is_empty() {
            return Err("allowed_bucket must be non-empty".to_string());
        }
        if !Self::is_valid_bucket_name(&self.allowed_bucket) {
            return Err(format!(
                "allowed_bucket '{}' is not a valid S3 bucket name",
                self.allowed_bucket
            ));
        }
        if self.max_object_size == 0 {
            return Err("max_object_size must be greater than 0".to_string());
        }
        if self.max_object_size > 5 * 1024 * 1024 * 1024 {
            return Err("max_object_size must be at most 5GB".to_string());
        }
        if self.region.is_empty() {
            return Err("region must be non-empty".to_string());
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

    /// Validates that a string is a valid S3 bucket name according to AWS naming rules.
    ///
    /// Rules enforced:
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
        // No consecutive periods
        if name.contains("..") {
            return false;
        }
        // No period adjacent to hyphen
        if name.contains(".-") || name.contains("-.") {
            return false;
        }
        // Must not be a valid IP address (simple heuristic: 4 dot-separated numbers)
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
        // Reject null bytes and control characters (except tab, newline, carriage return)
        for ch in key.chars() {
            if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Error)]
pub enum S3AdapterError {
    #[error("invalid target: expected S3Object, got {0}")]
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
    #[error("versioning is required but not confirmed for bucket '{0}'")]
    VersioningRequired(String),
    #[error("rollback/compensate requires a before_version_id in metadata")]
    MissingBeforeVersionId,
    #[error("rollback/compensate requires an after_version_id or delete_marker in metadata")]
    MissingAfterVersionId,
}

impl From<S3AdapterError> for AdapterError {
    fn from(err: S3AdapterError) -> Self {
        match err {
            S3AdapterError::InvalidTarget(msg) => AdapterError::Validation(msg),
            S3AdapterError::UnsupportedAction(msg) => AdapterError::Unsupported(msg),
            S3AdapterError::UnsupportedCheck(msg) => AdapterError::Unsupported(msg),
            S3AdapterError::Validation(msg) => AdapterError::Validation(msg),
            S3AdapterError::BucketNotAllowed { bucket, allowed } => {
                AdapterError::Validation(format!(
                    "bucket '{}' is not in allowlist (allowed: {})",
                    bucket, allowed
                ))
            }
            S3AdapterError::InvalidObjectKey(key) => {
                AdapterError::Validation(format!("invalid object key: {}", key))
            }
            S3AdapterError::ObjectSizeExceedsLimit { size, max } => {
                AdapterError::Validation(format!("object size {} exceeds limit {}", size, max))
            }
            S3AdapterError::VersioningRequired(bucket) => AdapterError::Validation(format!(
                "versioning is required but not confirmed for bucket '{}'",
                bucket
            )),
            S3AdapterError::MissingBeforeVersionId => AdapterError::Validation(
                "rollback/compensate requires before_version_id in metadata".into(),
            ),
            S3AdapterError::MissingAfterVersionId => AdapterError::Validation(
                "rollback/compensate requires after_version_id or delete_marker in metadata".into(),
            ),
        }
    }
}

/// Rollback metadata for S3 mutating operations.
///
/// Captures the versioning state before and after execution to enable
/// compensation by deleting the new version / delete marker.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct S3RollbackMetadata {
    /// Version ID of the object before the mutating operation (if any).
    pub before_version_id: Option<String>,
    /// Version ID of the object after the mutating operation (if any).
    pub after_version_id: Option<String>,
    /// Whether the execute operation created a delete marker instead of a version.
    pub delete_marker: bool,
    /// Object key.
    pub object_key: String,
    /// Bucket name.
    pub bucket: String,
    /// Action type that was executed.
    pub action: String,
}

/// S3 adapter implementing the `RollbackAdapter` trait.
///
/// Provides validation, planning, and live S3 network execution with
/// versioning-based rollback semantics.
///
/// The `s3-client` feature enables the AWS SDK for live S3 calls.
/// Without it, the adapter falls back to shape-only validation.
pub struct S3Adapter {
    key: &'static str,
    config: S3Config,
    #[cfg(feature = "s3-client")]
    client: std::sync::Mutex<Option<aws_sdk_s3::Client>>,
    #[cfg(not(feature = "s3-client"))]
    _no_client: (),
}

impl S3Adapter {
    /// Creates a new S3Adapter with the given key and default configuration.
    ///
    /// **Important**: `allowed_bucket` must be set before use; the default is empty.
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            config: S3Config::default(),
            #[cfg(feature = "s3-client")]
            client: std::sync::Mutex::new(None),
            #[cfg(not(feature = "s3-client"))]
            _no_client: (),
        }
    }

    /// Creates a new S3Adapter with explicit configuration.
    pub fn new_with_config(key: &'static str, config: S3Config) -> Self {
        Self {
            key,
            config,
            #[cfg(feature = "s3-client")]
            client: std::sync::Mutex::new(None),
            #[cfg(not(feature = "s3-client"))]
            _no_client: (),
        }
    }

    /// Returns a reference to the S3 configuration.
    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// Extracts the S3 object details from a `RollbackTarget::S3Object` variant.
    fn extract_s3_target(
        target: &RollbackTarget,
    ) -> Result<(&str, &str, Option<&str>), AdapterError> {
        match target {
            RollbackTarget::S3Object {
                bucket,
                key,
                version_id,
            } => Ok((bucket.as_str(), key.as_str(), version_id.as_deref())),
            _ => Err(AdapterError::Validation(format!(
                "invalid target: expected S3Object, got {:?}",
                target
            ))),
        }
    }

    /// Validates that the target bucket matches the allowlist.
    fn validate_bucket_allowlist(&self, bucket: &str) -> Result<(), S3AdapterError> {
        if bucket != self.config.allowed_bucket {
            return Err(S3AdapterError::BucketNotAllowed {
                bucket: bucket.to_string(),
                allowed: self.config.allowed_bucket.clone(),
            });
        }
        Ok(())
    }

    /// Validates an object key against FerrumGate safety rules.
    fn validate_object_key(key: &str) -> Result<(), S3AdapterError> {
        if !S3Config::is_valid_object_key(key) {
            return Err(S3AdapterError::InvalidObjectKey(key.to_string()));
        }
        Ok(())
    }

    /// Validates that an object size is within the configured limit.
    fn validate_object_size(&self, size: u64) -> Result<(), S3AdapterError> {
        if size > self.config.max_object_size {
            return Err(S3AdapterError::ObjectSizeExceedsLimit {
                size,
                max: self.config.max_object_size,
            });
        }
        Ok(())
    }

    /// Validates that versioning is required (and would be confirmed for a live client).
    /// In this slice, we enforce the config flag but do not make a live HEAD request.
    #[cfg(feature = "s3-client")]
    async fn validate_versioning_requirement_live(
        &self,
        bucket: &str,
    ) -> Result<(), S3AdapterError> {
        if self.config.require_versioning {
            match self.client().await {
                Ok(client) => {
                    let resp = client
                        .get_bucket_versioning()
                        .bucket(bucket.to_string())
                        .send()
                        .await;
                    match resp {
                        Ok(output) => {
                            let status = output.status();
                            if !matches!(
                                status,
                                Some(aws_sdk_s3::types::BucketVersioningStatus::Enabled)
                            ) {
                                return Err(S3AdapterError::VersioningRequired(bucket.to_string()));
                            }
                        }
                        Err(e) => {
                            return Err(S3AdapterError::Validation(format!(
                                "get_bucket_versioning failed for bucket '{}': {}",
                                bucket, e
                            )));
                        }
                    }
                }
                Err(_) => {
                    // Client not configured for live S3; skip live check.
                    // Shape-only validation is acceptable only when the adapter
                    // is not wired to a real endpoint/credentials.
                }
            }
        }
        Ok(())
    }

    /// No-op fallback when `s3-client` feature is disabled.
    #[cfg(not(feature = "s3-client"))]
    async fn validate_versioning_requirement_live(
        &self,
        _bucket: &str,
    ) -> Result<(), S3AdapterError> {
        Ok(())
    }

    /// Normalizes a validation error with phase context.
    fn phase_wrap_validation(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Validation(format!("[{}] {}", phase, msg))
    }

    /// Normalizes an internal error with phase context.
    #[allow(dead_code)]
    fn phase_wrap_internal(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Internal(format!("[{}] {}", phase, msg))
    }

    /// Computes a content hash for payload size validation.
    fn compute_content_hash(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    /// Builds or returns the cached AWS S3 client.
    ///
    /// Returns Err if `live` is false. This allows unit tests to run without
    /// real credentials while still failing closed when a live client is
    /// configured but the S3 call fails.
    ///
    /// When `live` is true, `aws_config::from_env()` is used, which supports
    /// explicit credentials, environment variables, and the AWS default
    /// credential chain (including IAM roles).
    #[cfg(feature = "s3-client")]
    async fn client(&self) -> Result<aws_sdk_s3::Client, AdapterError> {
        if !self.config.live {
            return Err(AdapterError::Validation(
                "S3 live mode is not enabled; set live=true to use live S3 calls".into(),
            ));
        }
        {
            let lock = self.client.lock().unwrap();
            if let Some(client) = lock.clone() {
                return Ok(client);
            }
        }
        let mut aws_cfg = aws_config::from_env()
            .region(aws_sdk_s3::config::Region::new(self.config.region.clone()));
        if let Some(ref endpoint) = self.config.endpoint_url {
            aws_cfg = aws_cfg.endpoint_url(endpoint.clone());
        }
        if let (Some(key), Some(secret)) =
            (&self.config.access_key_id, &self.config.secret_access_key)
        {
            let creds = aws_credential_types::Credentials::new(
                key.clone(),
                secret.clone(),
                None,
                None,
                "ferrum-adapter-s3",
            );
            aws_cfg = aws_cfg.credentials_provider(creds);
        }
        let shared_cfg = aws_cfg.load().await;
        let mut s3_builder = aws_sdk_s3::config::Builder::from(&shared_cfg);
        if self.config.endpoint_url.is_some() {
            s3_builder = s3_builder.force_path_style(true);
        }
        let client = aws_sdk_s3::Client::from_conf(s3_builder.build());
        let mut lock = self.client.lock().unwrap();
        *lock = Some(client.clone());
        Ok(client)
    }

    /// Runs a single check spec and returns an error if it fails.
    ///
    /// For live checks, uses the S3 client when available.
    #[cfg(feature = "s3-client")]
    async fn run_check_live(
        &self,
        check: &ferrum_proto::CheckSpec,
        bucket: &str,
        key: &str,
        _phase: &'static str,
    ) -> Result<(), AdapterError> {
        match check.check_type {
            CheckType::S3ObjectExists => {
                // Validate 'bucket' field if present
                if let Some(serde_json::Value::String(check_bucket)) = check.config.get("bucket") {
                    if check_bucket != bucket {
                        return Err(AdapterError::Validation(format!(
                            "S3ObjectExists check bucket mismatch: check targets '{}', expected '{}'",
                            check_bucket, bucket
                        )));
                    }
                }
                // Validate 'key' field if present
                if let Some(serde_json::Value::String(check_key)) = check.config.get("key") {
                    if check_key != key {
                        return Err(AdapterError::Validation(format!(
                            "S3ObjectExists check key mismatch: check targets '{}', expected '{}'",
                            check_key, key
                        )));
                    }
                }
                // Live check: try HeadObject if client available
                if let Ok(client) = self.client().await {
                    match client.head_object().bucket(bucket).key(key).send().await {
                        Ok(_) => Ok(()),
                        Err(e) => Err(AdapterError::Validation(format!(
                            "S3ObjectExists live check failed for {}/{}: {}",
                            bucket, key, e
                        ))),
                    }
                } else {
                    Ok(())
                }
            }
            CheckType::S3VersionIdMatches => {
                // Validate 'bucket' and 'key' fields if present
                if let Some(serde_json::Value::String(check_bucket)) = check.config.get("bucket") {
                    if check_bucket != bucket {
                        return Err(AdapterError::Validation(format!(
                            "S3VersionIdMatches check bucket mismatch: check targets '{}', expected '{}'",
                            check_bucket, bucket
                        )));
                    }
                }
                if let Some(serde_json::Value::String(check_key)) = check.config.get("key") {
                    if check_key != key {
                        return Err(AdapterError::Validation(format!(
                            "S3VersionIdMatches check key mismatch: check targets '{}', expected '{}'",
                            check_key, key
                        )));
                    }
                }
                let expected = check.config.get("expected_version_id").ok_or_else(|| {
                    AdapterError::Validation(
                        "S3VersionIdMatches check requires 'expected_version_id' config".into(),
                    )
                })?;
                // Live check: try HeadObject if client available
                if let Ok(client) = self.client().await {
                    match client.head_object().bucket(bucket).key(key).send().await {
                        Ok(resp) => {
                            let actual = resp.version_id();
                            let expected_str = expected.as_str().unwrap_or("");
                            if actual != Some(expected_str) {
                                return Err(AdapterError::Validation(format!(
                                    "S3VersionIdMatches mismatch: expected '{}', got '{:?}'",
                                    expected_str, actual
                                )));
                            }
                            Ok(())
                        }
                        Err(e) => Err(AdapterError::Validation(format!(
                            "S3VersionIdMatches live check failed for {}/{}: {}",
                            bucket, key, e
                        ))),
                    }
                } else {
                    Ok(())
                }
            }
            _ => Err(AdapterError::Unsupported(format!(
                "unsupported check type: {:?}",
                check.check_type
            ))),
        }
    }

    /// Shape-only fallback when `s3-client` feature is disabled.
    #[cfg(not(feature = "s3-client"))]
    async fn run_check_live(
        &self,
        check: &ferrum_proto::CheckSpec,
        bucket: &str,
        key: &str,
        _phase: &'static str,
    ) -> Result<(), AdapterError> {
        match check.check_type {
            CheckType::S3ObjectExists | CheckType::S3VersionIdMatches => {
                // Validate 'bucket' and 'key' fields if present
                if let Some(serde_json::Value::String(check_bucket)) = check.config.get("bucket") {
                    if check_bucket != bucket {
                        return Err(AdapterError::Validation(format!(
                            "S3ObjectExists check bucket mismatch: check targets '{}', expected '{}'",
                            check_bucket, bucket
                        )));
                    }
                }
                if let Some(serde_json::Value::String(check_key)) = check.config.get("key") {
                    if check_key != key {
                        return Err(AdapterError::Validation(format!(
                            "S3ObjectExists check key mismatch: check targets '{}', expected '{}'",
                            check_key, key
                        )));
                    }
                }
                Ok(())
            }
            _ => Err(AdapterError::Unsupported(format!(
                "unsupported check type: {:?}",
                check.check_type
            ))),
        }
    }
}

#[async_trait]
impl RollbackAdapter for S3Adapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate target is S3Object
        let (bucket, key, version_id) = Self::extract_s3_target(&request.target)?;

        // Validate that action_type is one of the four S3 operations
        match request.action_type {
            ActionType::S3PutObject
            | ActionType::S3DeleteObject
            | ActionType::S3GetObject
            | ActionType::S3CopyObject => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported action type: {:?}",
                    request.action_type
                )));
            }
        }

        // Validate bucket allowlist
        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        // Validate object key
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        // Validate versioning requirement (live if client available, config-only otherwise)
        if let Err(e) = self.validate_versioning_requirement_live(bucket).await {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        // Run prepare_checks if present
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

        // Capture before_version_id via live HeadObject if client available
        #[cfg(feature = "s3-client")]
        let before_version_id = if let Ok(client) = self.client().await {
            match client.head_object().bucket(bucket).key(key).send().await {
                Ok(resp) => resp.version_id().map(String::from),
                Err(e) => {
                    tracing::debug!("head_object failed during prepare: {}", e);
                    version_id.map(String::from)
                }
            }
        } else {
            version_id.map(String::from)
        };
        #[cfg(not(feature = "s3-client"))]
        let before_version_id = version_id.map(String::from);
        if let Some(ref vid) = before_version_id {
            metadata.insert(
                "before_version_id".to_string(),
                serde_json::Value::String(vid.clone()),
            );
        } else {
            metadata.insert("before_version_id".to_string(), serde_json::Value::Null);
        }

        // Store action type for rollback/compensate routing
        metadata.insert(
            "action".to_string(),
            serde_json::Value::String(format!("{:?}", request.action_type)),
        );

        // Store rollback metadata as JSON blob
        let rollback_meta = S3RollbackMetadata {
            before_version_id: version_id.map(String::from),
            after_version_id: None,
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

        // Mark execution as groundwork for this slice
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
        let (bucket, key, _version_id) = Self::extract_s3_target(&contract.target)?;

        // Revalidate bucket and key
        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
        }

        match contract.action_type {
            ActionType::S3PutObject => {
                // Validate payload shape: expect object with "content" (base64 or string) or "body"
                let content = if let Some(obj) = payload.as_object() {
                    if let Some(content_val) = obj.get("content") {
                        if let Some(content_str) = content_val.as_str() {
                            content_str.as_bytes().to_vec()
                        } else {
                            return Err(AdapterError::Validation(
                                "S3PutObject payload 'content' must be a string".into(),
                            ));
                        }
                    } else if let Some(body_val) = obj.get("body") {
                        if let Some(body_str) = body_val.as_str() {
                            body_str.as_bytes().to_vec()
                        } else {
                            return Err(AdapterError::Validation(
                                "S3PutObject payload 'body' must be a string".into(),
                            ));
                        }
                    } else {
                        return Err(AdapterError::Validation(
                            "S3PutObject payload must contain 'content' or 'body'".into(),
                        ));
                    }
                } else if let Some(content_str) = payload.as_str() {
                    content_str.as_bytes().to_vec()
                } else {
                    return Err(AdapterError::Validation(
                        "S3PutObject payload must be a string or object with 'content'/'body'"
                            .into(),
                    ));
                };

                // Validate size
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

                // Live execution: try PutObject if client available
                #[cfg(feature = "s3-client")]
                let after_version_id = if let Ok(client) = self.client().await {
                    let body = aws_sdk_s3::primitives::ByteStream::from(content.clone());
                    match client
                        .put_object()
                        .bucket(bucket)
                        .key(key)
                        .body(body)
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            let vid = resp.version_id().map(String::from);
                            metadata.insert(
                                "after_version_id".to_string(),
                                vid.clone()
                                    .map(serde_json::Value::String)
                                    .unwrap_or(serde_json::Value::Null),
                            );
                            metadata.insert(
                                "execution_groundwork".to_string(),
                                serde_json::Value::Bool(false),
                            );
                            vid
                        }
                        Err(e) => {
                            return Err(AdapterError::Validation(format!(
                                "S3 PutObject live execution failed: {}",
                                e
                            )));
                        }
                    }
                } else {
                    metadata.insert("after_version_id".to_string(), serde_json::Value::Null);
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    None
                };
                #[cfg(not(feature = "s3-client"))]
                let after_version_id = {
                    metadata.insert("after_version_id".to_string(), serde_json::Value::Null);
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    None
                };

                Ok(ExecuteReceipt {
                    external_id: after_version_id,
                    result_digest: Some(content_hash),
                    adapter_metadata: metadata,
                })
            }
            ActionType::S3DeleteObject => {
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

                // Live execution: try DeleteObject if client available
                #[cfg(feature = "s3-client")]
                let delete_marker_version_id = if let Ok(client) = self.client().await {
                    match client.delete_object().bucket(bucket).key(key).send().await {
                        Ok(resp) => {
                            let vid = resp.version_id().map(String::from);
                            let is_delete_marker = resp.delete_marker().unwrap_or(false);
                            metadata.insert(
                                "delete_marker_version_id".to_string(),
                                vid.clone()
                                    .map(serde_json::Value::String)
                                    .unwrap_or(serde_json::Value::Null),
                            );
                            metadata.insert(
                                "delete_marker".to_string(),
                                serde_json::Value::Bool(is_delete_marker),
                            );
                            metadata.insert(
                                "execution_groundwork".to_string(),
                                serde_json::Value::Bool(false),
                            );
                            vid
                        }
                        Err(e) => {
                            return Err(AdapterError::Validation(format!(
                                "S3 DeleteObject live execution failed: {}",
                                e
                            )));
                        }
                    }
                } else {
                    metadata.insert(
                        "delete_marker_version_id".to_string(),
                        serde_json::Value::Null,
                    );
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    None
                };
                #[cfg(not(feature = "s3-client"))]
                let delete_marker_version_id = {
                    metadata.insert(
                        "delete_marker_version_id".to_string(),
                        serde_json::Value::Null,
                    );
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    None
                };

                Ok(ExecuteReceipt {
                    external_id: delete_marker_version_id,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            ActionType::S3GetObject => {
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

                // Live execution: try GetObject if client available
                #[cfg(feature = "s3-client")]
                let mut result_digest = None;
                #[cfg(feature = "s3-client")]
                if let Ok(client) = self.client().await {
                    match client.get_object().bucket(bucket).key(key).send().await {
                        Ok(resp) => {
                            let data = resp.body.collect().await.map_err(|e| {
                                AdapterError::Internal(format!("S3 GetObject stream error: {}", e))
                            })?;
                            let bytes = data.into_bytes();
                            if bytes.len() as u64 > self.config.max_object_size {
                                return Err(AdapterError::Validation(format!(
                                    "S3 GetObject body exceeds max_object_size {}",
                                    self.config.max_object_size
                                )));
                            }
                            let hash = Self::compute_content_hash(&bytes);
                            metadata.insert(
                                "content_hash".to_string(),
                                serde_json::Value::String(hash.clone()),
                            );
                            metadata.insert(
                                "bytes_read".to_string(),
                                serde_json::Value::Number(bytes.len().into()),
                            );
                            metadata.insert(
                                "execution_groundwork".to_string(),
                                serde_json::Value::Bool(false),
                            );
                            result_digest = Some(hash);
                        }
                        Err(e) => {
                            return Err(AdapterError::Validation(format!(
                                "S3 GetObject live execution failed: {}",
                                e
                            )));
                        }
                    }
                } else {
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                }
                #[cfg(not(feature = "s3-client"))]
                let result_digest = None;
                #[cfg(not(feature = "s3-client"))]
                {
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                }

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest,
                    adapter_metadata: metadata,
                })
            }
            ActionType::S3CopyObject => {
                // Payload should contain the destination key
                let destination_key = if let Some(obj) = payload.as_object() {
                    obj.get("destination_key")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                } else {
                    None
                };

                let destination_key = destination_key.ok_or_else(|| {
                    AdapterError::Validation(
                        "S3CopyObject execute payload requires 'destination_key' field".into(),
                    )
                })?;

                if let Err(e) = Self::validate_object_key(&destination_key) {
                    return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
                }

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
                    "source_bucket".to_string(),
                    serde_json::Value::String(bucket.to_string()),
                );
                metadata.insert(
                    "source_key".to_string(),
                    serde_json::Value::String(key.to_string()),
                );
                metadata.insert(
                    "destination_key".to_string(),
                    serde_json::Value::String(destination_key.clone()),
                );

                // Live execution: try CopyObject if client available
                #[cfg(feature = "s3-client")]
                let after_version_id = if let Ok(client) = self.client().await {
                    let copy_source = format!("{}/{}", bucket, key);
                    match client
                        .copy_object()
                        .bucket(bucket)
                        .key(&destination_key)
                        .copy_source(copy_source)
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            let vid = resp.version_id().map(String::from);
                            metadata.insert(
                                "after_version_id".to_string(),
                                vid.clone()
                                    .map(serde_json::Value::String)
                                    .unwrap_or(serde_json::Value::Null),
                            );
                            metadata.insert(
                                "execution_groundwork".to_string(),
                                serde_json::Value::Bool(false),
                            );
                            vid
                        }
                        Err(e) => {
                            return Err(AdapterError::Validation(format!(
                                "S3 CopyObject live execution failed: {}",
                                e
                            )));
                        }
                    }
                } else {
                    metadata.insert("after_version_id".to_string(), serde_json::Value::Null);
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    None
                };
                #[cfg(not(feature = "s3-client"))]
                let after_version_id = {
                    metadata.insert("after_version_id".to_string(), serde_json::Value::Null);
                    metadata.insert(
                        "execution_groundwork".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    None
                };

                Ok(ExecuteReceipt {
                    external_id: after_version_id,
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
        let (bucket, key, _version_id) = Self::extract_s3_target(&contract.target)?;

        // Revalidate bucket and key
        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_VERIFY, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(PHASE_VERIFY, e.to_string()));
        }

        // If no verify_checks are provided, fail-closed.
        if contract.verify_checks.is_empty() {
            return Err(Self::phase_wrap_validation(
                PHASE_VERIFY,
                "no verify_checks provided and no default verification available for S3 operations. \
                 Provide verify_checks with S3ObjectExists or S3VersionIdMatches to confirm \
                 the operation had the expected effect."
                    .to_string(),
            ));
        }

        // Run explicit verify_checks with live client when available
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

impl S3Adapter {
    /// Shared rollback/compensate logic for versioning-based recovery.
    ///
    /// For mutating operations (PutObject, DeleteObject, CopyObject), the recovery
    /// strategy is:
    /// 1. Read `before_version_id` from prepare metadata.
    /// 2. Read `after_version_id` or `delete_marker_version_id` from execute metadata.
    /// 3. If there is an after version, delete that specific version to restore the before version.
    /// 4. If there is a delete marker, delete the delete marker to restore the object.
    ///
    /// When `live: true`, the adapter attempts a live S3 delete of the captured
    /// after version or delete marker; if the live delete fails, `recovered` is false.
    #[cfg(feature = "s3-client")]
    async fn rollback_or_compensate(
        &self,
        contract: &RollbackContract,
        phase: &'static str,
    ) -> Result<RecoveryReceipt, AdapterError> {
        let (bucket, key, _version_id) = Self::extract_s3_target(&contract.target)?;

        // Revalidate bucket and key
        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(phase, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(phase, e.to_string()));
        }

        match contract.action_type {
            ActionType::S3PutObject | ActionType::S3DeleteObject | ActionType::S3CopyObject => {
                // Extract rollback metadata from prepare and execute phases
                let before_version_id = contract
                    .metadata
                    .get("before_version_id")
                    .and_then(|v| if v.is_null() { None } else { v.as_str() });
                let after_version_id = contract
                    .metadata
                    .get("after_version_id")
                    .and_then(|v| if v.is_null() { None } else { v.as_str() });
                let delete_marker_version_id = contract
                    .metadata
                    .get("delete_marker_version_id")
                    .and_then(|v| if v.is_null() { None } else { v.as_str() });

                // Live rollback: delete the created version/delete marker if client available
                let mut recovered = false;
                if let Ok(client) = self.client().await {
                    let version_to_delete = after_version_id.or(delete_marker_version_id);
                    if let Some(vid) = version_to_delete {
                        let result = client
                            .delete_object()
                            .bucket(bucket)
                            .key(key)
                            .version_id(vid)
                            .send()
                            .await;
                        match result {
                            Ok(_) => {
                                recovered = true;
                            }
                            Err(e) => {
                                tracing::warn!("S3 rollback delete failed: {}", e);
                            }
                        }
                    }
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "adapter_kind".to_string(),
                    serde_json::Value::String(ADAPTER_KIND.to_string()),
                );
                metadata.insert(
                    "phase".to_string(),
                    serde_json::Value::String(phase.to_string()),
                );
                metadata.insert("recovered".to_string(), serde_json::Value::Bool(recovered));
                metadata.insert(
                    "reason".to_string(),
                    serde_json::Value::String(
                        if recovered {
                            "S3 rollback/compensate succeeded: deleted the created version/delete marker"
                                .to_string()
                        } else {
                            "S3 rollback/compensate did not delete a version; client unavailable or no after_version_id/delete_marker_version_id"
                                .to_string()
                        },
                    ),
                );
                metadata.insert(
                    "before_version_id".to_string(),
                    before_version_id
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .unwrap_or(serde_json::Value::Null),
                );
                metadata.insert(
                    "after_version_id".to_string(),
                    after_version_id
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .unwrap_or(serde_json::Value::Null),
                );
                metadata.insert(
                    "delete_marker_version_id".to_string(),
                    delete_marker_version_id
                        .map(|s| serde_json::Value::String(s.to_string()))
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
                    recovered,
                    adapter_metadata: metadata,
                })
            }
            ActionType::S3GetObject => {
                // Read-only operation: nothing to rollback
                Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: JsonMap::new(),
                })
            }
            _ => Err(AdapterError::Unsupported(format!(
                "[{}] unsupported action type for rollback/compensate: {:?}",
                phase, contract.action_type
            ))),
        }
    }

    /// Fallback when `s3-client` feature is disabled: no live delete possible.
    #[cfg(not(feature = "s3-client"))]
    async fn rollback_or_compensate(
        &self,
        contract: &RollbackContract,
        phase: &'static str,
    ) -> Result<RecoveryReceipt, AdapterError> {
        let (bucket, key, _version_id) = Self::extract_s3_target(&contract.target)?;

        if let Err(e) = self.validate_bucket_allowlist(bucket) {
            return Err(Self::phase_wrap_validation(phase, e.to_string()));
        }
        if let Err(e) = Self::validate_object_key(key) {
            return Err(Self::phase_wrap_validation(phase, e.to_string()));
        }

        match contract.action_type {
            ActionType::S3PutObject | ActionType::S3DeleteObject | ActionType::S3CopyObject => {
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
                        "S3 rollback/compensate requires s3-client feature for live delete"
                            .to_string(),
                    ),
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
            ActionType::S3GetObject => Ok(RecoveryReceipt {
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
    use ferrum_proto::{CheckSpec, ExecutionId, IntentId, ProposalId, RollbackClass};

    fn make_test_request(action_type: ActionType) -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "s3".to_string(),
            target: RollbackTarget::S3Object {
                bucket: "my-test-bucket".to_string(),
                key: "path/to/object.txt".to_string(),
                version_id: Some("abc123".to_string()),
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
            adapter_key: "s3".to_string(),
            target: RollbackTarget::S3Object {
                bucket: "my-test-bucket".to_string(),
                key: "path/to/object.txt".to_string(),
                version_id: Some("abc123".to_string()),
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
    async fn test_s3_prepare_put_object_accepted() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let request = make_test_request(ActionType::S3PutObject);
        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
        assert_eq!(
            receipt.adapter_metadata.get("adapter_kind").unwrap(),
            "ferrum-adapter-s3"
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
            receipt.adapter_metadata.get("before_version_id").unwrap(),
            "abc123"
        );
    }

    #[tokio::test]
    async fn test_s3_prepare_rejects_disallowed_bucket() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "other-bucket".to_string(),
                ..Default::default()
            },
        );
        let request = make_test_request(ActionType::S3PutObject);
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
    async fn test_s3_prepare_rejects_invalid_key() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut request = make_test_request(ActionType::S3PutObject);
        request.target = RollbackTarget::S3Object {
            bucket: "my-test-bucket".to_string(),
            key: "../etc/passwd".to_string(),
            version_id: None,
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
    async fn test_s3_prepare_rejects_unsupported_action() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut request = make_test_request(ActionType::S3PutObject);
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
    async fn test_s3_execute_put_object_returns_metadata() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::S3PutObject);
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
    async fn test_s3_execute_put_object_rejects_oversized_payload() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                max_object_size: 5,
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::S3PutObject);
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
    async fn test_s3_execute_delete_object_returns_metadata() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::S3DeleteObject);
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
    async fn test_s3_execute_copy_object_requires_destination_key() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::S3CopyObject);
        let payload = serde_json::json!({});
        let result = adapter.execute(&contract, &payload).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("destination_key"),
            "error should mention destination_key: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_s3_verify_fails_closed_without_checks() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut contract = make_test_contract(ActionType::S3PutObject);
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
    async fn test_s3_verify_with_s3_object_exists_check() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let mut contract = make_test_contract(ActionType::S3PutObject);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::S3ObjectExists,
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
    async fn test_s3_rollback_put_object_returns_not_recovered() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::S3PutObject);
        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(!receipt.recovered);
        let expected_reason = if cfg!(feature = "s3-client") {
            "S3 rollback/compensate did not delete a version; client unavailable or no after_version_id/delete_marker_version_id"
        } else {
            "S3 rollback/compensate requires s3-client feature for live delete"
        };
        assert_eq!(
            receipt
                .adapter_metadata
                .get("reason")
                .unwrap()
                .as_str()
                .unwrap(),
            expected_reason
        );
    }

    #[tokio::test]
    async fn test_s3_rollback_get_object_returns_recovered() {
        let adapter = S3Adapter::new_with_config(
            "s3",
            S3Config {
                allowed_bucket: "my-test-bucket".to_string(),
                ..Default::default()
            },
        );
        let contract = make_test_contract(ActionType::S3GetObject);
        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
    }

    #[test]
    fn test_config_default_validates() {
        let config = S3Config {
            allowed_bucket: "my-bucket".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_rejects_empty_bucket() {
        let config = S3Config::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_rejects_invalid_bucket_name() {
        let config = S3Config {
            allowed_bucket: "My_Bucket".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_rejects_zero_max_size() {
        let config = S3Config {
            allowed_bucket: "my-bucket".to_string(),
            max_object_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_rejects_oversized_max_size() {
        let config = S3Config {
            allowed_bucket: "my-bucket".to_string(),
            max_object_size: 6 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_valid_bucket_names() {
        assert!(S3Config::is_valid_bucket_name("my-bucket"));
        assert!(S3Config::is_valid_bucket_name("my.bucket"));
        assert!(S3Config::is_valid_bucket_name("my-bucket-123"));
        assert!(!S3Config::is_valid_bucket_name("a")); // min length is 3
        assert!(!S3Config::is_valid_bucket_name("ab")); // too short
        assert!(!S3Config::is_valid_bucket_name("my_bucket")); // underscore
        assert!(!S3Config::is_valid_bucket_name("My-Bucket")); // uppercase
        assert!(!S3Config::is_valid_bucket_name("my..bucket")); // consecutive dots
        assert!(!S3Config::is_valid_bucket_name("my.-bucket")); // dot adjacent to hyphen
        assert!(!S3Config::is_valid_bucket_name("192.168.1.1")); // IP-like
    }

    #[test]
    fn test_valid_object_keys() {
        assert!(S3Config::is_valid_object_key("path/to/object.txt"));
        assert!(S3Config::is_valid_object_key("a"));
        assert!(!S3Config::is_valid_object_key(""));
        assert!(!S3Config::is_valid_object_key("/leading-slash"));
        assert!(!S3Config::is_valid_object_key("path/../etc"));
        assert!(!S3Config::is_valid_object_key(&"a".repeat(1025)));
    }

    #[test]
    fn test_compute_content_hash() {
        let hash1 = S3Adapter::compute_content_hash(b"hello");
        let hash2 = S3Adapter::compute_content_hash(b"hello");
        let hash3 = S3Adapter::compute_content_hash(b"world");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_rollback_metadata_serialization() {
        let meta = S3RollbackMetadata {
            before_version_id: Some("v1".to_string()),
            after_version_id: None,
            delete_marker: false,
            object_key: "test.txt".to_string(),
            bucket: "bucket".to_string(),
            action: "S3PutObject".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("v1"));
        assert!(json.contains("test.txt"));
    }
}
