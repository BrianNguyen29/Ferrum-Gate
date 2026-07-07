//! S3 Object Lock WORM upload support.
//!
//! This module provides a small, bounded helper for uploading audit bundles to an
//! S3-compatible object store with Object Lock. It is **not** a general-purpose S3
//! admin tool: it does not create buckets, enable Object Lock, manage IAM, or
//! perform deletes/retention bypasses.
//!
//! Live SDK calls are gated by the `s3-client` feature. Without it, the module
//! falls back to shape validation and returns an explicit error if a live upload is
//! requested. This keeps unit tests safe without credentials.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

/// Supported S3 Object Lock modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ObjectLockMode {
    /// Governance mode allows privileged users to alter retention.
    Governance,
    /// Compliance mode is stricter and cannot be bypassed by any user.
    Compliance,
}

impl ObjectLockMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ObjectLockMode::Governance => "GOVERNANCE",
            ObjectLockMode::Compliance => "COMPLIANCE",
        }
    }
}

impl FromStr for ObjectLockMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "governance" => Ok(ObjectLockMode::Governance),
            "compliance" => Ok(ObjectLockMode::Compliance),
            _ => Err(format!(
                "invalid object_lock_mode '{}': expected 'governance' or 'compliance'",
                s
            )),
        }
    }
}

/// Configuration for a WORM/object-lock audit bundle upload.
#[derive(Clone)]
pub struct WormUploadConfig {
    /// Target bucket name.
    pub bucket: String,
    /// Optional custom endpoint (e.g., `http://localhost:9000` for MinIO).
    pub endpoint_url: Option<String>,
    /// AWS region (default "us-east-1").
    pub region: String,
    /// Optional static access key ID.
    pub access_key_id: Option<String>,
    /// Optional static secret access key.
    pub secret_access_key: Option<String>,
    /// Object Lock mode to apply to uploaded objects.
    pub object_lock_mode: ObjectLockMode,
    /// Retention period in days (must be >= 1).
    pub retention_days: u32,
    /// Whether to apply a legal hold on uploaded objects.
    pub legal_hold: bool,
    /// Whether live S3 SDK calls are enabled. When false, validation and upload
    /// return shape-only results or explicit errors.
    pub live: bool,
}

impl std::fmt::Debug for WormUploadConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WormUploadConfig")
            .field("bucket", &self.bucket)
            .field("endpoint_url", &self.endpoint_url)
            .field("region", &self.region)
            .field(
                "access_key_id",
                &self.access_key_id.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "secret_access_key",
                &self.secret_access_key.as_ref().map(|_| "<redacted>"),
            )
            .field("object_lock_mode", &self.object_lock_mode)
            .field("retention_days", &self.retention_days)
            .field("legal_hold", &self.legal_hold)
            .field("live", &self.live)
            .finish()
    }
}

impl Default for WormUploadConfig {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            endpoint_url: None,
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            object_lock_mode: ObjectLockMode::Governance,
            retention_days: 30,
            legal_hold: false,
            live: false,
        }
    }
}

impl WormUploadConfig {
    /// Validate the shape of the configuration. This does not make live network
    /// calls; use `WormUploader::validate_bucket` for live bucket checks.
    pub fn validate(&self) -> Result<(), WormUploadError> {
        if self.bucket.is_empty() {
            return Err(WormUploadError::Validation(
                "bucket must be non-empty".into(),
            ));
        }
        if !crate::S3Config::is_valid_bucket_name(&self.bucket) {
            return Err(WormUploadError::Validation(format!(
                "bucket '{}' is not a valid S3 bucket name",
                self.bucket
            )));
        }
        if self.retention_days < 1 {
            return Err(WormUploadError::Validation(
                "retention_days must be at least 1".into(),
            ));
        }
        if self.region.is_empty() {
            return Err(WormUploadError::Validation(
                "region must be non-empty".into(),
            ));
        }
        if let Some(ref endpoint) = self.endpoint_url {
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(WormUploadError::Validation(format!(
                    "endpoint_url must start with http:// or https://, got: {}",
                    endpoint
                )));
            }
        }
        Ok(())
    }
}

/// Receipt from a successful WORM upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormUploadReceipt {
    pub bucket: String,
    pub key: String,
    pub version_id: Option<String>,
    pub etag: Option<String>,
    pub retain_until: DateTime<Utc>,
    pub mode: ObjectLockMode,
}

/// Errors that can occur during WORM upload operations.
#[derive(Debug, Error)]
pub enum WormUploadError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("object lock is not enabled on bucket '{0}'")]
    ObjectLockNotEnabled(String),
    #[error("versioning is required but not enabled on bucket '{0}'")]
    VersioningRequired(String),
    #[error("S3 live mode is not enabled; set live=true to use live S3 calls")]
    LiveDisabled,
    #[error("S3 client error: {0}")]
    Client(String),
    #[error("invalid object key: {0}")]
    InvalidObjectKey(String),
}

