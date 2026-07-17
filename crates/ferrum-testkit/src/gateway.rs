use std::sync::Arc;

use ferrum_cap::CapabilityService;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::StoreFacade;

/// In-memory SQLite runtime harness for gateway integration tests.
///
/// Provides a fully wired `GatewayRuntime` backed by a migrated in-memory
/// SQLite store, a static PDP, an in-memory capability service, and a noop
/// rollback adapter. No bridges are registered.
pub struct SqliteGateway {
    pub store: Arc<ferrum_store::SqliteStore>,
    pub runtime: ferrum_gateway::GatewayRuntime,
}

impl SqliteGateway {
    /// Creates a new in-memory SQLite gateway runtime.
    pub async fn new() -> anyhow::Result<Self> {
        let store = Arc::new(ferrum_store::SqliteStore::connect("sqlite::memory:").await?);
        store.apply_embedded_migrations().await?;

        let pdp = Arc::new(ferrum_pdp::StaticPdpEngine);
        let cap: Arc<dyn CapabilityService> =
            Arc::new(ferrum_cap::InMemoryCapabilityService::default());

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

        let runtime = ferrum_gateway::GatewayRuntime::new(
            pdp,
            cap,
            rollback,
            store.clone() as Arc<dyn StoreFacade>,
            Vec::new(),
        );

        Ok(Self { store, runtime })
    }
}
