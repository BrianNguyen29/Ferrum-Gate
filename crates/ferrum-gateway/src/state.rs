use ferrum_cap::CapabilityService;
use ferrum_firewall::SemanticFirewall;
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
    pub firewall: Arc<dyn SemanticFirewall>,
}

impl GatewayRuntime {
    pub fn new(
        pdp: Arc<dyn PdpEngine>,
        cap: Arc<dyn CapabilityService>,
        rollback: Arc<RollbackService>,
        store: Arc<SqliteStore>,
        firewall: Arc<dyn SemanticFirewall>,
    ) -> Self {
        Self {
            pdp,
            cap,
            rollback,
            store,
            firewall,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AuthMode {
    Disabled,
    Bearer,
}

impl std::str::FromStr for AuthMode {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "disabled" => Ok(AuthMode::Disabled),
            "bearer" => Ok(AuthMode::Bearer),
            _ => Err("invalid auth_mode, must be 'disabled' or 'bearer'"),
        }
    }
}

#[derive(Clone)]
pub struct ServerConfig {
    pub auth_mode: AuthMode,
    pub bearer_token: Option<String>,
}

#[derive(Clone)]
pub struct GatewayConfig {
    pub bind_addr: std::net::SocketAddr,
}
