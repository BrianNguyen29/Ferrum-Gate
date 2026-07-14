use anyhow::{Context, Result};
use clap::Parser;
use ferrum_adapter_fs::{FsAdapter, FsBoundsConfig, PlannableFsAdapter};
#[cfg(feature = "gcs")]
use ferrum_adapter_gcs::{GcsAdapter, PlannableGcsAdapter};
use ferrum_adapter_git::{GitRollbackAdapter, PlannableGitAdapter};
use ferrum_adapter_http::{PlannableHttpAdapter, register_http_adapter};
use ferrum_adapter_maildraft::{PlannableMailDraftAdapter, register_maildraft_adapter};
#[cfg(feature = "s3")]
use ferrum_adapter_s3::{PlannableS3Adapter, S3Adapter};
use ferrum_adapter_sqlite::{PlannableSqliteAdapter, SqliteAdapter};
use ferrum_cap::CapabilityService;
use ferrum_gateway::{GatewayRuntime, NonceCacheBackend, run_http_server};
use ferrum_pdp::StaticPdpEngine;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
#[cfg(feature = "postgres")]
use ferrum_store::postgres::{PostgresNonceCache, PostgresStore};
use ferrum_store::{
    InMemoryNonceCache, LifecycleReconciliationReport, SqliteStore, SqliteWalTuning, StoreFacade,
};
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt};

mod config;
use config::{Args, redact_dsn_for_log, resolve_config};

async fn reconcile_lifecycle_outbox_before_startup(
    store: &Arc<dyn StoreFacade>,
) -> anyhow::Result<LifecycleReconciliationReport> {
    let report = ferrum_store::reconcile_lifecycle_outbox(store, 1_000)
        .await
        .context("failed to reconcile lifecycle outbox")?;
    tracing::info!(
        scanned = report.scanned,
        already_reconciled = report.already_reconciled,
        repaired_missing_provenance = report.repaired_missing_provenance,
        needs_operator_review = report.needs_operator_review,
        "lifecycle outbox reconciliation completed before HTTP startup"
    );
    Ok(report)
}

#[cfg(feature = "postgres")]
fn build_nonce_cache(
    config: &ferrum_gateway::ServerConfig,
    pg_store: Option<&ferrum_store::postgres::PostgresStore>,
) -> anyhow::Result<Arc<dyn ferrum_store::NonceCache>> {
    match config.nonce_cache_backend {
        NonceCacheBackend::Auto if pg_store.is_some() => Ok(Arc::new(PostgresNonceCache::new(
            pg_store.expect("pg_store checked above").pool().clone(),
        ))),
        NonceCacheBackend::Auto | NonceCacheBackend::Memory => Ok(Arc::new(
            InMemoryNonceCache::new(config.nonce_cache_max_entries),
        )),
        NonceCacheBackend::Postgres => {
            let store = pg_store.ok_or_else(|| {
                anyhow::anyhow!("nonce_cache_backend='postgres' requires a PostgreSQL store DSN")
            })?;
            Ok(Arc::new(PostgresNonceCache::new(store.pool().clone())))
        }
    }
}

#[cfg(not(feature = "postgres"))]
fn build_nonce_cache(
    config: &ferrum_gateway::ServerConfig,
    _pg_store: Option<&()>,
) -> anyhow::Result<Arc<dyn ferrum_store::NonceCache>> {
    match config.nonce_cache_backend {
        NonceCacheBackend::Postgres => Err(anyhow::anyhow!(
            "PostgreSQL nonce cache is not enabled. Build with --features postgres to enable it."
        )),
        _ => Ok(Arc::new(InMemoryNonceCache::new(
            config.nonce_cache_max_entries,
        ))),
    }
}

