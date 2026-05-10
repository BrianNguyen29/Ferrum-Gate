# Phase 4A SendGrid AlertManager Bridge Evidence

**Date**: 2026-05-10

**Scope**: Phase 4A SendGrid AlertManager bridge live test — direct SendGrid API email and AlertManager→SendGrid webhook email delivery confirmation

**Status**: **NON-PROD evidence only**. SendGrid bridge email delivery confirmed by operator receipt. Production alerting remains PARTIAL/NO (VM-local AlertManager only). NOT production-ready, NOT full production posture, NOT PostgreSQL, NOT HA.

---

## Non-Claims

This Phase 4A evidence artifact does **not** claim:

- production-ready status
- full production posture
- production alerting capability YES (VM-local AlertManager only; no external incident management)
- PostgreSQL runtime (SQLite single-node)
- HA/multi-node deployment
- real owned domain (DuckDNS free DNS)
- Phase 4A operator signoff
- real SendGrid API key value stored (VM-local secret only, not in version control)
- real recipient email committed to repo
- GCP mutation (read-only evidence gathering)

---

## Overview

Phase 4A SendGrid bridge live test was executed on the nonprod VM. Two delivery paths were verified:

1. **Direct SendGrid REST API** — test email sent via `SENDGRID_DIRECT_HTTP` endpoint
2. **AlertManager→SendGrid webhook** — test alert `FerrumGateSendGridBridgeSuccessTest` fired through AlertManager webhook receiver

Operator confirmed receipt of both emails.

---

## Test Environment

| Field | Value | Notes |
|-------|-------|-------|
| Project | `fairy-b13f4` | GCP project |
| Region | `asia-southeast1` | GCP region |
| Zone | `asia-southeast1-a` | GCP zone |
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| Static IP | `34.158.51.8` | External IP |
| TLS Domain | `ferrumgate.duckdns.org` | DuckDNS — TLS SUCCESS (Phase 3J) |
| HTTPS URL | `https://ferrumgate.duckdns.org` | Primary endpoint |
| Database | SQLite single-node | Not PostgreSQL |
| Monitoring | Local Prometheus + AlertManager | Phase 3H deployed |
| SendGrid API Key | **VM-local secret** | `/etc/ferrumgate/secrets/sendgrid-api-key` mode 640 owner root group prometheus — NOT in version control |
| Alert Recipient | **Operator-managed** | Not committed to repo |
| Alert Sender | **Operator-managed** | Must be verified sender in SendGrid |

---

## Pre-Conditions

### AlertManager Config Validation

```
amtool check-config /etc/ferrumgate/monitoring/alertmanager-config.yaml
→ SUCCESS
```

AlertManager config includes SendGrid SMTP receiver (port 9093) and webhook receiver for the SendGrid bridge script.

### Services Status

| Service | Status | Notes |
|---------|--------|-------|
| prometheus-alertmanager | **active** | Running and ingesting alerts |
| prometheus | **active** | Scraping and alerting |
| alertmanager-sendgrid-bridge | **active** |Webhook relay script running |

---

## Historical Blocker: Sender Verification

**Blocker resolved before test**: Direct SendGrid REST API initially returned HTTP 403 due to sender identity not being verified in SendGrid.

- **Symptom**: `SENDGRID_DIRECT_HTTP=403` on first direct API attempt
- **Root cause**: Alert sender email not yet verified in SendGrid sender authentication
- **Resolution**: Sender verification propagated; subsequent direct API call returned `SENDGRID_DIRECT_HTTP=202`
- **Impact**: Test was delayed until sender verification completed; this is a standard SendGrid setup step

This blocker is documented as a historical setup item, not a bridge design issue.

---

## Test 1: Direct SendGrid REST API

### Method

Test alert `FerrumGateSendGridBridgeDirectTest` sent directly to SendGrid API v3 `/v3/mail/send` using operator-managed API key from VM-local secret.

### Result

```
SENDGRID_DIRECT_HTTP=202
```

Direct SendGrid API email delivered successfully. Operator confirmed receipt.

### Analysis

- API key stored at `/etc/ferrumgate/secrets/sendgrid-api-key` with mode 640, owner root, group prometheus
- API key value NOT documented or committed
- Sender email was verified before this test (historical blocker resolved)
- HTTP 202 indicates SendGrid accepted the message

