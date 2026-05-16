# R1 — Alerting Rotation Policy (Off-VM Alerting)

> **Status**: Operator-owned appendix. NOT production-ready. NOT live alerting configured.
> **Purpose**: Define rotation, escalation, and provider-failover policy for off-VM alerting before unattended operation.
> **Scope**: Single-node SQLite v1 conditional pilot. Docs-only; no secrets; no live mutation.
> **Blocked until**: Operator provides real contact/channel, provider account, and API key.

---

## 1. Policy Summary

Before FerrumGate can run unattended, alerting must reach an off-VM channel with confirmed delivery.
This document defines the rotation policy for contacts, API keys, and provider selection.
It does NOT configure live alerting and does NOT store any API key.

---

## 2. Current State

| Field | Value | Notes |
|-------|-------|-------|
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| External IP | `34.158.51.8` | Static IP |
| Monitoring stack | Prometheus + AlertManager (VM-local) | Phase 3H deployed |
| Off-VM alert delivery | **NONE** | Local-only mode |
| SendGrid bridge template | `configs/monitoring/alertmanager-sendgrid-bridge.example.yaml` | Placeholder only; no real key |
| Prior non-prod delivery evidence | Direct API test + AlertManager webhook delivered | Doc `artifacts/2026-05-09-gcp-phase3h-offsite-monitoring.md` |

---

## 3. Required Operator Inputs (All Gated)

| Input | Status | Description | Placeholder in Commands |
|-------|--------|-------------|------------------------|
| Alert provider | **REQUIRED** | SendGrid, Amazon SES, PagerDuty, Slack webhook, or SMTP relay | `ALERT_PROVIDER` |
| Provider API key / token | **REQUIRED** | Stored VM-locally at `/etc/ferrumgate/secrets/`; NEVER in repo | `PROVIDER_API_KEY` |
| Primary contact | **REQUIRED** | Email address or webhook URL for critical alerts | `PRIMARY_CONTACT` |
| Secondary contact | **REQUIRED** | Escalation path when primary does not acknowledge | `SECONDARY_CONTACT` |
| Sender identity | **REQUIRED** | Verified sender domain/email (for email providers) | `ALERT_SENDER` |

---

## 4. Rotation Policy

### 4.1 API Key / Token Rotation

| Aspect | Policy |
|--------|--------|
| Rotation trigger | 90 days max age; on suspected compromise; on personnel change |
| Rotation procedure | 1) Generate new key at provider console<br>2) Write new key to VM secret file<br>3) Reload AlertManager<br>4) Send test alert<br>5) Revoke old key at provider console |
| Evidence required | Screenshot or log of test alert delivery after rotation |
| Rollback | Restore previous key file from backup; reload AlertManager |

**Exact command sequence (placeholder)**:

```bash
# 1. Generate new key at provider console (operator action, not scripted)
# 2. Write new key on VM
sudo mkdir -p /etc/ferrumgate/secrets
sudo chmod 700 /etc/ferrumgate/secrets
sudo sh -c 'cat > /etc/ferrumgate/secrets/alert-provider-api-key' << 'EOF'
PROVIDER_API_KEY
EOF
sudo chmod 600 /etc/ferrumgate/secrets/alert-provider-api-key
sudo chown root:root /etc/ferrumgate/secrets/alert-provider-api-key

# 3. Reload AlertManager
curl -X POST http://localhost:9093/-/reload

# 4. Send test alert (via AlertManager API or amtool)
amtool alert add alertname=TestAlert severity=critical \
  --alertmanager.url=http://localhost:9093

# 5. Revoke old key at provider console (operator action)
```

### 4.2 Contact Rotation

| Aspect | Policy |
|--------|--------|
| Primary contact | Single owner; rotate quarterly or on on-call rotation change |
| Secondary contact | Manager or secondary on-call; never the same person as primary |
| Update procedure | Edit `/etc/ferrumgate/monitoring/alertmanager-config.yaml`; replace `PRIMARY_CONTACT` and `SECONDARY_CONTACT`; reload AlertManager |
| Evidence required | Test alert delivered to both primary and secondary within 5 minutes |

### 4.3 Provider Failover

| Aspect | Policy |
|--------|--------|
| Preferred | One primary provider (e.g., SendGrid) |
| Fallback | Secondary provider or SMTP relay to operator-managed mail server |
| Trigger for failover | Primary provider API down for >15 minutes; key revoked unexpectedly; rate limit exceeded |
| Failover procedure | Swap AlertManager receiver to fallback provider config; reload; verify delivery |

---

## 5. Escalation Matrix

| Severity | Primary Action | Timeout | Escalation Action |
|----------|---------------|---------|-------------------|
| `critical` | Page primary contact immediately | 15 minutes | Page secondary contact |
| `warning` | Email primary contact | 1 hour | Email secondary contact |
| `info` | Log only; no page | N/A | N/A |

**AlertManager routing snippet (placeholder)**:

```yaml
route:
  group_by: ['alertname', 'severity']
  receiver: 'primary'
  routes:
    - match:
        severity: critical
      receiver: 'primary-critical'
      repeat_interval: 1h
    - match:
        severity: warning
      receiver: 'primary-warning'
      repeat_interval: 4h

receivers:
  - name: 'primary-critical'
    # Operator replaces with real webhook/email config
  - name: 'primary-warning'
    # Operator replaces with real webhook/email config
```

---

## 6. Rollback

| Action | Rollback Command |
|--------|-----------------|
| Bad provider config | Restore `/etc/ferrumgate/monitoring/alertmanager-config.yaml.backup.*`; `curl -X POST http://localhost:9093/-/reload` |
| Bad key rotation | Restore previous secret file from backup; reload AlertManager |
| Disable off-VM alerting | Comment out external receivers in AlertManager config; reload |

---

## 7. Evidence Gates (Before Marking Block B Closed)

| Gate | Evidence Required |
|------|-------------------|
| G-B1 | Alert delivered to `PRIMARY_CONTACT` from VM via chosen provider (screenshot or `amtool` output) |
| G-B2 | Alert delivered to `SECONDARY_CONTACT` from VM via chosen provider |
| G-B3 | Key rotation procedure executed at least once in non-prod (log of generation → test → revoke) |
| G-B4 | Escalation matrix documented and acknowledged by operator |

---

## 8. Non-Claims

- NOT production-ready
- NOT live alerting configured
- NOT real API key stored in repo
- NOT real contact email stored in repo
- NOT SendGrid-specific; operator may choose any provider
- This is a policy document only; actual configuration is operator-owned

---

*Artifact created: 2026-05-15. Alerting rotation policy — docs-only, no secrets, no live configuration.*
