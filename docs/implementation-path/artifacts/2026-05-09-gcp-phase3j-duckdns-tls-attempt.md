# GCP Phase 3J DuckDNS TLS Attempt Artifact

**Date**: 2026-05-09

**Scope**: Phase 3J DuckDNS free-domain TLS configuration — initial attempt blocked by ACME DNS/CAA instability; **retry succeeded**; DuckDNS HTTPS/TLS now working; Prometheus retargeted; nip.io historical (rollback no longer needed)

**Status**: **NON-PROD evidence only**. DuckDNS DNS A record working. DuckDNS HTTPS/TLS **SUCCESS** (after retry). nip.io superseded as primary (retained as fallback option).

---

## Non-Claims

This Phase 3J follow-up does **not** claim:

- production-ready status
- full production posture
- production alerting capability
- real owned domain (DuckDNS is free, no-ownership-verification DNS)
- full G2 completion beyond Phase 3F conditional single-node SQLite pilot scope
- full production pilot authorization
- Phase 3J operator signoff
- PostgreSQL runtime, HA, or multi-node deployment

Phase 3J documents a **successful DuckDNS TLS deployment** via free DuckDNS domain after initial ACME DNS/CAA failure. nip.io is retained as fallback but no longer the active primary endpoint.

---

## Overview

Phase 3J captures:

1. **DuckDNS DNS Resolution**: DNS A record correctly resolves to VM static IP (34.158.51.8)
2. **DuckDNS TLS Initial Failure**: First attempt ran `phase3g_configure_real_domain.sh` with `--real-domain ferrumgate.duckdns.org`; Caddy served challenges but ACME certificate issuance blocked by DuckDNS/CAA DNS instability (CAA lookup timeout, SERVFAIL); rolled back to nip.io
3. **DuckDNS TLS Retry Success**: Re-ran the script; ACME succeeded; certificate obtained; DuckDNS HTTPS now working
4. **Prometheus Retargeting**: Prometheus updated from `https://34-158-51-8.nip.io:443/v1/metrics` to `https://ferrumgate.duckdns.org:443/v1/metrics`; target health=up

---

## DuckDNS DNS Resolution

### Before Fix

```
ferrumgate.duckdns.org -> 118.69.4.63 (WRONG — did not match VM static IP)
HTTPS to ferrumgate.duckdns.org: TIMEOUT
```

### After User DNS Change

```
getent ahostsv4 ferrumgate.duckdns.org
-> 34.158.51.8 (CORRECT — matches VM static IP 34.158.51.8)
```

### HTTP Challenge Check (Before TLS Script)

```
HTTP/1.1 308 Permanent Redirect
Server: Caddy
-> DuckDNS DNS A record: WORKING
```

---

## DuckDNS TLS Attempt

### Script Execution

```bash
bash scripts/gcp/phase3g_configure_real_domain.sh --confirm --real-domain ferrumgate.duckdns.org
```

### Initial Attempt: TLS FAILED

First run of the script served ACME challenges but certificate issuance was blocked by DuckDNS/CAA DNS instability:

**Caddy Logs — ACME Failure Evidence (First Attempt)**

```
# Primary attempt — tls-alpn-01 and http-01 challenges served, secondary validation failed:
"query timed out looking up CAA for ferrumgate.duckdns.org"
"SERVFAIL looking up CAA for ferrumgate.duckdns.org"

# Staging retry — also failed:
"query timed out looking up CAA for duckdns.org"
"query timed out looking up A for ferrumgate.duckdns.org"
"no valid AAAA records"
```

**Root Cause (Initial Attempt)**: DuckDNS free DNS service had unstable DNS resolution for CAA lookups and A record propagation at time of first attempt. Let's Encrypt secondary DNS validation (CAA check) timed out, blocking certificate issuance.

**Rollback**: Caddy reverted to `34-158-51-8.nip.io`; all health checks passed.

---

### Retry: TLS SUCCESS

Re-ran `phase3g_configure_real_domain.sh --confirm --real-domain ferrumgate.duckdns.org`.

**Caddy Logs — ACME Success Evidence (Retry)**

```
# ACME success on retry:
"authorization finalized for ferrumgate.duckdns.org"
"validations succeeded finalizing order"
"successfully downloaded certificate chains"
"certificate obtained successfully"
"releasing lock"
```

**Caddy Live Config**

```
ferrumgate.duckdns.org {
  # Caddyfile now active with DuckDNS domain
  caddy.service active
  ferrumgate.service active
}
```

---

## DuckDNS HTTPS Health Checks

