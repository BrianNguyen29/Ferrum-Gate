# GCP Phase 3B Non-Prod Domain/TLS Rehearsal Artifact

Date: 2026-05-08

Status: **NON-PROD rehearsal evidence only**.

Non-claims:
- NOT production-ready.
- NOT G2 complete.
- NOT pilot authorized.
- NOT operator signoff.
- `nip.io` is temporary and must not be treated as a production domain.

## Target

- Project: `fairy-b13f4`
- Region: `asia-southeast1`
- Zone: `asia-southeast1-a`
- VM: `ferrumgate-nonprod`
- Static IP: `34.158.51.8`
- Temporary TLS domain: `34-158-51-8.nip.io`
- HTTPS URL: `https://34-158-51-8.nip.io`
- Internal app bind after Phase 3B: `127.0.0.1:19080`
- TLS terminator: Caddy `v2.11.2`

## Commands run

```bash
bash scripts/gcp/phase3b_configure_tls_caddy.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --tls-domain 34-158-51-8.nip.io \
  --confirm
```

The first execution exposed a Caddy file-log permission issue. The configure script was corrected to use Caddy's default journald logging instead of a custom `/var/log/caddy/caddy.log` writer.

The second execution configured Caddy but exposed weak script verification: token prefix read used non-sudo `grep`, and failed curl probes could print `000000` while the script still summarized success. The configure script was corrected to:
- read only an 8-character token prefix with `sudo grep ... | head -c 8`;
- wait for certificate provisioning;
- fail on non-200 endpoint statuses;
- remove the `|| echo "000"` status fallback.

Final strict execution passed:

```text
Firewall 'ferrumgate-nonprod-fw-http' already exists.
Firewall 'ferrumgate-nonprod-fw-https' already exists.
Caddy already installed: v2.11.2 h1:iOlpsSiSKqEW+SIXrcZsZ/NO74SzB/ycqqvAIEfIm64=
ferrumgate bind updated to 127.0.0.1:19080
ferrumgate.service restarted and active (bind: 127.0.0.1:19080)
Caddy configured and reloaded for 34-158-51-8.nip.io
Certificate provisioned (0s).
Token prefix: 57149b36...
/v1/healthz: HTTP 200
/v1/readyz: HTTP 200
/v1/metrics: HTTP 200
```

## Independent probes

Public HTTPS health endpoint:

```text
HTTP/2 200
via: 1.1 Caddy
content-type: application/json
```

Service status:

```text
sudo systemctl is-active caddy -> active
sudo systemctl is-active ferrumgate.service -> active
```

Firewall state:

```text
ferrumgate-nonprod-fw-app    tcp:19080  118.69.4.63/32
ferrumgate-nonprod-fw-http   tcp:80     0.0.0.0/0
ferrumgate-nonprod-fw-https  tcp:443    0.0.0.0/0
ferrumgate-nonprod-fw-ssh    tcp:22     118.69.4.63/32
```

Auth probes:

```text
GET /v1/approvals without bearer token -> 401
GET /v1/approvals with VM-local bearer token -> 200
```

Only an 8-character token prefix was printed in script output. The full bearer token was not printed or committed.

## Rollback

Rollback command:

```bash
bash scripts/gcp/phase3b_destroy_tls_caddy.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --confirm
```

Expected rollback behavior:
- stop/disable Caddy;
- remove public 80/443 firewall rules;
- restore FerrumGate bind to `0.0.0.0:19080` for Phase 3A fallback;
- verify Phase 3A fallback health endpoint.

## Result

Phase 3B domain/TLS rehearsal is validated for the GCP non-prod VM with temporary `nip.io` domain and Caddy TLS termination.

This artifact does **not** authorize production pilot, complete G2, or replace operator signoff.
