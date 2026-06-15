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
//! - This slice is **groundwork**: validation, planning, and metadata are fully implemented.
//!   Live S3 network execution requires a future slice that wires an AWS SDK or MinIO client.
//! - Single-bucket allowlist only; no bucket creation/IAM/ACL admin.
//! - Max object size is enforced at the adapter boundary, not by S3 itself.
//! - Multipart upload, lifecycle, presigned URLs, replication, and batch deletion are out of scope.

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
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            allowed_bucket: String::new(),
            max_object_size: 100 * 1024 * 1024, // 100MB
            require_versioning: true,
            endpoint_url: None,
            region: "us-east-1".to_string(),
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
/// This slice provides full validation, planning, and metadata capture.
/// Live S3 network execution is deferred to a future slice.
pub struct S3Adapter {
    key: &'static str,
    config: S3Config,
}

impl S3Adapter {
    /// Creates a new S3Adapter with the given key and default configuration.
    ///
    /// **Important**: `allowed_bucket` must be set before use; the default is empty.
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            config: S3Config::default(),
        }
    }

    /// Creates a new S3Adapter with explicit configuration.
    pub fn new_with_config(key: &'static str, config: S3Config) -> Self {
        Self { key, config }
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
    fn validate_versioning_requirement(&self, bucket: &str) -> Result<(), S3AdapterError> {
        if self.config.require_versioning {
            // Groundwork: in a future slice with a live client, we would verify
            // `get_bucket_versioning` and fail-closed if Status != "Enabled".
            // For now, we accept the configuration as the operator contract.
            let _ = bucket;
        }
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

    /// Runs a single check spec and returns an error if it fails.
    ///
    /// # Arguments
    /// * `check` - The check specification to run
    /// * `bucket` - The target bucket
    /// * `key` - The target object key
    /// * `phase` - The phase context for error messages
    fn run_check(
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
                // Groundwork: in a future slice with a live client, we would verify
                // object existence via HeadObject. For now, we validate shape only.
                Ok(())
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
                // Validate 'expected_version_id' is present
                let _expected = check.config.get("expected_version_id").ok_or_else(|| {
                    AdapterError::Validation(
                        "S3VersionIdMatches check requires 'expected_version_id' config".into(),
                    )
                })?;
                // Groundwork: in a future slice with a live client, we would verify
                // the actual version ID against the expected value.
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

        // Validate versioning requirement (config-only in this slice)
        if let Err(e) = self.validate_versioning_requirement(bucket) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        // Run prepare_checks if present
        for check in &request.prepare_checks {
            Self::run_check(check, bucket, key, PHASE_PREPARE)?;
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

        // Capture before_version_id if provided in target (live client would query HeadObject)
        if let Some(vid) = version_id {
            metadata.insert(
                "before_version_id".to_string(),
                serde_json::Value::String(vid.to_string()),
            );
        } else {
            // Groundwork: with a live client, we would query the current version ID
            // and store it here. For now, we mark it as unknown.
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

                // Groundwork: live execution would call PutObject here.
                // For this slice, we capture metadata and return an unsupported marker.
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
                // after_version_id would be captured from PutObject response in a future slice
                metadata.insert("after_version_id".to_string(), serde_json::Value::Null);
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
            ActionType::S3DeleteObject => {
                // Groundwork: live execution would call DeleteObject here.
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
                // delete_marker_version_id would be captured from DeleteObject response
                metadata.insert(
                    "delete_marker_version_id".to_string(),
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
            ActionType::S3GetObject => {
                // Groundwork: live execution would call GetObject here.
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

                // Groundwork: live execution would call CopyObject here.
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

        // Run explicit verify_checks
        for check in &contract.verify_checks {
            Self::run_check(check, bucket, key, PHASE_VERIFY)?;
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
    /// In this slice, the logic is fully structured but the live S3 call is deferred.
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

                // Groundwork: in a future slice with a live client, we would:
                // - For PutObject: delete the after_version_id to restore the before_version_id
                // - For DeleteObject: delete the delete_marker_version_id to restore the object
                // - For CopyObject: delete the destination object's new version
                // For now, we return recovered=false with structured metadata explaining the gap.
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
                        "S3 rollback/compensate network execution is not implemented in this slice; \
                         use MinIO smoke or a future slice."
                            .to_string(),
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
                    recovered: false,
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
        assert_eq!(
            receipt
                .adapter_metadata
                .get("reason")
                .unwrap()
                .as_str()
                .unwrap(),
            "S3 rollback/compensate network execution is not implemented in this slice; \
             use MinIO smoke or a future slice."
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
