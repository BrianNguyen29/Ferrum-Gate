# 95 — GCP Compute Phase 3B Domain/TLS Plan (nip.io + Caddy)

## Overview

Phase 3B adds TLS termination to the existing Phase 3A non-prod VM using [Caddy](https://caddyserver.com/docs/) as a reverse proxy with automatically managed Let's Encrypt certificates. A temporary [nip.io](https://nip.io/) DNS domain is used to avoid needing a real registered domain for this rehearsal.

This document is **NOT production-ready**, **NOT G2 complete**, **NOT pilot authorized**, and **NOT operator signoff**.

---

## Non-Claims (Phase 3B)

> **IMPORTANT**: Phase 3B carries the following explicit non-claims:
> - NOT production-ready
> - NOT G2 complete (operator signoff pending)
> - NOT pilot authorized
> - NOT operator signoff
> - NOT suitable for production workloads
> - NOT a permanent domain solution (nip.io is a free dynamic DNS resolver for IP-based routing; it is not appropriate for production)
>
> Phase 3B is operator-owned GCP non-prod TLS rehearsal/evidence support only.

---

## Architecture

### Before Phase 3B (Phase 3A)

```
Internet
   |
   |-- TCP 19080 (allowlist only) --> [ ferrumgate: 0.0.0.0:19080 ]
   |   (no TLS)
```

### After Phase 3B

```
Internet
   |
   |-- TCP 80 (HTTP) -------------> [ Caddy ] --> 127.0.0.1:19080
   |-- TCP 443 (HTTPS/TLS) -------> [ Caddy ] --> 127.0.0.1:19080
   |                                   |
   |                              Let's Encrypt
   |                              (automatic cert)
   |
   |-- TCP 19080 (from allowlist) ---> [ ferrumgate: 127.0.0.1:19080 ]
                                        (localhost-only, internal)
```

### Key Changes

| Component | Phase 3A | Phase 3B |
|---|---|---|
| ferrumgate bind address | `0.0.0.0:19080` | `127.0.0.1:19080` (localhost only) |
| External access | Direct to ferrumgate port 19080 | Via Caddy reverse proxy on 443 |
| TLS | None | Let's Encrypt (automatic via Caddy) |
| HTTP port 80 | Not used | Caddy ACME HTTP-01 challenge |
| Firewall | SSH 22 + app 19080 from allowlist | Added 80 + 443 from 0.0.0.0/0 to VM tag |

### nip.io Caveats

[nip.io](https://nip.io/) is a free wildcard DNS service that maps `<IPv4-dashes>.nip.io` to the corresponding IPv4 address. It is:
- **Convenient for testing/rehearsal** — no DNS registrar or domain purchase required
- **NOT for production** — rely on it only for temporary non-prod rehearsal
- Subject to availability of the nip.io service (no SLA)
- For production: use a real registered domain with Cloud DNS or another DNS provider

---

## Prerequisites

1. **Phase 3A VM must be running** with ferrumgate service on port 19080
2. **Static external IP**: `34.158.51.8`
3. **TLS_DOMAIN** must resolve: `34-158-51-8.nip.io` → `34.158.51.8` (nip.io provides this automatically)
4. **Ports 80 and 443** must be reachable from the internet (GCP firewall will be configured by the script)
5. **gcloud CLI** authenticated and configured for project `fairy-b13f4`

### Verify Prerequisites

```bash
# Check VM is running
gcloud compute instances describe ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4

# Check external IP
gcloud compute instances describe ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 \
    --format='value(networkInterfaces[0].accessConfigs[0].natIP)'
# Expected: 34.158.51.8

# Verify nip.io resolves
nslookup 34-158-51-8.nip.io
# Expected: 34.158.51.8

# Check ferrumgate is running on Phase 3A (direct access)
curl -s -o /dev/null -w "%{http_code}" \
    http://34.158.51.8:19080/v1/healthz
# Expected: 200 (once token provided) or 401 (auth required)
```

---

## Scripts

| Script | Purpose | Runs On |
|---|---|---|
| `phase3b_configure_tls_caddy.sh` | Install Caddy, switch ferrumgate to localhost bind, configure TLS, test endpoints | **Local** |
| `phase3b_destroy_tls_caddy.sh` | Stop Caddy, remove 80/443 firewall rules, restore ferrumgate to 0.0.0.0 bind | **Local** |

---

## Usage

### 1. Configure TLS

```bash
cd /path/to/Ferrum-Gate-verify

# Verify prerequisites first (no VM changes, no --confirm needed)
nslookup 34-158-51-8.nip.io

# Configure TLS (requires --confirm)
bash scripts/gcp/phase3b_configure_tls_caddy.sh \
    --project-id fairy-b13f4 \
    --region asia-southeast1 \
    --zone asia-southeast1-a \
    --vm-name ferrumgate-nonprod \
    --tls-domain 34-158-51-8.nip.io \
    --confirm
```

Expected outputs:
- Caddy version and installation confirmation
- Firewall rules created for ports 80 and 443
- ferrumgate bind changed to `127.0.0.1:19080`
- Caddy certificate provisioned via Let's Encrypt
- HTTPS endpoint tests for `/v1/healthz`, `/v1/readyz`, `/v1/metrics`

### 2. Test HTTPS Endpoints

```bash
# Retrieve full bearer token from VM (requires sudo)
TOKEN=$(gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'sudo cat /etc/ferrumgate/ferrumgate_initial_token')

# Test HTTPS health
curl -H "Authorization: Bearer ${TOKEN}" \
    https://34-158-51-8.nip.io/v1/healthz

# Test readyz
curl -H "Authorization: Bearer ${TOKEN}" \
    https://34-158-51-8.nip.io/v1/readyz

# Test metrics (no auth required)
curl https://34-158-51-8.nip.io/v1/metrics

# Check certificate info
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'caddy list-certificates'
```

### 3. Rollback to Phase 3A (Remove TLS)

```bash
bash scripts/gcp/phase3b_destroy_tls_caddy.sh \
    --project-id fairy-b13f4 \
    --region asia-southeast1 \
    --zone asia-southeast1-a \
    --vm-name ferrumgate-nonprod \
    --confirm
```

After rollback:
- Caddy is stopped and disabled
- Firewall rules for 80 and 443 are deleted
- ferrumgate bind is restored to `0.0.0.0:19080`
- Direct access to `http://34.158.51.8:19080` resumes

---

## Security Notes

### Phase 3B Firewall Changes

Phase 3B opens ports **80** and **443 to `0.0.0.0/0`** (the internet) via the custom VPC's firewall. This is necessary for Caddy's Let's Encrypt ACME HTTP-01 challenge to work. The rules are:
- `ferrumgate-nonprod-fw-http`: TCP 80 from 0.0.0.0/0 to VM network tag
- `ferrumgate-nonprod-fw-https`: TCP 443 from 0.0.0.0/0 to VM network tag

These are **targeted to the VM's network tag** (`ferrumgate-nonprod-app`), so they only affect the FerrumGate VM, not other resources in the VPC.

### ferrumgate localhost-only Bind

With Caddy as the reverse proxy, ferrumgate binds to `127.0.0.1:19080` (localhost only). This means:
- ferrumgate is **not directly accessible from the internet** on port 19080
- All external HTTPS traffic goes through Caddy which terminates TLS
- The allowlist restriction on port 19080 still applies but is now a second layer (Caddy is the internet-facing endpoint)

### Token Security

The bearer token is unchanged from Phase 3A. It is still stored in `/etc/ferrumgate/ferrumgate_initial_token` (root-only on VM). The token prefix is still printed by scripts, never the full token.

---

## Phase 3A Fallback / Rollback

To roll back Phase 3B at any time and restore Phase 3A:

```bash
bash scripts/gcp/phase3b_destroy_tls_caddy.sh --confirm
```

This:
1. Stops and disables the Caddy service
2. Deletes the 80/443 firewall rules (returning to Phase 3A state)
3. Restores `FERRUMD_BIND_ADDR=0.0.0.0:19080` in `/etc/ferrumgate/env`
4. Restarts the ferrumgate service
5. Leaves the VM, static IP, VPC, subnet, SSH/app firewall rules intact

To completely remove all Phase 3A + 3B resources:

```bash
bash scripts/gcp/phase3a_destroy_nonprod_vm.sh --confirm
```

---

## Custom Domain Path (Post-Phase 3B)

When ready for a real domain (out of scope for Phase 3B):

1. Register a domain (e.g., via Cloud Domains in GCP)
2. Create a Cloud DNS A record pointing the domain to `34.158.51.8`
3. Update `TLS_DOMAIN` in `phase3b_configure_tls_caddy.sh` to the real domain
4. Re-run the configure script (Caddy will re-issue certificates for the new domain)
5. The Caddyfile supports any domain with a valid DNS record pointing to this IP

**Phase 3B is NOT authorized for production use. Custom domain path requires additional review.**

---

## Troubleshooting

### Let's Encrypt certificate fails to provision

```bash
# On VM: check Caddy logs
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'sudo journalctl -u caddy -n 50 --no-pager'

# Check port 80 is reachable from internet
curl -s -o /dev/null -w "%{http_code}" http://34.158.51.8/

# Check DNS resolves
nslookup 34-158-51-8.nip.io
```

### Caddy reload fails

```bash
# On VM: validate Caddyfile
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'sudo caddy validate --config /etc/caddy/Caddyfile'

# Check Caddy version compatibility
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'caddy version'
```

### ferrumgate unreachable after bind change

```bash
# On VM: check ferrumgate service
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'sudo systemctl status ferrumgate.service --no-pager'

# Check bind address is correct
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'grep FERRUMD_BIND_ADDR /etc/ferrumgate/env'

# Check ferrumgate is listening
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    'sudo ss -tlnp | grep 19080'
```

---

## References

- Caddy documentation: <https://caddyserver.com/docs/>
- Caddy Cloudsmith apt repository: <https://caddyserver.com/docs/install#debian-ubuntu-raspbian>
- Let's Encrypt ACME HTTP-01 challenge: <https://letsencrypt.org/docs/challenge-types/#http-01-challenge>
- nip.io: <https://nip.io/>
- GCP Firewall rules: <https://cloud.google.com/vpc/docs/firewalls>
- Phase 3A plan: [94-gcp-compute-phase3a-nonprod-target-plan.md](./94-gcp-compute-phase3a-nonprod-target-plan.md)

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial Phase 3B TLS/nip.io/Caddy plan. Operator-owned rehearsal only. |
