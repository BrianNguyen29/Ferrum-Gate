//! Asynchronous WORM-compatible audit bundle sink.
//!
//! The worker runs as a background task only when the `worm-sink` feature is
//! compiled **and** the gateway configuration enables it. It periodically scans
//! the audit log, builds portable bundles in the canonical format, and uploads
//! them to an S3-compatible Object Lock bucket.
//!
//! This is an **archival replica**, not a replacement for the database audit log,
//! and failures are best-effort: they are logged and counted but never block the
//! request path.

use async_trait::async_trait;
use chrono::Utc;
use ferrum_adapter_s3::{
    ObjectLockMode, WormUploadConfig, WormUploadError, WormUploadReceipt,
    WormUploader as S3WormUploader,
};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;

use crate::state::AppState;

/// Gateway-side configuration for the WORM audit sink.
#[derive(Clone)]
pub struct WormSinkConfig {
    /// Bucket where bundles are uploaded.
    pub bucket: String,
    /// Key prefix for uploaded bundles. Must not start with `/` or contain `..`.
    pub prefix: String,
    /// Object Lock mode: `governance` or `compliance`.
    pub object_lock_mode: ObjectLockMode,
    /// Retention period in days.
    pub retention_days: u32,
    /// Whether to apply a legal hold.
    pub legal_hold: bool,
    /// Interval between export attempts in seconds.
    pub export_interval_secs: u64,
    /// Maximum audit entries per bundle.
    pub batch_limit: u32,
    /// Whether live S3 SDK calls are enabled.
    pub live: bool,
    /// Optional custom S3 endpoint.
    pub endpoint_url: Option<String>,
    /// AWS region.
    pub region: String,
    /// Optional static access key ID.
    pub access_key_id: Option<String>,
    /// Optional static secret access key.
    pub secret_access_key: Option<String>,
}

impl std::fmt::Debug for WormSinkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WormSinkConfig")
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .field("object_lock_mode", &self.object_lock_mode)
            .field("retention_days", &self.retention_days)
            .field("legal_hold", &self.legal_hold)
            .field("export_interval_secs", &self.export_interval_secs)
            .field("batch_limit", &self.batch_limit)
            .field("live", &self.live)
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
            .finish()
    }
}

impl Default for WormSinkConfig {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            prefix: "audit-worm".to_string(),
            object_lock_mode: ObjectLockMode::Governance,
            retention_days: 30,
            legal_hold: false,
            export_interval_secs: 300,
            batch_limit: 1000,
            live: false,
            endpoint_url: None,
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
        }
    }
}

impl WormSinkConfig {
    /// Validate the gateway-side WORM sink configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.bucket.is_empty() {
            return Err("audit_worm_sink bucket must be non-empty".to_string());
        }
        if !ferrum_adapter_s3::S3Config::is_valid_bucket_name(&self.bucket) {
            return Err(format!(
                "audit_worm_sink bucket '{}' is invalid",
                self.bucket
            ));
        }
        if self.prefix.starts_with('/') {
            return Err("audit_worm_sink prefix must not start with '/'".to_string());
        }
        if self.prefix.contains("..") {
            return Err("audit_worm_sink prefix must not contain '..'".to_string());
        }
        if self.retention_days < 1 {
            return Err("audit_worm_sink retention_days must be at least 1".to_string());
        }
        if self.export_interval_secs < 60 {
            return Err("audit_worm_sink export_interval_secs must be at least 60".to_string());
        }
        if !(1..=100_000).contains(&self.batch_limit) {
            return Err(format!(
                "audit_worm_sink batch_limit must be between 1 and 100000, got {}",
                self.batch_limit
            ));
        }
        match self.object_lock_mode {
            ObjectLockMode::Governance | ObjectLockMode::Compliance => {}
        }
        if self.region.is_empty() {
            return Err("audit_worm_sink region must be non-empty".to_string());
        }
        if let Some(ref endpoint) = self.endpoint_url {
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(format!(
                    "audit_worm_sink endpoint_url must start with http:// or https://, got {}",
                    endpoint
                ));
            }
        }
        Ok(())
    }

    /// Build the S3 adapter upload configuration from this gateway configuration.
    pub fn to_upload_config(&self) -> WormUploadConfig {
        WormUploadConfig {
            bucket: self.bucket.clone(),
            endpoint_url: self.endpoint_url.clone(),
            region: self.region.clone(),
            access_key_id: self.access_key_id.clone(),
            secret_access_key: self.secret_access_key.clone(),
            object_lock_mode: self.object_lock_mode,
            retention_days: self.retention_days,
            legal_hold: self.legal_hold,
            live: self.live,
        }
    }
}

