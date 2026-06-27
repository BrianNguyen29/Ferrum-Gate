use super::*;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// P0: Monitoring endpoints bypass workload rate limiter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_monitoring_endpoints_bypass_rate_limiter() {
    let runtime = test_runtime().await;
    // Very restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // Monitoring endpoints should NOT be rate limited
    for endpoint in ["/v1/metrics", "/v1/readyz", "/v1/readyz/deep"] {
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

#[tokio::test]
async fn test_workload_endpoint_is_rate_limited() {
    let runtime = test_runtime().await;
    // Very restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // First request to a workload endpoint should succeed
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

    // Subsequent requests should eventually be rate limited
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

    assert!(got_429, "workload endpoint should be rate limited");
}

// ---------------------------------------------------------------------------
// P1: SmartIpKeyExtractor separate-bucket behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_distinct_x_real_ip_get_separate_buckets() {
    let runtime = test_runtime().await;
    // Restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // Exhaust the burst for IP A
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.36.0.1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // IP A should now be rate limited
    let mut ip_a_limited = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.36.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            ip_a_limited = true;
            break;
        }
    }
    assert!(ip_a_limited, "IP A should be rate limited after burst");

    // IP B should still succeed because it has its own bucket
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.36.0.2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "IP B should have a separate bucket and succeed"
    );
}

#[tokio::test]
async fn test_same_x_real_ip_is_limited_across_adapters() {
    let runtime = test_runtime().await;
    // Restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // First request from IP X to /v1/approvals succeeds
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.36.0.5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second request from same IP X to /v1/intents should be rate limited
    // because the bucket is keyed by IP, not by route.
    let mut got_429 = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/intents")
                    .header("x-real-ip", "10.36.0.5")
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
        "same x-real-ip should be limited across different workload routes"
    );
}

// ---------------------------------------------------------------------------
// P3: PrincipalOrIpKeyExtractor principal-aware rate limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_authenticated_and_anonymous_share_ip_but_separate_buckets() {
    let runtime = test_runtime().await;
    // Restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // Exhaust burst for anonymous traffic from IP 10.0.0.1
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.0.0.1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Anonymous should be rate limited
    let mut anon_limited = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.0.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            anon_limited = true;
            break;
        }
    }
    assert!(anon_limited, "anonymous should be rate limited");

    // Authenticated request from same IP should still succeed (separate bucket)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.0.0.1")
                .header("Authorization", "Bearer some-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "authenticated should have separate bucket from anonymous on same IP"
    );
}

#[tokio::test]
async fn test_distinct_auth_tokens_get_separate_buckets_same_ip() {
    let runtime = test_runtime().await;
    // Restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // Exhaust burst for principal A from IP 10.0.0.1
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.0.0.1")
                .header("Authorization", "Bearer token-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Principal A should be rate limited
    let mut principal_a_limited = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.0.0.1")
                    .header("Authorization", "Bearer token-a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            principal_a_limited = true;
            break;
        }
    }
    assert!(principal_a_limited, "principal A should be rate limited");

    // Principal B from same IP should still succeed (separate bucket)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.0.0.1")
                .header("Authorization", "Bearer token-b")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "principal B should have separate bucket from principal A"
    );
}

#[tokio::test]
async fn test_distinct_agent_ids_get_separate_buckets_same_ip() {
    let runtime = test_runtime().await;
    // Restrictive rate limit: 1 req/sec, burst 1
    let router = build_router_with_governor(runtime, 1, 1);

    // Exhaust burst for agent A from IP 10.0.0.1
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.0.0.1")
                .header("X-Ferrum-Agent-Id", "agent-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Agent A should be rate limited
    let mut agent_a_limited = false;
    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.0.0.1")
                    .header("X-Ferrum-Agent-Id", "agent-a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            agent_a_limited = true;
            break;
        }
    }
    assert!(agent_a_limited, "agent A should be rate limited");

    // Agent B from same IP should still succeed (separate bucket)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .header("x-real-ip", "10.0.0.1")
                .header("X-Ferrum-Agent-Id", "agent-b")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "agent B should have separate bucket from agent A"
    );
}