---

## Test 2: AlertManager→SendGrid Bridge

### Method

Test alert `FerrumGateSendGridBridgeSuccessTest` fired through AlertManager. AlertManager webhook receiver forwards to SendGrid bridge relay script, which calls SendGrid API.

### Steps

1. Post test alert to AlertManager via `amtool alert` or alertmanager API
2. AlertManager evaluates and fires webhook to bridge relay
3. Bridge relay calls SendGrid REST API with alert payload
4. SendGrid delivers email to operator-managed recipient

### Configuration

- AlertManager webhook target: `http://localhost:9094/` (bridge relay endpoint)
- Bridge relay: operator-managed script reading API key from VM-local secret
- SendGrid API key: `/etc/ferrumgate/secrets/sendgrid-api-key` (mode 640, root:prometheus)

### Result

| Check | Status |
|-------|--------|
| Test alert posted | `FerrumGateSendGridBridgeSuccessTest` active in AlertManager |
| AlertManager error logs | **None** during test window |
| prometheus-alertmanager | **active** |
| prometheus | **active** |
| Alert resolved | `ALERTS=0` after resolution |
| Operator receipt | **Confirmed** — received AlertManager email |

### Analysis

AlertManager→SendGrid bridge delivered email successfully. No error logs in AlertManager during test window. Bridge relay script correctly reads API key from VM-local secret file.

---

## Combined Receipt Confirmation

Operator confirmed receipt of **both** emails:

1. Direct SendGrid API test email
2. AlertManager→SendGrid bridge test email

Both delivery paths functional.

---

## Outcome Summary

| Capability | Status | Notes |
|------------|--------|-------|
| SendGrid bridge email delivery | **YES** | Confirmed by operator receipt |
| Direct SendGrid API email | **YES** | HTTP 202 received |
| AlertManager→SendGrid webhook | **YES** | Alert fired, no error logs |
| Production alerting | **PARTIAL/NO** | VM-local AlertManager only; no external incident management |
| Production-ready | **NO** | Nonprod single-node SQLite |
| Full production posture | **NO** | DuckDNS; local monitoring only |
| PostgreSQL | **NO** | SQLite single-node |
| HA/multi-node | **NO** | Single-node only |

---

## Remaining Blockers

| Item | Status | Blocker |
|------|--------|---------|
| Production alerting | **PARTIAL** | VM-local AlertManager only; no PagerDuty/OpsGenie/external IM |
| Real owned domain | **BLOCKED** | DuckDNS free DNS; real owned domain required for production |
| Production-ready claim | **BLOCKED** | Nonprod only; single-node SQLite |
| PostgreSQL runtime | **BLOCKED** | Path 3 — not in Phase 1 scope |
| HA/multi-node | **BLOCKED** | Single-node only |

---

## References

- Phase 4A plan: [102-phase4a-ops-hardening-alert-bridge-plan.md](../102-phase4a-ops-hardening-alert-bridge-plan.md)
- Phase 4A scaffold artifact: [2026-05-09-phase4a-ops-hardening-alert-bridge-plan.md](./2026-05-09-phase4a-ops-hardening-alert-bridge-plan.md)
- Phase 3H offsite monitoring: [2026-05-09-gcp-phase3h-offsite-monitoring.md](./2026-05-09-gcp-phase3h-offsite-monitoring.md)
- Phase 3J DuckDNS TLS: [2026-05-09-gcp-phase3j-duckdns-tls-attempt.md](./2026-05-09-gcp-phase3j-duckdns-tls-attempt.md)
- Phase 3F authorization: [100-phase3f-conditional-sqlite-pilot-authorization.md](../100-phase3f-conditional-sqlite-pilot-authorization.md)
- Production readiness roadmap: [67-production-readiness-roadmap.md](../67-production-readiness-roadmap.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting YES, NOT PostgreSQL, NOT HA, NOT real SendGrid API key stored, NOT real recipient email committed. SendGrid bridge email delivery confirmed by operator receipt. Production alerting remains PARTIAL (VM-local AlertManager only). DuckDNS TLS SUCCESS. Operator-managed secrets and recipient.