/// Abstraction over WORM upload operations so tests can inject failures.
#[async_trait]
pub trait WormUploader: Send + Sync {
    /// Validate that the target bucket is ready for Object Lock uploads.
    async fn validate_bucket(&self) -> Result<(), WormUploadError>;
    /// Upload an object and return a receipt.
    async fn upload_object(
        &self,
        key: &str,
        body: Vec<u8>,
    ) -> Result<WormUploadReceipt, WormUploadError>;
}

#[async_trait]
impl WormUploader for S3WormUploader {
    async fn validate_bucket(&self) -> Result<(), WormUploadError> {
        self.validate_bucket().await
    }
    async fn upload_object(
        &self,
        key: &str,
        body: Vec<u8>,
    ) -> Result<WormUploadReceipt, WormUploadError> {
        self.upload_object(key, body).await
    }
}

/// Build the S3 object key prefix for a bundle, including the user prefix and
/// deterministic bundle identifiers.
fn bundle_key(prefix: &str, first_id: i64, last_id: i64) -> String {
    let base = prefix.trim_end_matches('/');
    if base.is_empty() {
        format!("audit-bundle-{}-{}", first_id, last_id)
    } else {
        format!("{}/audit-bundle-{}-{}", base, first_id, last_id)
    }
}

/// Spawn and run the WORM sink worker.
///
/// The worker scans the audit log in ascending id order, builds portable
/// bundles, and uploads them. It advances an in-memory cursor only after a
/// successful upload. On restart, the cursor is reset to 0 and the worker will
/// re-scan from the beginning using deterministic keys, which produces
/// deterministic re-uploads for already-archived batches. Under S3 Object Lock +
/// versioning, those re-uploads create additional object versions rather than
/// overwriting existing objects. This avoids gaps on restart without requiring a
/// durable checkpoint store for this bounded slice.
///
/// Upload failures are logged and counted as a metric; they do not block the
/// request path.
pub async fn worm_sink_worker(
    state: Arc<AppState>,
    shutdown: Arc<tokio::sync::Notify>,
    config: WormSinkConfig,
    uploader: Arc<dyn WormUploader>,
) {
    let interval = Duration::from_secs(config.export_interval_secs);
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let last_id = AtomicI64::new(0);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e) = run_one_export_pass(&state, &config, &uploader, &last_id).await {
                    tracing::warn!(error = %e, "WORM sink export pass failed");
                }
            }
            _ = shutdown.notified() => {
                tracing::info!("WORM sink worker shutting down");
                break;
            }
        }
    }
}

async fn run_one_export_pass(
    state: &Arc<AppState>,
    config: &WormSinkConfig,
    uploader: &Arc<dyn WormUploader>,
    last_id: &AtomicI64,
) -> Result<(), WormSinkWorkerError> {
    let result = run_one_export_pass_inner(state, config, uploader, last_id).await;
    if result.is_err() {
        state
            .metrics
            .audit_worm_sink_failures_total
            .fetch_add(1, Ordering::Relaxed);
    }
    result
}

