use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::Request;
use axum::extract::State;
use axum::extract::connect_info::ConnectInfo;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_governor::GovernorError;
use tower_governor::key_extractor::KeyExtractor;

use crate::auth_actor::AuthActor;
use crate::state::AppState;

/// Typed request extension carrying the resolved client IP address.
///
/// Populated once by the client-IP resolver layer so that downstream rate-limit
/// key extractors and telemetry all agree on the same address without
/// re-parsing proxy headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ResolvedClientIp(IpAddr);

impl ResolvedClientIp {
    pub(crate) fn new(ip: IpAddr) -> Self {
        Self(ip)
    }

    pub(crate) fn ip(&self) -> IpAddr {
        self.0
    }
}

/// Rate-limiting key that buckets authenticated requests by a verified actor
/// identity combined with IP, and anonymous requests by IP alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RateLimitKey {
    PrincipalIp { principal: String, ip: IpAddr },
    Ip(IpAddr),
}

/// Resolve the client IP once and store it as the typed `ResolvedClientIp`
/// request extension.
///
/// Trust semantics:
/// * Direct/untrusted peers ignore `X-Real-IP` and `X-Forwarded-For` entirely;
///   the peer IP is used.
/// * A trusted immediate peer (one whose IP is in `trusted_proxy_cidrs`) may
///   supply a single valid `X-Real-IP` value. Malformed, multiple, or missing
///   values fall back to the peer IP.
/// * Missing `ConnectInfo` fails closed before any auth handler runs.
pub(crate) async fn resolve_client_ip(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = match request
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
    {
        Some(ConnectInfo(addr)) => *addr,
        None => {
            tracing::warn!("request missing ConnectInfo; closing before auth/handler");
            return (axum::http::StatusCode::BAD_REQUEST, "missing peer address").into_response();
        }
    };

    let peer_ip = peer.ip();
    let resolved_ip = if state
        .server_config
        .trusted_proxy_cidrs
        .iter()
        .any(|cidr| cidr.contains(&peer_ip))
    {
        resolve_trusted_proxy_ip(&request, peer_ip)
    } else {
        peer_ip
    };

    request
        .extensions_mut()
        .insert(ResolvedClientIp::new(resolved_ip));
    next.run(request).await
}

/// Extract a single valid `X-Real-IP` from a trusted immediate peer, falling
/// back to the peer IP when the header is absent, appears more than once, or
/// cannot be parsed as an IP address.
fn resolve_trusted_proxy_ip(request: &Request, peer_ip: IpAddr) -> IpAddr {
    let mut resolved: Option<IpAddr> = None;
    for value in request.headers().get_all("x-real-ip") {
        if resolved.is_some() {
            // More than one value: fall back to peer IP.
            return peer_ip;
        }
        let Some(s) = value.to_str().ok() else {
            return peer_ip;
        };
        let candidate = s.trim().parse::<IpAddr>();
        match candidate {
            Ok(ip) => resolved = Some(ip),
            Err(_) => return peer_ip,
        }
    }
    resolved.unwrap_or(peer_ip)
}

/// Key extractor for the outer (pre-auth) IP-only governor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedIpKeyExtractor;

impl KeyExtractor for ResolvedIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<ResolvedClientIp>()
            .map(ResolvedClientIp::ip)
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Key extractor for the inner (post-auth) governor. Uses the verified
/// `AuthActor` extension inserted by the auth middleware; no raw header or
/// token value is ever used as a principal component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthActorIpKeyExtractor;

impl KeyExtractor for AuthActorIpKeyExtractor {
    type Key = RateLimitKey;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        let ip = req
            .extensions()
            .get::<ResolvedClientIp>()
            .map(ResolvedClientIp::ip)
            .ok_or(GovernorError::UnableToExtractKey)?;

        if let Some(actor) = req.extensions().get::<AuthActor>() {
            Ok(RateLimitKey::PrincipalIp {
                principal: actor.actor_id.clone(),
                ip,
            })
        } else {
            Ok(RateLimitKey::Ip(ip))
        }
    }
}

/// Build a tower-governor configuration with the given key extractor and rate
/// limits. Used by both production and test-only router builders.
#[macro_export]
macro_rules! governor_config {
    ($key_extractor:expr, $per_second:expr, $burst_size:expr $(,)?) => {{
        tower_governor::governor::GovernorConfigBuilder::default()
            .key_extractor($key_extractor)
            .per_second($per_second)
            .burst_size($burst_size)
            .finish()
            .expect("validated governor configuration")
    }};
}
