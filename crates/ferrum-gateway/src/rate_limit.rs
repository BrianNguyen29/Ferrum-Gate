use tower_governor::{GovernorError, key_extractor::KeyExtractor};

/// Rate-limiting key that buckets authenticated requests by a principal
/// identifier combined with IP, and anonymous requests by IP alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RateLimitKey {
    PrincipalIp {
        principal: String,
        ip: std::net::IpAddr,
    },
    Ip(std::net::IpAddr),
}

/// Key extractor that uses principal identity when available, falling back to
/// the client IP address.  This isolates authenticated traffic from anonymous
/// traffic on the same IP (noisy-neighbor mitigation) while preserving the
/// existing IP-based behavior for unauthenticated requests.
///
/// # Trust model
///
/// - The principal component depends on the auth middleware running *before*
///   the rate-limit layer so that `Authorization` / `X-Ferrum-Agent-Id` headers
///   have already been validated.  In `run_http_server` the `GovernorLayer` is
///   applied to the workload router and the auth layer is wrapped around the
///   merged app afterwards, so auth runs first.
/// - The IP component is delegated to `SmartIpKeyExtractor`, which trusts
///   `X-Real-IP` and `X-Forwarded-For` headers when they are present.  This
///   makes the rate limiter only as trustworthy as the proxy/LB in front of
///   `ferrumd`.
/// - Production deployments must place `ferrumd` behind a reverse proxy or
///   load balancer that sets/overwrites `X-Real-IP` (and optionally
///   `X-Forwarded-For`) to the real client address.  See
///   `configs/examples/nginx-ferrumgate.conf` and the rate-limiting section of
///   `docs/PRODUCTION_NOTES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrincipalOrIpKeyExtractor;

impl KeyExtractor for PrincipalOrIpKeyExtractor {
    type Key = RateLimitKey;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        let ip = tower_governor::key_extractor::SmartIpKeyExtractor.extract(req)?;

        if let Some(agent_id) = req
            .headers()
            .get("X-Ferrum-Agent-Id")
            .and_then(|v| v.to_str().ok())
        {
            if !agent_id.is_empty() {
                return Ok(RateLimitKey::PrincipalIp {
                    principal: format!("agent:{}", agent_id),
                    ip,
                });
            }
        }

        if let Some(auth) = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
        {
            if !auth.is_empty() {
                let hash = blake3::hash(auth.as_bytes()).to_hex().to_string();
                return Ok(RateLimitKey::PrincipalIp {
                    principal: format!("auth:{}", hash),
                    ip,
                });
            }
        }

        Ok(RateLimitKey::Ip(ip))
    }
}
