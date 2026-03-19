use anyhow::Context;
use ferrum_adapter_fs::FsRollbackAdapter;
use ferrum_adapter_git::register_git_adapter;
use ferrum_adapter_maildraft::MaildraftAdapter;
use ferrum_adapter_sqlite::SqliteRollbackAdapter;
use ferrum_cap::InMemoryCapabilityService;
use ferrum_firewall::DefaultFirewall;
use ferrum_gateway::{GatewayConfig, GatewayRuntime, run_http_server};
use ferrum_pdp::StaticPdpEngine;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let pdp = Arc::new(StaticPdpEngine);
    let cap = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    registry.register(Arc::new(FsRollbackAdapter::new("fs")));
    register_git_adapter(&mut registry);
    registry.register(Arc::new(SqliteRollbackAdapter::new("sqlite")));
    registry.register(Arc::new(MaildraftAdapter::new("maildraft")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:?cache=shared").await?);
    store.apply_embedded_migrations().await?;

    let firewall = Arc::new(DefaultFirewall::new());

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store, firewall);
    let addr: SocketAddr = "127.0.0.1:8080"
        .parse()
        .context("failed to parse bind address")?;

    let config = GatewayConfig { bind_addr: addr };
    run_http_server(config, runtime).await
}