| Endpoint | Status | Response |
|----------|--------|----------|
| `https://ferrumgate.duckdns.org/v1/healthz` | HTTP/2 200 | `{"status":"ok"}` |
| `https://ferrumgate.duckdns.org/v1/readyz` | 200 | `{"status":"ready"}` |
| `https://ferrumgate.duckdns.org/v1/readyz/deep` | 200 | `{"status":"ok","healthy":true,... store ok, write_queue depth=0}` |
| `https://ferrumgate.duckdns.org/v1/metrics` | 200 | `ferrumgate_store_health_up 1`<br>`ferrumgate_write_queue_depth 0` |

**Result**: All DuckDNS HTTPS checks PASSED. DuckDNS TLS working.

---

## Prometheus Retargeting

### Before

```
scrapeUrl=https://34-158-51-8.nip.io:443/v1/metrics
```

### After

```
scrapeUrl=https://ferrumgate.duckdns.org:443/v1/metrics
```

### Validation

- `promtool check config`: PASSED
- Prometheus reload: done
- Prometheus target: `job=ferrumgate` `scrapeUrl=https://ferrumgate.duckdns.org:443/v1/metrics` `health=up` `lastError=` (empty)
- Prometheus query `up{job="ferrumgate"}`: returns `1` for `instance=ferrumgate.duckdns.org:443`

**Result**: Prometheus DuckDNS scrape target UP.

---

## Target Environment (Post-Retry)

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `ferrumgate.duckdns.org` (DuckDNS free DNS — **TLS SUCCESS**) |
| HTTPS URL | `https://ferrumgate.duckdns.org` |
| Database | SQLite single-node |
| Auth Mode | Bearer token |
| Monitoring | Local-only (Prometheus + AlertManager on-VM) |
| Alert Contact | **None** (local-only mode) |
| nip.io | Superseded as primary (retained as fallback option) |

---

## Claim Status Summary

| Claim | Status | Evidence |
|-------|--------|----------|
| DuckDNS DNS A record resolves to VM | **YES** | `getent ahostsv4 ferrumgate.duckdns.org` -> `34.158.51.8` |
| DuckDNS HTTPS/TLS working | **YES** (after retry) | All `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics` returned 200 |
| Prometheus DuckDNS scrape target UP | **YES** | `up{job="ferrumgate"}` returns `1` for `instance=ferrumgate.duckdns.org:443` |
| Initial ACME failure (historical) | **YES** | CAA lookup timeout, SERVFAIL on first attempt |
| nip.io rollback (historical) | **YES** | First attempt rolled back; now superseded by DuckDNS success |
| Real owned domain | **NO** | DuckDNS is free DNS; no ownership verification |
| Production-ready | **NO** | Nonprod only; single-node SQLite |
| Production alerting | **NO** | Local-only mode; no alert contact |
| Full production posture | **NO** | Nonprod evidence only |

---

## What Phase 3J is NOT

Phase 3J does NOT:

- Claim real owned domain (DuckDNS is free, no-ownership-verification DNS)
- Claim production-ready or full production posture
- Include production alerting capability (local-only mode)
- Include service account keys, tokens, or email literals
- Modify Phase 3F conditional pilot scope
- Replace Phase 3E operator signoff

---

## Remaining Blockers

| Item | Status | Blocker |
|------|--------|---------|
| Real owned domain TLS | **BLOCKED** | DuckDNS is free DNS with no ownership verification; real owned domain still required for production |
| Production alerting | **BLOCKED** | No alert contact; local-only mode |
| Production-ready claim | **BLOCKED** | Nonprod only; single-node SQLite |

---

## References

- Phase 3G plan: [101-phase3g-ops-hardening-plan.md](../101-phase3g-ops-hardening-plan.md)
- Phase 3G scaffold review: [2026-05-09-gcp-phase3g-scaffold-review.md](./2026-05-09-gcp-phase3g-scaffold-review.md)
- Phase 3H offsite monitoring: [2026-05-09-gcp-phase3h-offsite-monitoring.md](./2026-05-09-gcp-phase3h-offsite-monitoring.md)
- Phase 3I no-domain follow-up: [2026-05-09-gcp-phase3i-no-domain-followup.md](./2026-05-09-gcp-phase3i-no-domain-followup.md)
- Phase 3J DuckDNS TLS attempt: [2026-05-09-gcp-phase3j-duckdns-tls-attempt.md](./2026-05-09-gcp-phase3j-duckdns-tls-attempt.md) (this artifact)
- Phase 3F authorization: [100-phase3f-conditional-sqlite-pilot-authorization.md](../100-phase3f-conditional-sqlite-pilot-authorization.md)
- Phase 3E evidence: [2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT real owned domain. DuckDNS TLS SUCCESS (after initial ACME failure). nip.io superseded as primary. Phase 3J nonprod evidence only.