fn http_egress_enabled(config: &ferrum_gateway::ServerConfig) -> bool {
    config
        .http_egress
        .as_ref()
        .map(|c| !c.is_empty())
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Resolve configuration with precedence
    let config = resolve_config(&args)?;

    // Set up logging
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_filter));

    match config.log_format {
        ferrum_gateway::LogFormat::Json => {
            fmt()
                .with_env_filter(env_filter)
                .with_target(false)
                .json()
                .init();
        }
        ferrum_gateway::LogFormat::Text => {
            fmt().with_env_filter(env_filter).with_target(false).init();
        }
    }

    tracing::info!(
        "starting ferrumd with config: auth_mode={}, bind_addr={}, store_dsn={}, log_format={}, rate_limit_per_second={}, rate_limit_burst={}",
        config.auth_mode,
        config.bind_addr,
        redact_dsn_for_log(&config.store_dsn),
        config.log_format,
        config.rate_limit_per_second,
        config.rate_limit_burst
    );

    let pdp = Arc::new(StaticPdpEngine);

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let git_enabled = !config.git_repo_roots.is_empty();
    if git_enabled {
        for root in &config.git_repo_roots {
            std::fs::create_dir_all(root).with_context(|| {
                format!(
                    "failed to create configured Git repository root {}",
                    root.display()
                )
            })?;
        }
        registry.register(Arc::new(GitRollbackAdapter::new(
            config.git_repo_roots.clone(),
        )?));
    } else {
        tracing::warn!(
            "Git adapter not registered because git_repo_roots is empty; \
             set FERRUMD_GIT_REPO_ROOTS or server.git_repo_roots to enable bounded Git mutations"
        );
    }
    if let Some(workdir) = &config.fs_workdir {
        std::fs::create_dir_all(workdir).with_context(|| {
            format!(
                "failed to create filesystem adapter workdir {}",
                workdir.display()
            )
        })?;
        registry.register(Arc::new(FsAdapter::new_with_workdir(
            "fs",
            FsBoundsConfig::default(),
            workdir.clone(),
        )));
    } else {
        tracing::warn!(
            "filesystem adapter not registered because fs_workdir is not configured; \
             set FERRUMD_FS_WORKDIR or server.fs_workdir to enable bounded filesystem mutations"
        );
    }
    if http_egress_enabled(&config) {
        let http_egress = config.http_egress.as_ref().expect("checked above");
        register_http_adapter(&mut registry, http_egress)
            .map_err(|e| anyhow::anyhow!("failed to register HTTP adapter: {e}"))?;
        tracing::info!(
            "HTTP adapter registered for allowed hosts: {:?}",
            http_egress.allowed_hosts
        );
    } else {
        tracing::warn!(
            "HTTP adapter not registered because http_egress.allowed_hosts is empty; \
             set FERRUMD_HTTP_EGRESS_ALLOWED_HOSTS or server.http_egress.allowed_hosts to enable bounded HTTP mutations"
        );
    }
    let sqlite_adapter_enabled = !config.sqlite_db_roots.is_empty();
    if sqlite_adapter_enabled {
        for root in &config.sqlite_db_roots {
            std::fs::create_dir_all(root).with_context(|| {
                format!(
                    "failed to create configured SQLite database root {}",
                    root.display()
                )
            })?;
        }
        registry.register(Arc::new(SqliteAdapter::new(
            "sqlite",
            config.sqlite_db_roots.clone(),
        )?));
    } else {
        tracing::warn!(
            "SQLite mutation adapter not registered because sqlite_db_roots is empty; \
             set FERRUMD_SQLITE_DB_ROOTS or server.sqlite_db_roots to enable bounded SQLite mutations"
        );
    }
    register_maildraft_adapter(&mut registry);
    #[cfg(feature = "s3")]
    {
        if let Some(ref s3_cfg) = config.s3_config {
            if let Err(e) = s3_cfg.validate() {
                tracing::warn!("S3 config invalid; adapter not registered: {}", e);
            } else {
                registry.register(Arc::new(S3Adapter::new_with_config("s3", s3_cfg.clone())));
                tracing::info!(
                    "S3 adapter registered for bucket '{}'",
                    s3_cfg.allowed_bucket
                );
            }
        } else {
            tracing::warn!(
                "S3 adapter not registered because s3_config is not configured; \
                 set FERRUMD_S3_ALLOWED_BUCKET or server.s3_config.allowed_bucket to enable bounded S3 mutations"
            );
        }
    }
    #[cfg(not(feature = "s3"))]
    {
        tracing::info!("S3 adapter not registered because s3 feature is not enabled");
    }
    #[cfg(feature = "gcs")]
    {
        if let Some(ref gcs_cfg) = config.gcs_config {
            if let Err(e) = gcs_cfg.validate() {
                tracing::warn!("GCS config invalid; adapter not registered: {}", e);
            } else {
                registry.register(Arc::new(GcsAdapter::new_with_config(
                    "gcs",
                    gcs_cfg.clone(),
                )));
                tracing::info!(
                    "GCS adapter registered for bucket '{}'",
                    gcs_cfg.allowed_bucket
                );
            }
        } else {
            tracing::warn!(
                "GCS adapter not registered because gcs_config is not configured; \
                 set FERRUMD_GCS_ALLOWED_BUCKET or server.gcs_config.allowed_bucket to enable bounded GCS mutations"
            );
        }
    }
    #[cfg(not(feature = "gcs"))]
    {
        tracing::info!("GCS adapter not registered because gcs feature is not enabled");
    }
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    if sqlite_adapter_enabled {
        rollback_service.register_planner(Arc::new(PlannableSqliteAdapter));
    }
    rollback_service.register_planner(Arc::new(PlannableMailDraftAdapter));
    if git_enabled {
        rollback_service.register_planner(Arc::new(PlannableGitAdapter));
    }
    if http_egress_enabled(&config) {
        rollback_service.register_planner(Arc::new(PlannableHttpAdapter));
    }
    #[cfg(feature = "s3")]
    {
        if let Some(ref s3_cfg) = config.s3_config {
            if s3_cfg.validate().is_ok() {
                rollback_service.register_planner(Arc::new(PlannableS3Adapter));
                tracing::info!(
                    "S3 planner registered for bucket '{}'",
                    s3_cfg.allowed_bucket
                );
            }
        }
    }
    #[cfg(feature = "gcs")]
    {
        if let Some(ref gcs_cfg) = config.gcs_config {
            if gcs_cfg.validate().is_ok() {
                rollback_service.register_planner(Arc::new(PlannableGcsAdapter));
                tracing::info!(
                    "GCS planner registered for bucket '{}'",
                    gcs_cfg.allowed_bucket
                );
            }
        }
    }
    let rollback = Arc::new(rollback_service);

    let (store, nonce_cache): (Arc<dyn StoreFacade>, Arc<dyn ferrum_store::NonceCache>) = if config
        .store_dsn
        .to_lowercase()
        .starts_with("postgres://")
        || config.store_dsn.to_lowercase().starts_with("postgresql://")
    {
        #[cfg(feature = "postgres")]
        {
            let pg_pool_config = ferrum_store::postgres::PostgresPoolConfig {
                max_connections: config.pg_max_connections,
                min_idle: config.pg_min_idle,
                acquire_timeout_secs: config.pg_acquire_timeout_secs,
                statement_timeout_ms: config.pg_statement_timeout_ms,
                idle_in_transaction_timeout_ms: config.pg_idle_in_transaction_timeout_ms,
            };
            let pg_store = PostgresStore::connect_with_config(&config.store_dsn, pg_pool_config)
                .await
                .context("failed to connect to postgres")?;
            pg_store
                .apply_embedded_migrations()
                .await
                .context("failed to apply postgres migrations")?;
            let nonce_cache = build_nonce_cache(&config, Some(&pg_store))?;
            (Arc::new(pg_store) as Arc<dyn StoreFacade>, nonce_cache)
        }
        #[cfg(not(feature = "postgres"))]
        {
            // Unreachable in practice because validate_store_dsn rejects postgres DSNs
            // when the feature is not enabled.
            return Err(anyhow::anyhow!(
                "PostgreSQL support is not enabled. Build with --features postgres to enable it."
            ));
        }
    } else {
        let wal_tuning = SqliteWalTuning {
            synchronous: config.store_synchronous.clone(),
            wal_autocheckpoint: config.store_wal_autocheckpoint,
        };
        let sqlite_store = SqliteStore::connect_with_tuning(&config.store_dsn, wal_tuning)
            .await
            .context("failed to connect to sqlite")?;
        sqlite_store
            .apply_embedded_migrations()
            .await
            .context("failed to apply migrations")?;
        let nonce_cache = build_nonce_cache(&config, None)?;
        (Arc::new(sqlite_store) as Arc<dyn StoreFacade>, nonce_cache)
    };

    let cap: Arc<dyn CapabilityService> = Arc::new(ferrum_gateway::StoreCapabilityService::new(
        Arc::clone(&store),
    ));

    let reconciliation_report = reconcile_lifecycle_outbox_before_startup(&store).await?;

    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    let worker_handle = if config.lifecycle_reconciliation_enabled {
        let store_clone = Arc::clone(&store);
        let shutdown = Arc::clone(&shutdown_notify);
        let interval_secs = config.lifecycle_reconciliation_interval_secs;
        let batch_limit = config.lifecycle_reconciliation_batch_limit;
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match ferrum_store::reconcile_lifecycle_outbox(&store_clone, batch_limit)
                            .await
                        {
                            Ok(report) => {
                                if report.scanned > 0 {
                                    tracing::info!(
                                        scanned = report.scanned,
                                        already_reconciled = report.already_reconciled,
                                        repaired_missing_provenance = report.repaired_missing_provenance,
                                        needs_operator_review = report.needs_operator_review,
                                        "periodic lifecycle outbox reconciliation completed"
                                    );
                                }
                            }
                            Err(error) => {
                                tracing::error!(
                                    %error,
                                    "periodic lifecycle outbox reconciliation failed"
                                );
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("lifecycle reconciler shutting down");
                        break;
                    }
                }
            }
        }))
    } else {
        None
    };

    let nonce_reconciler_shutdown = Arc::new(tokio::sync::Notify::new());
    // The vacuum task is only needed when the resolved backend is PostgreSQL.
    // `Auto` resolves to Postgres when the store DSN is PostgreSQL.
    let is_postgres_nonce_cache = config.nonce_cache_backend == NonceCacheBackend::Postgres
        || (config.nonce_cache_backend == NonceCacheBackend::Auto
            && (config.store_dsn.to_lowercase().starts_with("postgres://")
                || config.store_dsn.to_lowercase().starts_with("postgresql://")));
    let nonce_reconciler_handle = if is_postgres_nonce_cache {
        let nonce_cache_clone = Arc::clone(&nonce_cache);
        let shutdown = Arc::clone(&nonce_reconciler_shutdown);
        // Mirror the quarantine reconciler interval for the periodic vacuum.
        let interval_secs = config.quarantine_reconciliation_interval_secs.max(60);
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match nonce_cache_clone.vacuum().await {
                            Ok(removed) => {
                                if removed > 0 {
                                    tracing::info!(
                                        removed,
                                        "nonce cache vacuum removed expired rows"
                                    );
                                }
                            }
                            Err(error) => {
                                tracing::error!(%error, "nonce cache vacuum failed");
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("nonce cache vacuum reconciler shutting down");
                        break;
                    }
                }
            }
        }))
    } else {
        None
    };

    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::clone(&store), vec![])
        .with_lifecycle_reconciliation_report(reconciliation_report);
    let result = run_http_server(config, runtime, nonce_cache).await;

    if let Some(handle) = worker_handle {
        shutdown_notify.notify_waiters();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    nonce_reconciler_shutdown.notify_waiters();
    if let Some(handle) = nonce_reconciler_handle {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    if let Err(e) = store.shutdown().await {
        tracing::warn!(error = %e, "store shutdown failed");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_adapter_http::HttpEgressConfig;

    #[tokio::test]
    async fn test_startup_reconciler_is_idempotent_without_pending_records() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let facade: Arc<dyn StoreFacade> = store;

        let first = reconcile_lifecycle_outbox_before_startup(&facade)
            .await
            .unwrap();
        let second = reconcile_lifecycle_outbox_before_startup(&facade)
            .await
            .unwrap();

        assert_eq!(first, LifecycleReconciliationReport::default());
        assert_eq!(second, LifecycleReconciliationReport::default());
    }

    #[test]
    fn test_http_egress_enabled_requires_non_empty_config() {
        let mut config = ferrum_gateway::ServerConfig::default();

        // Absent config -> disabled
        assert!(!http_egress_enabled(&config));

        // Present but empty -> disabled (not merely Some)
        config.http_egress = Some(HttpEgressConfig::default());
        assert!(!http_egress_enabled(&config));

        // Non-empty allowed_hosts -> enabled
        config.http_egress =
            Some(HttpEgressConfig::from_hosts(vec!["example.com".to_string()]).unwrap());
        assert!(http_egress_enabled(&config));
    }
}