/// WORM S3 uploader.
///
/// The uploader is intentionally small: it validates bucket/object-lock state
/// and uploads objects with Object Lock retention (and optional legal hold). It
/// does not create buckets, manage IAM, or delete objects.
pub struct WormUploader {
    config: WormUploadConfig,
    #[cfg(feature = "s3-client")]
    client: std::sync::Mutex<Option<aws_sdk_s3::Client>>,
    #[cfg(not(feature = "s3-client"))]
    _no_client: (),
}

impl WormUploader {
    /// Create a new uploader from the given configuration.
    pub fn new(config: WormUploadConfig) -> Self {
        Self {
            config,
            #[cfg(feature = "s3-client")]
            client: std::sync::Mutex::new(None),
            #[cfg(not(feature = "s3-client"))]
            _no_client: (),
        }
    }

    /// Returns a reference to the configuration.
    pub fn config(&self) -> &WormUploadConfig {
        &self.config
    }

    /// Validate that the target bucket has Object Lock and versioning enabled.
    ///
    /// When `live` is false, this returns Ok without making network calls. When
    /// live is true and the `s3-client` feature is enabled, it queries the bucket.
    /// If the `s3-client` feature is disabled, live validation returns
    /// `WormUploadError::LiveDisabled`.
    pub async fn validate_bucket(&self) -> Result<(), WormUploadError> {
        self.config.validate()?;

        #[cfg(feature = "s3-client")]
        {
            if !self.config.live {
                return Ok(());
            }
            let client = self.client().await?;

            match client
                .get_bucket_versioning()
                .bucket(self.config.bucket.clone())
                .send()
                .await
            {
                Ok(output) => {
                    if !matches!(
                        output.status(),
                        Some(aws_sdk_s3::types::BucketVersioningStatus::Enabled)
                    ) {
                        return Err(WormUploadError::VersioningRequired(
                            self.config.bucket.clone(),
                        ));
                    }
                }
                Err(e) => {
                    return Err(WormUploadError::Client(format!(
                        "get_bucket_versioning failed for bucket '{}': {}",
                        self.config.bucket, e
                    )));
                }
            }

            match client
                .get_object_lock_configuration()
                .bucket(self.config.bucket.clone())
                .send()
                .await
            {
                Ok(output) => {
                    if let Some(config) = output.object_lock_configuration() {
                        if !matches!(
                            config.object_lock_enabled(),
                            Some(aws_sdk_s3::types::ObjectLockEnabled::Enabled)
                        ) {
                            return Err(WormUploadError::ObjectLockNotEnabled(
                                self.config.bucket.clone(),
                            ));
                        }
                    } else {
                        return Err(WormUploadError::ObjectLockNotEnabled(
                            self.config.bucket.clone(),
                        ));
                    }
                }
                Err(e) => {
                    return Err(WormUploadError::Client(format!(
                        "get_object_lock_configuration failed for bucket '{}': {}",
                        self.config.bucket, e
                    )));
                }
            }

            Ok(())
        }

        #[cfg(not(feature = "s3-client"))]
        {
            if self.config.live {
                Err(WormUploadError::LiveDisabled)
            } else {
                Ok(())
            }
        }
    }

