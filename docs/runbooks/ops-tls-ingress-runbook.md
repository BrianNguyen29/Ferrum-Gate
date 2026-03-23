# Runbook: TLS Termination / Ingress in Front of ferrumd

## Context

`ferrumd` has **no in-process TLS listener**. All transport-layer encryption must happen at an external ingress layer (reverse proxy, load balancer, API gateway). Application-layer bearer auth is handled inside `ferrumd`.

This runbook covers deploying a TLS-terminating nginx reverse proxy in front of `ferrumd` for production use.

## Runtime assumptions

| Item | Value |
|------|-------|
| `ferrumd` bind | `0.0.0.0:8080` (config-driven) |
| `ferrumd` auth | `bearer` mode — app-layer, no TLS requirement |
| Health endpoints | `/v1/healthz` (liveness), `/v1/readyz` (readiness) — unauthenticated |
| Control-plane routes | All other routes require `Authorization: Bearer <token>` |
| Startup guard | Rejects non-loopback bind with auth disabled unless `allow_insecure_nonlocal = true` |

These are documented in [15-deployment-and-operations.md](../15-deployment-and-operations.md).

## Prerequisites

- A TLS certificate and key (Let's Encrypt, Vault PKI, or enterprise CA)
- nginx with `ssl`, `proxy`, and `http` modules
- `ferrumd` deployed with `configs/ferrumgate.prod.toml` or equivalent bearer-auth config
- `ferrumd` started with `--bind 0.0.0.0:8080` (or `FERRUMD_BIND_ADDR=0.0.0.0:8080`)
- `FERRUMD_BEARER_TOKEN` set to a strong, unique value

## Nginx config

Save as `/etc/nginx/sites-available/ferrumgate`:

```nginx
server {
    listen 443 ssl;
    server_name ferrumgate.example.com;

    # TLS termination
    ssl_certificate     /etc/ssl/certs/ferrumgate.crt;
    ssl_certificate_key /etc/ssl/private/ferrumgate.key;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5:!RC4;
    ssl_prefer_server_ciphers on;

    # Security headers
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
    add_header X-Content-Type-Options    "nosniff" always;
    add_header X-Frame-Options          "DENY" always;
    add_header X-XSS-Protection          "1; mode=block" always;

    # Proxy to ferrumd (plaintext on loopback — trusted network)
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;

        # Pass real client IP for audit/logging
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Host $host;

        # timeouts — match or exceed ferrumd's keepalive expectations
        proxy_connect_timeout 5s;
        proxy_send_timeout    60s;
        proxy_read_timeout    60s;

        # Required for HTTP/1.1 proxying and keepalive
        proxy_buffering off;
    }

    # Health checks — nginx health_pass is fine here since
    # these endpoints are unauthenticated in ferrumd.
    # You can also use nginx's upstream health check instead.
    location /v1/healthz {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_connect_timeout 2s;
        proxy_read_timeout    2s;
        access_log off;
    }

    location /v1/readyz {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_connect_timeout 2s;
        proxy_read_timeout    2s;
        access_log off;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name ferrumgate.example.com;
    return 301 https://$host$request_uri;
}
```

Apply with:

```sh
sudo ln -s /etc/nginx/sites-available/ferrumgate /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

## Startup guard preflight

Before rolling out, verify the startup guard passes with the intended non-loopback + bearer-auth config:

```sh
# dry-run: print effective config and startup guard verdict without binding
cargo run -p ferrumd -- --config configs/ferrumgate.prod.toml --print-effective-config

# or, check startup guard only
cargo run -p ferrumd -- --config configs/ferrumgate.prod.toml --check-startup-guard
```

Expected: startup guard reports `pass` when `auth.mode = "bearer"` and bind is non-loopback.

## Readiness check after startup

After `ferrumd` and nginx are running:

```sh
# via ferrumctl
cargo run -p ferrumctl -- server ready

# or curl directly (bypasses nginx health location)
curl http://127.0.0.1:8080/v1/readyz

# or through nginx (uses the health location above)
curl https://ferrumgate.example.com/v1/readyz
```

Expected: `{"status":"ready"}` from `/v1/readyz` whether accessed directly or through nginx.

## Verifying TLS and bearer auth end-to-end

```sh
# TLS is terminated at nginx; ferrumd sees plaintext on loopback.
# Bearer auth is app-layer — test that unauthenticated requests are rejected:

# direct to ferrumd (unauthenticated — expect 401 if bearer mode)
curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:8080/v1/approvals

# through nginx without token (expect 401 via ferrumd app auth)
curl -s -o /dev/null -w "%{http_code}" https://ferrumgate.example.com/v1/approvals

# through nginx with bearer token (expect 200 with the normal approval-list response)
curl -s -o /dev/null -w "%{http_code}" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  https://ferrumgate.example.com/v1/approvals
```

## Certificate rollover

1. Reload nginx with new cert/key: `sudo systemctl reload nginx`
2. Verify: `openssl s_client -connect ferrumgate.example.com:443 -servername ferrumgate.example.com </dev/null 2>/dev/null | openssl x509 -noout -dates`
3. Check `/v1/readyz` still returns `ok` after reload.

## Debugging startup guard rejection

If `ferrumd` refuses to bind non-loopback:

```
Error: ferrumd rejected non-loopback bind with auth disabled ...
```

Fix: ensure `auth.mode = "bearer"` or set `allow_insecure_nonlocal = true` explicitly in the config. See [15-deployment-and-operations.md](../15-deployment-and-operations.md) for the fail-closed startup guard logic.

## Dependencies

- nginx ≥ 1.18 (or any version with `ssl`, `proxy`, `http` modules)
- A TLS certificate valid for the ingress DNS name
- `ferrumd` running with bearer auth enabled
- Network path: clients → nginx (443) → ferrumd (8080 loopback)