async fn run_one_export_pass_inner(
    state: &Arc<AppState>,
    config: &WormSinkConfig,
    uploader: &Arc<dyn WormUploader>,
    last_id: &AtomicI64,
) -> Result<(), WormSinkWorkerError> {
    let after_id = last_id.load(Ordering::Relaxed);
    let (entries, _next_cursor) = state
        .runtime
        .store
        .audit_log()
        .list_since_id(after_id, config.batch_limit)
        .await
        .map_err(|e| WormSinkWorkerError::Store(e.to_string()))?;

    if entries.is_empty() {
        return Ok(());
    }

    let first_id = entries.first().map(|e| e.id).unwrap_or(0);
    let last_id_value = entries.last().map(|e| e.id).unwrap_or(0);

    let body = entries
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| WormSinkWorkerError::Serialize(e.to_string()))?
        .join("\n");

    // The first hashed entry of this batch links back to the previous bundle's
    // last hash. Record that boundary so each bundle can be verified
    // independently without losing chain continuity.
    let previous_boundary_hash = entries
        .iter()
        .find(|e| e.content_hash.is_some())
        .and_then(|e| e.previous_hash.clone());

    let bundle =
        ferrum_audit_bundle::build_bundle_with_boundary(&entries, &body, previous_boundary_hash)
            .map_err(|e| WormSinkWorkerError::Bundle(e.to_string()))?;

    let base_key = bundle_key(&config.prefix, first_id, last_id_value);
    let jsonl_key = format!("{}/audit.jsonl", base_key);
    let manifest_key = format!("{}/manifest.json", base_key);

    let body_bytes = bundle.body.into_bytes();
    let manifest_bytes = serde_json::to_vec_pretty(&bundle.manifest)
        .map_err(|e| WormSinkWorkerError::Serialize(e.to_string()))?;

    // Upload both objects with Object Lock. The manifest is the authoritative
    // bundle marker.
    uploader
        .upload_object(&jsonl_key, body_bytes)
        .await
        .map_err(|e| WormSinkWorkerError::Upload(e.to_string()))?;
    uploader
        .upload_object(&manifest_key, manifest_bytes)
        .await
        .map_err(|e| WormSinkWorkerError::Upload(e.to_string()))?;

    last_id.store(last_id_value, Ordering::Relaxed);
    state
        .metrics
        .audit_worm_sink_exports_total
        .fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .audit_worm_sink_last_success_timestamp_seconds
        .store(Utc::now().timestamp() as u64, Ordering::Relaxed);

    tracing::info!(
        first_id,
        last_id = last_id_value,
        count = entries.len(),
        key = %base_key,
        "WORM sink audit bundle exported"
    );

    Ok(())
}