    /// Upload a bundle object with Object Lock retention.
    ///
    /// The caller provides the full S3 object key (including any prefix). The
    /// object is written with `object_lock_mode` and `object_lock_retain_until_date`
    /// computed from `retention_days`. When `legal_hold` is true, an Object Lock
    /// legal hold is also applied.
    ///
    /// Returns a receipt describing the uploaded object.
    pub async fn upload_object(
        &self,
        key: &str,
        body: Vec<u8>,
    ) -> Result<WormUploadReceipt, WormUploadError> {
        if !crate::S3Config::is_valid_object_key(key) {
            return Err(WormUploadError::InvalidObjectKey(key.to_string()));
        }
        self.config.validate()?;

        #[cfg(feature = "s3-client")]
        {
            if !self.config.live {
                return Err(WormUploadError::LiveDisabled);
            }
            let client = self.client().await?;
            let retain_until =
                Utc::now() + chrono::Duration::days(i64::from(self.config.retention_days));
            let sdk_mode = match self.config.object_lock_mode {
                ObjectLockMode::Governance => aws_sdk_s3::types::ObjectLockMode::Governance,
                ObjectLockMode::Compliance => aws_sdk_s3::types::ObjectLockMode::Compliance,
            };
            let retain_until_sdk = aws_sdk_s3::primitives::DateTime::from_secs_and_nanos(
                retain_until.timestamp(),
                retain_until.timestamp_subsec_nanos(),
            );
            let byte_stream = aws_sdk_s3::primitives::ByteStream::from(body);

            let mut request = client
                .put_object()
                .bucket(self.config.bucket.clone())
                .key(key)
                .body(byte_stream)
                .object_lock_mode(sdk_mode)
                .object_lock_retain_until_date(retain_until_sdk);
            if self.config.legal_hold {
                request = request.object_lock_legal_hold_status(
                    aws_sdk_s3::types::ObjectLockLegalHoldStatus::On,
                );
            }

            match request.send().await {
                Ok(output) => Ok(WormUploadReceipt {
                    bucket: self.config.bucket.clone(),
                    key: key.to_string(),
                    version_id: output.version_id().map(String::from),
                    etag: output.e_tag().map(String::from),
                    retain_until,
                    mode: self.config.object_lock_mode,
                }),
                Err(e) => Err(WormUploadError::Client(format!(
                    "S3 PutObject live execution failed for {}/{}: {}",
                    self.config.bucket, key, e
                ))),
            }
        }

        #[cfg(not(feature = "s3-client"))]
        {
            let _ = (key, body);
            Err(WormUploadError::LiveDisabled)
        }
    }

    #[cfg(feature = "s3-client")]
    async fn client(&self) -> Result<aws_sdk_s3::Client, WormUploadError> {
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
                "ferrum-adapter-s3-worm",
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_lock_mode_from_str() {
        assert_eq!(
            "governance".parse::<ObjectLockMode>().unwrap(),
            ObjectLockMode::Governance
        );
        assert_eq!(
            "compliance".parse::<ObjectLockMode>().unwrap(),
            ObjectLockMode::Compliance
        );
        assert!("invalid".parse::<ObjectLockMode>().is_err());
    }

    #[test]
    fn test_worm_config_validate_rejects_empty_bucket() {
        let cfg = WormUploadConfig::default();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_worm_config_validate_rejects_zero_retention() {
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            retention_days: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_worm_config_validate_rejects_invalid_bucket() {
        let cfg = WormUploadConfig {
            bucket: "My_Bucket".to_string(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_worm_config_validate_accepts_valid() {
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            retention_days: 7,
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[tokio::test]
    async fn test_validate_bucket_skips_when_not_live() {
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            live: false,
            ..Default::default()
        };
        let uploader = WormUploader::new(cfg);
        assert!(uploader.validate_bucket().await.is_ok());
    }

    #[tokio::test]
    #[cfg_attr(feature = "s3-client", ignore)]
    async fn test_validate_bucket_fails_when_live_without_s3_client() {
        // Only relevant when s3-client is disabled: live=true without SDK should
        // return LiveDisabled. This test is ignored when s3-client is enabled.
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            live: true,
            ..Default::default()
        };
        let uploader = WormUploader::new(cfg);
        assert!(matches!(
            uploader.validate_bucket().await,
            Err(WormUploadError::LiveDisabled)
        ));
    }

    #[tokio::test]
    async fn test_upload_object_rejects_invalid_key() {
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            ..Default::default()
        };
        let uploader = WormUploader::new(cfg);
        let result = uploader.upload_object("/leading-slash", vec![]).await;
        assert!(matches!(result, Err(WormUploadError::InvalidObjectKey(_))));
    }

    #[tokio::test]
    async fn test_upload_object_rejects_not_live() {
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            live: false,
            ..Default::default()
        };
        let uploader = WormUploader::new(cfg);
        let result = uploader.upload_object("audit/bundle.jsonl", vec![]).await;
        assert!(matches!(result, Err(WormUploadError::LiveDisabled)));
    }

    #[test]
    fn test_receipt_serializes() {
        let receipt = WormUploadReceipt {
            bucket: "my-bucket".to_string(),
            key: "audit/bundle.jsonl".to_string(),
            version_id: Some("v1".to_string()),
            etag: Some("abc123".to_string()),
            retain_until: Utc::now(),
            mode: ObjectLockMode::Governance,
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("my-bucket"));
        assert!(json.contains("v1"));
    }

    #[test]
    fn test_worm_upload_config_debug_redacts_credentials() {
        let cfg = WormUploadConfig {
            bucket: "my-bucket".to_string(),
            access_key_id: Some("AKIAEXAMPLE".to_string()),
            secret_access_key: Some("s3cr3t".to_string()),
            ..Default::default()
        };
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("my-bucket"));
        assert!(!debug.contains("AKIAEXAMPLE"));
        assert!(!debug.contains("s3cr3t"));
        assert!(debug.contains("<redacted>"));
    }
}
