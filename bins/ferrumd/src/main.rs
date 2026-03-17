use anyhow::Context;
use ferrum_cap::InMemoryCapabilityService;
use ferrum_gateway::{run_http_server, GatewayConfig, GatewayRuntime};
use ferrum_pdp::StaticPdpEngine;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let pdp = Arc::new(StaticPdpEngine::default());
    let cap = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:?cache=shared").await?);
    store.apply_embedded_migrations().await?;

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store);
    let addr: SocketAddr = "127.0.0.1:8080"
        .parse()
        .context("failed to parse bind address")?;

    let config = GatewayConfig { bind_addr: addr };
    run_http_server(config, runtime).await
}