/// Errors that can occur during a WORM sink worker pass.
#[derive(Debug, thiserror::Error)]
pub enum WormSinkWorkerError {
    #[error("store error: {0}")]
    Store(String),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("bundle error: {0}")]
    Bundle(String),
    #[error("upload error: {0}")]
    Upload(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, GatewayRuntime, ServerConfig};
    use ferrum_adapter_s3::WormUploadError;
    use ferrum_store::{SqliteStore, StoreFacade};
    use std::sync::Mutex;
    use std::sync::atomic::AtomicU64;

    struct RecordingUploader {
        uploads: Mutex<Vec<(String, Vec<u8>)>>,
    }

    #[async_trait]
    impl WormUploader for RecordingUploader {
        async fn validate_bucket(&self) -> Result<(), WormUploadError> {
            Ok(())
        }
        async fn upload_object(
            &self,
            key: &str,
            body: Vec<u8>,
        ) -> Result<WormUploadReceipt, WormUploadError> {
            self.uploads.lock().unwrap().push((key.to_string(), body));
            Ok(WormUploadReceipt {
                bucket: "bucket".to_string(),
                key: key.to_string(),
                version_id: None,
                etag: None,
                retain_until: Utc::now(),
                mode: ObjectLockMode::Governance,
            })
        }
    }

    struct FailingUploader;

    #[async_trait]
    impl WormUploader for FailingUploader {
        async fn validate_bucket(&self) -> Result<(), WormUploadError> {
            Ok(())
        }
        async fn upload_object(
            &self,
            _key: &str,
            _body: Vec<u8>,
        ) -> Result<WormUploadReceipt, WormUploadError> {
            Err(WormUploadError::Client("injected upload failure".into()))
        }
    }

    struct CountingUploader {
        uploads: AtomicU64,
    }

    #[async_trait]
    impl WormUploader for CountingUploader {
        async fn validate_bucket(&self) -> Result<(), WormUploadError> {
            Ok(())
        }
        async fn upload_object(
            &self,
            _key: &str,
            _body: Vec<u8>,
        ) -> Result<WormUploadReceipt, WormUploadError> {
            self.uploads.fetch_add(1, Ordering::Relaxed);
            Ok(WormUploadReceipt {
                bucket: "bucket".to_string(),
                key: "key".to_string(),
                version_id: None,
                etag: None,
                retain_until: Utc::now(),
                mode: ObjectLockMode::Governance,
            })
        }
    }

    async fn test_state() -> Arc<AppState> {
        let sqlite = SqliteStore::connect("sqlite::memory:").await.unwrap();
        sqlite.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = Arc::new(sqlite);
        let runtime = GatewayRuntime::new(
            Arc::new(ferrum_pdp::StaticPdpEngine),
            Arc::new(ferrum_cap::InMemoryCapabilityService::default()),
            Arc::new(ferrum_rollback::RollbackService::new(Arc::new(
                ferrum_rollback::AdapterRegistry::default(),
            ))),
            store,
            vec![],
        );
        AppState::test_new(runtime, ServerConfig::default())
    }

    fn append_audit(state: &Arc<AppState>, actor_id: &str) {
        let store = state.runtime.store.clone();
        let actor_id = actor_id.to_string();
        tokio::spawn(async move {
            let entry = ferrum_proto::AuditLogEntry {
                id: 0,
                actor_id,
                action: ferrum_proto::AuditAction::TokenCreate,
                resource_type: ferrum_proto::AuditResourceType::Token,
                resource_id: "t1".to_string(),
                result: "ok".to_string(),
                metadata: None,
                created_at: Utc::now(),
                content_hash: None,
                previous_hash: None,
            };
            let _ = store.audit_log().append(&entry).await;
        });
    }

    #[test]
    fn test_bundle_key() {
        assert_eq!(bundle_key("prefix", 1, 10), "prefix/audit-bundle-1-10");
        assert_eq!(bundle_key("prefix/", 1, 10), "prefix/audit-bundle-1-10");
        assert_eq!(bundle_key("", 1, 10), "audit-bundle-1-10");
        assert_eq!(bundle_key("/", 1, 10), "audit-bundle-1-10");
    }

    #[test]
    fn test_worm_sink_config_validation() {
        let mut cfg = WormSinkConfig {
            bucket: "my-bucket".to_string(),
            prefix: "audit".to_string(),
            object_lock_mode: ObjectLockMode::Compliance,
            retention_days: 7,
            export_interval_secs: 60,
            batch_limit: 1000,
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());

        cfg.prefix = "/leading".to_string();
        assert!(cfg.validate().is_err());
        cfg.prefix = "a/../b".to_string();
        assert!(cfg.validate().is_err());

        cfg.prefix = "audit".to_string();
        cfg.retention_days = 0;
        assert!(cfg.validate().is_err());
        cfg.retention_days = 1;
        cfg.export_interval_secs = 59;
        assert!(cfg.validate().is_err());
        cfg.export_interval_secs = 60;
        cfg.batch_limit = 0;
        assert!(cfg.validate().is_err());
        cfg.batch_limit = 100_001;
        assert!(cfg.validate().is_err());
    }

    #[tokio::test]
    async fn test_upload_failure_increments_failure_metric() {
        let state = test_state().await;
        append_audit(&state, "alice");
        // Allow the spawned append to complete.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let before = state
            .metrics
            .audit_worm_sink_failures_total
            .load(Ordering::Relaxed);
        let cfg = WormSinkConfig {
            bucket: "my-bucket".to_string(),
            prefix: "audit".to_string(),
            object_lock_mode: ObjectLockMode::Governance,
            retention_days: 7,
            export_interval_secs: 60,
            batch_limit: 1000,
            live: false,
            ..Default::default()
        };
        let uploader = Arc::new(FailingUploader) as Arc<dyn WormUploader>;
        let last_id = AtomicI64::new(0);
        let _ = run_one_export_pass(&state, &cfg, &uploader, &last_id).await;
        let after = state
            .metrics
            .audit_worm_sink_failures_total
            .load(Ordering::Relaxed);
        assert_eq!(after, before + 1);
    }

    #[tokio::test]
    async fn test_success_advances_cursor_and_increments_export_metric() {
        let state = test_state().await;
        append_audit(&state, "alice");
        append_audit(&state, "bob");
        tokio::time::sleep(Duration::from_millis(50)).await;

        let uploader = Arc::new(CountingUploader {
            uploads: AtomicU64::new(0),
        });
        let uploader_dyn = uploader.clone() as Arc<dyn WormUploader>;
        let cfg = WormSinkConfig {
            bucket: "my-bucket".to_string(),
            prefix: "audit".to_string(),
            object_lock_mode: ObjectLockMode::Governance,
            retention_days: 7,
            export_interval_secs: 60,
            batch_limit: 1000,
            live: false,
            ..Default::default()
        };
        let last_id = AtomicI64::new(0);
        run_one_export_pass(&state, &cfg, &uploader_dyn, &last_id)
            .await
            .expect("export pass should succeed");

        assert_eq!(last_id.load(Ordering::Relaxed), 2);
        assert_eq!(
            uploader.uploads.load(Ordering::Relaxed),
            2,
            "expected jsonl + manifest uploads"
        );
        assert_eq!(
            state
                .metrics
                .audit_worm_sink_exports_total
                .load(Ordering::Relaxed),
            1
        );
        assert!(
            state
                .metrics
                .audit_worm_sink_last_success_timestamp_seconds
                .load(Ordering::Relaxed)
                > 0
        );
    }

    #[test]
    fn test_worm_sink_config_debug_redacts_credentials() {
        let cfg = WormSinkConfig {
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

    #[tokio::test]
    async fn test_empty_audit_log_is_no_op() {
        let state = test_state().await;
        let uploader = Arc::new(CountingUploader {
            uploads: AtomicU64::new(0),
        });
        let uploader_dyn = uploader.clone() as Arc<dyn WormUploader>;
        let cfg = WormSinkConfig {
            bucket: "my-bucket".to_string(),
            prefix: "audit".to_string(),
            ..Default::default()
        };
        let last_id = AtomicI64::new(0);
        run_one_export_pass(&state, &cfg, &uploader_dyn, &last_id)
            .await
            .expect("empty export pass should succeed");
        assert_eq!(uploader.uploads.load(Ordering::Relaxed), 0);
        assert_eq!(
            state
                .metrics
                .audit_worm_sink_exports_total
                .load(Ordering::Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn test_windowed_bundle_records_boundary_hash() {
        let state = test_state().await;

        let entry1 = ferrum_proto::AuditLogEntry {
            id: 0,
            actor_id: "alice".to_string(),
            action: ferrum_proto::AuditAction::TokenCreate,
            resource_type: ferrum_proto::AuditResourceType::Token,
            resource_id: "t1".to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: Utc::now(),
            content_hash: None,
            previous_hash: None,
        };
        let entry2 = ferrum_proto::AuditLogEntry {
            id: 0,
            actor_id: "bob".to_string(),
            action: ferrum_proto::AuditAction::TokenCreate,
            resource_type: ferrum_proto::AuditResourceType::Token,
            resource_id: "t2".to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: Utc::now(),
            content_hash: None,
            previous_hash: None,
        };
        state
            .runtime
            .store
            .audit_log()
            .append(&entry1)
            .await
            .unwrap();
        state
            .runtime
            .store
            .audit_log()
            .append(&entry2)
            .await
            .unwrap();

        let uploader = Arc::new(RecordingUploader {
            uploads: Mutex::new(Vec::new()),
        });
        let uploader_dyn = uploader.clone() as Arc<dyn WormUploader>;
        let cfg = WormSinkConfig {
            bucket: "my-bucket".to_string(),
            prefix: "audit".to_string(),
            object_lock_mode: ObjectLockMode::Governance,
            retention_days: 7,
            export_interval_secs: 60,
            batch_limit: 1,
            live: false,
            ..Default::default()
        };

        // First pass: export entry 1 as a full bundle.
        let last_id = AtomicI64::new(0);
        run_one_export_pass(&state, &cfg, &uploader_dyn, &last_id)
            .await
            .expect("first export pass should succeed");
        let first_manifest: ferrum_audit_bundle::AuditBundleManifest = {
            let uploads = uploader.uploads.lock().unwrap();
            uploads
                .iter()
                .find(|(k, _)| k.ends_with("manifest.json"))
                .map(|(_, body)| serde_json::from_slice(body).unwrap())
                .expect("manifest uploaded")
        };
        assert!(first_manifest.previous_boundary_hash.is_none());

        // Second pass: export entry 2 as a windowed bundle.
        let uploader2 = Arc::new(RecordingUploader {
            uploads: Mutex::new(Vec::new()),
        });
        let uploader_dyn2 = uploader2.clone() as Arc<dyn WormUploader>;
        run_one_export_pass(&state, &cfg, &uploader_dyn2, &last_id)
            .await
            .expect("second export pass should succeed");
        let uploads2 = uploader2.uploads.lock().unwrap();
        let second_manifest: ferrum_audit_bundle::AuditBundleManifest = uploads2
            .iter()
            .find(|(k, _)| k.ends_with("manifest.json"))
            .map(|(_, body)| serde_json::from_slice(body).unwrap())
            .expect("manifest uploaded");
        assert_eq!(
            second_manifest.previous_boundary_hash,
            Some(first_manifest.last_hash)
        );
    }
}
