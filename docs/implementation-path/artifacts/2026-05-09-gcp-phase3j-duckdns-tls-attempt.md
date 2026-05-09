# GCP Phase 3J DuckDNS TLS Attempt Artifact

**Date**: 2026-05-09

**Scope**: Phase 3J DuckDNS free-domain TLS configuration attempt — ACME certificate issuance blocked by DNS/CAA validation instability; rolled back to nip.io

**Status**: **NON-PROD evidence only**. DuckDNS DNS A record working. DuckDNS HTTPS/TLS **BLOCKED**. nip.io rollback successful.

---

## Non-Claims

This Phase 3J follow-up does **not** claim:

- production-ready status
- full production posture
- production alerting capability
- real domain deployment (nip.io temporary domain still in use)
- full G2 completion beyond Phase 3F conditional single-node SQLite pilot scope
- full production pilot authorization
- Phase 3J operator signoff
- PostgreSQL runtime, HA, or multi-node deployment
- DuckDNS TLS success (TLS FAILED — ACME blocked)
- real owned domain (DuckDNS is free, no-ownership-verification DNS)

Phase 3J documents a **failed TLS attempt** via DuckDNS free domain. Rollback to nip.io successful.

---

## Overview

Phase 3J captures:

1. **DuckDNS DNS Resolution**: DNS A record now correctly resolves to VM static IP (34.158.51.8)
2. **DuckDNS TLS Attempt**: Ran `phase3g_configure_real_domain.sh` with `--real-domain ferrumgate.duckdns.org`; Caddy served challenges but ACME certificate issuance blocked by DNS/CAA instability
3. **Rollback to nip.io**: Caddy reverted to `34-158-51-8.nip.io`; all health checks pass

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

### Actions Taken

1. Updated Caddyfile for `ferrumgate.duckdns.org`
2. Validated Caddyfile (`caddy validate`)
3. Reloaded Caddy service (`caddy reload`)

### Result: TLS FAILED

HTTPS checks to `https://ferrumgate.duckdns.org/v1/healthz/readyz/deep/metrics` failed:

```
curl error 35: tlsv1 alert internal error
```

### Caddy Logs — ACME Failure Evidence

Caddy logs showed Let's Encrypt ACME validation failures:

```
# Primary attempt — tls-alpn-01 and http-01 challenges served, secondary validation failed:
"query timed out looking up CAA for ferrumgate.duckdns.org"
"SERVFAIL looking up CAA for ferrumgate.duckdns.org"

# Staging retry — also failed:
"query timed out looking up CAA for duckdns.org"
"query timed out looking up A for ferrumgate.duckdns.org"
"no valid AAAA records"
```

**Root Cause**: DuckDNS free DNS service has unstable DNS resolution for CAA lookups and A record propagation. Let's Encrypt secondary DNS validation (CAA check) timed out, blocking certificate issuance.

---

## Rollback to nip.io

### Actions Taken

1. Reverted Caddyfile to `34-158-51-8.nip.io` (simple Caddyfile)
2. Validated Caddyfile (`caddy validate`)
3. Reloaded Caddy service (`caddy reload`)

### Validation

```
caddy validate: PASSED
caddy service: active
```

---

## Post-Rollback nip.io Health Checks

| Endpoint | Status | Response |
|----------|--------|----------|
| `https://34-158-51-8.nip.io/v1/healthz` | 200 | `{"status":"ok"}` |
| `https://34-158-51-8.nip.io/v1/readyz` | 200 | `{"status":"ready"}` |
| `https://34-158-51-8.nip.io/v1/metrics` | 200 | `ferrumgate_store_health_up 1`<br>`ferrumgate_write_queue_depth 0` |

**Result**: All health checks PASSED. nip.io TLS working.

---

## Target Environment (Post-Rollback)

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` (temporary — **real domain BLOCKED**) |
| HTTPS URL | `https://34-158-51-8.nip.io` |
| Database | SQLite single-node |
| Auth Mode | Bearer token |
| Monitoring | Local-only (Prometheus + AlertManager on-VM) |
| Alert Contact | **None** (local-only mode) |

---

## Claim Status Summary

| Claim | Status | Evidence |
|-------|--------|----------|
| DuckDNS DNS A record resolves to VM | **YES** | `getent ahostsv4 ferrumgate.duckdns.org` -> `34.158.51.8` |
| DuckDNS HTTPS/TLS working | **NO / BLOCKED** | curl error 35; ACME CAA lookup timeout |
| nip.io rollback successful | **YES** | All `/v1/healthz`, `/v1/readyz`, `/v1/metrics` returned 200 |
| Real owned domain | **NO** | DuckDNS is free DNS; no ownership verification |
| Production-ready | **NO** | Nonprod only; single-node SQLite |
| Production alerting | **NO** | Local-only mode; no alert contact |
| Full production posture | **NO** | Nonprod evidence only |

---

## What Phase 3J is NOT

Phase 3J does NOT:

- Claim DuckDNS TLS success (TLS failed — ACME blocked)
- Claim real owned domain (DuckDNS is free, no-ownership-verification DNS)
- Claim production-ready or full production posture
- Include production alerting capability (local-only mode)
- Include service account keys, tokens, or email literals
- Modify Phase 3F conditional pilot scope
- Replace Phase 3E operator signoff

---

## Remaining Blockers (Unchanged)

| Item | Status | Blocker |
|------|--------|---------|
| Real domain TLS | **BLOCKED** | DuckDNS free DNS too unstable for ACME CAA validation; real owned domain required |
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

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT real domain deployment. DuckDNS TLS BLOCKED. nip.io rollback successful. Real owned domain required for TLS. Phase 3J nonprod evidence only.