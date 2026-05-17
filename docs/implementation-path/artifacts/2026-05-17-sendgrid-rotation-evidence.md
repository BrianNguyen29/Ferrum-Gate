# Artifact: 2026-05-17 SendGrid API Key Rotation Evidence

> **Type**: Evidence artifact (operator-confirmed actions, not a readiness claim)
> **Date**: 2026-05-17
> **Scope**: SendGrid API key rotation, permission root-cause remediation, delivery verification, old-key revocation, and SSH firewall restoration
> **Status**: Evidence recorded. No production-ready claim. No pilot-ready claim.
> **Secret handling**: No API key value, token prefix, bearer header, or email address is recorded in this artifact.

---

## Summary

This artifact documents operator-confirmed SendGrid rotation and remediation:

- Operator created a new SendGrid API key via hidden prompt; the key value was never exposed in chat or docs.
- The new key was deployed to the active AlertManager secret path on the VM.
- A permission root cause was identified and fixed (`/etc/ferrumgate/secrets` directory and file ownership/permissions).
- A synthetic alert (`FerrumGateSendGridDirPermFix`) was triggered and confirmed delivered to both primary and secondary email inboxes.
- The old SendGrid API key was revoked/deleted by the operator.
- The SSH firewall rule was restored to its pre-rotation source range.

---

## 1. Secret Deployment Evidence

### Active Secret Path

| Field | Value |
|-------|-------|
| `secret_path` | `/etc/ferrumgate/secrets/sendgrid-api-key` |
| `alertmanager_configured_path` | `/etc/ferrumgate/secrets/sendgrid-api-key` |

### File Metadata (observed)

| Field | Value |
|-------|-------|
| `mtime_utc` | 2026-05-17 14:22:14.430663339 +0000 |
| `file_mode` | 640 (`-rw-r-----`) |
| `file_owner` | root:prometheus |
| `directory_mode` | 750 (`drwxr-x---`) |
| `directory_owner` | root:prometheus |

### Service Status

| Field | Value |
|-------|-------|
| `prometheus-alertmanager.service` | active |

---

## 2. Root Cause and Fix

### Root Cause

AlertManager was configured to read the secret from `/etc/ferrumgate/secrets/sendgrid-api-key`.

The initial rotation wrote the key to `/etc/ferrumgate/secrets/alert-provider-api-key` instead of the active path. After correcting the path, AlertManager still encountered a permission denied error because the `/etc/ferrumgate/secrets` directory lacked group traverse/read permissions for the `prometheus` user.

### Fix Applied

1. Copied the key to the active path: `/etc/ferrumgate/secrets/sendgrid-api-key`.
2. Set directory ownership and permissions:
   - Owner: `root:prometheus`
   - Directory mode: `750` (`drwxr-x---`)
3. Set file ownership and permissions:
   - Owner: `root:prometheus`
   - File mode: `640` (`-rw-r-----`)

---

## 3. Delivery Verification

### Synthetic Alert Triggered

| Field | Value |
|-------|-------|
| `alert_name` | `FerrumGateSendGridDirPermFix` |
| `severity` | critical |
| `service` | ferrumgate |
| `instance` | ferrumgate-nonprod |

### Inbox Confirmation

| Path | Confirmed |
|------|-----------|
| Primary email inbox | Yes (operator confirmed) |
| Secondary email inbox | Yes (operator confirmed) |

No email addresses or message contents are recorded in this artifact.

---

## 4. Old Key Revocation

| Field | Value |
|-------|-------|
| `old_key_revoked` | Yes (operator confirmed) |
| `old_key_deleted_from_sendgrid` | Yes (operator confirmed) |

No key ID, key prefix, or API key value is recorded.

---

## 5. SSH Firewall Restoration

| Field | Value |
|-------|-------|
| `firewall_rule_name` | `ferrumgate-nonprod-fw-ssh` |
| `source_ranges_restored` | `118.69.4.63/32` |

---

## 6. Status Impact

### G-B3 — Bearer Token / API Key Rotation

- **Previous status**: Partial (bearer token rotation passed; SendGrid API key rotation pending)
- **Current status**: **Verified / Closed**
  - SendGrid API key rotated successfully.
  - Old key revoked.
  - Active secret path permissions corrected.
  - Synthetic alert delivery confirmed to primary and secondary inboxes.

### Block B — Off-VM Alerting

- **Status at time of writing**: **PARTIAL**
  - G-B1 (primary inbox delivery): Confirmed
  - G-B2 (secondary inbox delivery): Confirmed
  - G-B3 (bearer token + SendGrid API key rotation): **Verified / Closed**
  - G-B4 (escalation matrix populated for primary+secondary email path): Populated enough for primary+secondary path
  - **Remaining blocker at time of writing**: Formal escalation matrix acknowledgment by the operator was still pending.

> **Update**: Escalation matrix acknowledgment was formally recorded on 2026-05-17 in `docs/implementation-path/artifacts/2026-05-17-escalation-matrix-acknowledgment.md`. Block B is now **CLOSED**.

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- SendGrid API key was rotated by the operator without exposing the key value.
- The active secret path has correct ownership (`root:prometheus`) and permissions (`dir=750`, `file=640`).
- A permission root cause was identified and fixed.
- A synthetic alert was successfully delivered to both primary and secondary email inboxes.
- The old SendGrid API key was revoked/deleted.
- The SSH firewall rule was restored to its expected source range.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| Production-ready | **NO** | No production-ready claim is made. |
| Pilot-ready | **NO** | Block A is WAIVED/CONDITIONAL (not CLOSED); real owned domain remains required for production-ready or full G2 closure. |
| Escalation matrix formally acknowledged | **YES** | Recorded in `2026-05-17-escalation-matrix-acknowledgment.md` (2026-05-17). |
| Block B complete | **YES** | Closed per `2026-05-17-escalation-matrix-acknowledgment.md`. |

---

## Cross-References

| Document | Purpose |
|----------|---------|
| `docs/implementation-path/01-current-state.md` | Canonical current state and blocker status |
| `docs/implementation-path/artifacts/2026-05-13-d1-d6-target-host-evidence.md` | Prior target-host drill evidence (SSH firewall baseline) |
| `docs/implementation-path/102-phase4a-ops-hardening-alert-bridge-plan.md` | SendGrid AlertManager bridge template and setup plan |

---

*Artifact created: 2026-05-17. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim.*
