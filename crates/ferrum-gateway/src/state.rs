use ferrum_cap::CapabilityService;
use ferrum_pdp::PdpEngine;
use ferrum_rollback::RollbackService;
use ferrum_store::SqliteStore;
use std::sync::Arc;

#[derive(Clone)]
pub struct GatewayRuntime {
    pub pdp: Arc<dyn PdpEngine>,
    pub cap: Arc<dyn CapabilityService>,
    pub rollback: Arc<RollbackService>,
    pub store: Arc<SqliteStore>,
}

impl GatewayRuntime {
    pub fn new(
        pdp: Arc<dyn PdpEngine>,
        cap: Arc<dyn CapabilityService>,
        rollback: Arc<RollbackService>,
        store: Arc<SqliteStore>,
    ) -> Self {
        Self { pdp, cap, rollback, store }
    }
}

#[derive(Clone)]
pub struct GatewayConfig {
    pub bind_addr: std::net::SocketAddr,
}
