use super::*;
use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower::ServiceExt;

fn peer_addr(ip: [u8; 4], port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])), port)
}

async fn insert_scoped_token(
    runtime: &GatewayRuntime,
    token_value: &str,
    actor_id: &str,
) -> ferrum_proto::ScopedToken {
    let salt = generate_token_salt();
    let token = ferrum_proto::ScopedToken {
        token_id: uuid::Uuid::new_v4().to_string(),
        actor_id: actor_id.to_string(),
        role: ferrum_proto::TokenRole::Admin,
        scopes: vec!["*".to_string()],
        description: None,
        expires_at: chrono::Utc::now() + chrono::Duration::days(1),
        created_at: chrono::Utc::now(),
        last_used_at: None,
        revoked_at: None,
        revoked_reason: None,
        rotated_from: None,
        token_lookup_hash: hash_token_value(token_value),
        token_hash: hash_token_with_salt(token_value, &salt),
        token_salt: salt,
    };
    runtime.store.tokens().insert(&token).await.unwrap();
    token
}

// ---------------------------------------------------------------------------
// P0: Monitoring endpoints bypass both governors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_monitoring_endpoints_bypass_rate_limiter() {
    let runtime = test_runtime().await;
    // Very restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    for endpoint in ["/v1/metrics", "/v1/readyz", "/v1/healthz"] {
        for i in 0..5 {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(endpoint)
                        .header("x-real-ip", "192.168.1.1")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "monitoring endpoint {} request {} should bypass rate limiter",
                endpoint,
                i
            );
        }
    }
}

// ---------------------------------------------------------------------------
// P1: Outer (pre-auth) IP-only governor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_workload_endpoint_is_rate_limited_by_outer_governor() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor(runtime, 1, 1);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "192.168.1.1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut got_429 = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "192.168.1.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            got_429 = true;
            break;
        }
    }
    assert!(
        got_429,
        "workload endpoint should be rate limited by outer governor"
    );
}

#[tokio::test]
async fn test_outer_governor_precedes_auth() {
    let runtime = test_runtime().await;
    let config = ServerConfig {
        auth_mode: AuthMode::Bearer,
        bearer_token: Some("secret-token".to_string()),
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        ..ServerConfig::default()
    };
    let router = build_router_with_governor_and_config(runtime, config);
    let peer = peer_addr([192, 168, 1, 1], 12345);

    // First request: outer governor allows, then auth rejects (no token).
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Second request: outer governor rejects before auth is reached.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------------------
// P2: AuthMode::Disabled uses only the outer governor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_disabled_mode_uses_only_outer_governor() {
    let runtime = test_runtime().await;
    let config = ServerConfig {
        auth_mode: AuthMode::Disabled,
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        ..ServerConfig::default()
    };
    let router = build_router_with_governor_and_config(runtime, config);
    let peer = peer_addr([10, 0, 0, 1], 0);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("Authorization", "Bearer ignored-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same token, same IP: outer bucket exhausted -> 429.
    let mut got_429 = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .extension(ConnectInfo(peer))
                    .header("Authorization", "Bearer ignored-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            got_429 = true;
            break;
        }
    }
    assert!(
        got_429,
        "authenticated request should share outer IP bucket in disabled mode"
    );

    // A different token on the same IP is also limited because there is no inner governor.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("Authorization", "Bearer other-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------------------
// P3: Inner (post-auth) governor uses verified AuthActor identity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inner_governor_buckets_by_auth_actor_identity() {
    let runtime = test_runtime().await;
    let token_a = "scoped-token-a";
    let token_b = "scoped-token-b";
    insert_scoped_token(&runtime, token_a, "alice").await;
    insert_scoped_token(&runtime, token_b, "bob").await;

    let config = ServerConfig {
        auth_mode: AuthMode::Scoped,
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        pre_auth_rate_limit_per_second: Some(100),
        pre_auth_rate_limit_burst: Some(100),
        ..ServerConfig::default()
    };
    let router = build_router_with_governor_and_config(runtime, config);
    let peer = peer_addr([10, 0, 0, 1], 0);

    // Alice exhausts her own inner bucket.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("Authorization", format!("Bearer {}", token_a))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut alice_limited = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .extension(ConnectInfo(peer))
                    .header("Authorization", format!("Bearer {}", token_a))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            alice_limited = true;
            break;
        }
    }
    assert!(alice_limited, "alice should be limited by inner governor");

    // Bob from the same IP still succeeds because the inner key is actor+IP.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("Authorization", format!("Bearer {}", token_b))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "bob should have a separate inner bucket from alice"
    );

    // Unauthenticated request is rejected by auth before the inner governor runs.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// P4: Trusted proxy client-IP resolution
// ---------------------------------------------------------------------------

fn trust_none_config() -> ServerConfig {
    ServerConfig {
        auth_mode: AuthMode::Disabled,
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        trusted_proxy_cidrs: vec![],
        ..ServerConfig::default()
    }
}

fn trust_10_config() -> ServerConfig {
    ServerConfig {
        auth_mode: AuthMode::Disabled,
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        trusted_proxy_cidrs: vec!["10.0.0.0/8".parse().unwrap()],
        ..ServerConfig::default()
    }
}

#[tokio::test]
async fn test_direct_peer_ignores_x_real_ip() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_none_config());
    let peer = peer_addr([10, 0, 0, 1], 0);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-real-ip", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same peer without x-real-ip shares the same bucket (peer IP), so it is limited.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_direct_peer_ignores_x_forwarded_for() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_none_config());
    let peer = peer_addr([10, 0, 0, 1], 0);

    // Direct/untrusted peers ignore X-Forwarded-For; the peer IP is used.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-forwarded-for", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The same peer without X-Forwarded-For still shares the same bucket.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "X-Forwarded-For must not change the resolved bucket for an untrusted peer"
    );
}

#[tokio::test]
async fn test_trusted_peer_accepts_single_x_real_ip() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_10_config());

    // Two different trusted peers reporting the same x-real-ip share a bucket.
    let peer_a = peer_addr([10, 0, 0, 1], 0);
    let peer_b = peer_addr([10, 0, 0, 2], 0);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer_a))
                .header("x-real-ip", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer_b))
                .header("x-real-ip", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "same x-real-ip from different trusted peers must share a bucket"
    );
}

#[tokio::test]
async fn test_trusted_peer_fallback_on_missing_x_real_ip() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_10_config());
    let peer = peer_addr([10, 0, 0, 1], 0);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-real-ip", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Missing x-real-ip falls back to the peer IP, which is a different bucket.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_trusted_peer_fallback_on_multiple_x_real_ip() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_10_config());
    let peer = peer_addr([10, 0, 0, 1], 0);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-real-ip", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Multiple x-real-ip values fall back to the peer IP, a different bucket.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-real-ip", "1.2.3.4")
                .header("x-real-ip", "5.6.7.8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_trusted_peer_fallback_on_malformed_x_real_ip() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_10_config());
    let peer = peer_addr([10, 0, 0, 1], 0);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-real-ip", "1.2.3.4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Malformed x-real-ip falls back to the peer IP, a different bucket.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .extension(ConnectInfo(peer))
                .header("x-real-ip", "not-an-ip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_missing_connect_info_fails_closed_before_handler() {
    let runtime = test_runtime().await;
    let router = build_router_with_governor_and_config(runtime, trust_none_config());

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "missing ConnectInfo must fail closed before auth/handler"
    );
}
