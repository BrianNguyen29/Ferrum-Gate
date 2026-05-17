# Operator Unblock Packet — 2026-05-17

> **Status**: Planning artifact. No execution claimed. No production-ready claim.
> **Purpose**: Consolidated operator-action checklist to unblock Path 2 pilot from current blocked state.
> **Scope**: Single-node SQLite v1 conditional pilot only.
> **Constraint**: `production-ready = NO` throughout. Do not execute live actions without operator signoff.

---

## Current Blocker Summary

| Blocker | Status | Owner | Unblock Condition |
|---------|--------|-------|-------------------|
| **Block A — Real owned domain** | WAIVED/CONDITIONAL | Operator | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **Block B — SendGrid API key rotation** | DONE | Operator | Completed on 2026-05-17; see `2026-05-17-sendgrid-rotation-evidence.md` |
| **Block B — Escalation matrix acknowledgment** | ACKNOWLEDGED / CLOSED | Operator | Acknowledged on 2026-05-17; SMS/webhook deferred outside current pilot scope; see `2026-05-17-escalation-matrix-acknowledgment.md` |
| **Block C — Keyless backup** | CLOSED | Operator + Engineering | C1 verified; no further action required |

---

## Block A — Real Owned Domain

### Current State
- VM external IP: `34.158.51.8`
- VM: `ferrumgate-nonprod` in `asia-southeast1-a`
- Current DNS: `ferrumgate.duckdns.org` (non-production; acceptable for exploration only)
- **DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure**

### Operator Inputs Required
- `REAL_DOMAIN`: operator-owned domain with DNS A record pointing to `34.158.51.8`

### Exact Procedure (Dry-Run by Default)
```bash
# 1. Review the runbook (dry-run / planning only)
cat docs/implementation-path/artifacts/2026-05-15-r4-production-blocker-execution-runbook.md | grep -A 30 "Block A"

# 2. When ready, execute with --confirm and your real domain:
bash scripts/gcp/phase3g_configure_real_domain.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --real-domain <REAL_DOMAIN>
```

### Evidence Gates
| Gate | Evidence | Status |
|------|----------|--------|
| G-A1 | `curl` HTTPS 200 on `https://<REAL_DOMAIN>/v1/healthz` | ☐ Pending |
| G-A2 | `curl` HTTPS 200 on `https://<REAL_DOMAIN>/v1/approvals` with bearer token | ☐ Pending |
| G-A3 | `dig` output showing `<REAL_DOMAIN>` → `34.158.51.8` | ☐ Pending |

### Rollback
Restore `/etc/caddy/Caddyfile.backup.*` on VM and reload Caddy.

---

## Block B — SendGrid API Key Rotation

### Current State
- SendGrid API key rotation was completed on 2026-05-17.
- Active AlertManager secret path: `/etc/ferrumgate/secrets/sendgrid-api-key`.
- Active secret permissions verified: `MODE=640 OWNER=root:prometheus`; directory `750 root:prometheus`.
- Synthetic alert delivery confirmed to primary and secondary inboxes.
- Old SendGrid key revoked/deleted by operator.
- Status: **DONE** — see [`2026-05-17-sendgrid-rotation-evidence.md`](./2026-05-17-sendgrid-rotation-evidence.md).

### Operator Inputs Required
- None for current pilot scope. Rotation and delivery verification are complete.

### Exact Procedure
1. Log in to SendGrid dashboard (web UI)
2. Generate new API key with scoped permissions (Mail Send, Stats)
3. Copy new key to clipboard (do NOT email or paste in docs)
4. SSH to VM: `gcloud compute ssh ferrumgate-nonprod --zone=asia-southeast1-a`
5. Back up old key (if any): `sudo cp /etc/ferrumgate/secrets/alert-provider-api-key /etc/ferrumgate/secrets/alert-provider-api-key.bak.$(date +%Y%m%d%H%M%S)`
6. Write new key without printing it to stdout or shell history:
   ```bash
   read -rsp "New SendGrid API key: " SENDGRID_API_KEY
   printf '\n'
   printf '%s' "$SENDGRID_API_KEY" | sudo tee /etc/ferrumgate/secrets/alert-provider-api-key >/dev/null
   unset SENDGRID_API_KEY
   sudo chmod 600 /etc/ferrumgate/secrets/alert-provider-api-key
   ```
7. Reload AlertManager: `sudo systemctl reload alertmanager`
8. Test synthetic alert and confirm inbox delivery
9. Revoke old key in SendGrid dashboard after confirmation

### Evidence Gates
| Gate | Evidence | Status |
|------|----------|--------|
| G-B3 | New SendGrid key active on VM; old key revoked in dashboard; test alert delivers | ✅ Done |

### Rollback
Restore old key from backup file; reload AlertManager; re-enable old key in SendGrid dashboard if not yet revoked.

---

## Block B — Escalation Matrix Acknowledgment

### Current State
- Primary and secondary email contacts configured in active AlertManager config (`/etc/prometheus/alertmanager.yml`)
- `ACTIVE_CONFIG_CHECK=PASS`, `ALERTMANAGER_SERVICE=active`, `ACTIVE_SECONDARY_PRESENT=YES`, `ACTIVE_EMAIL_TO_COUNT=4`
- G-B1 (primary inbox) and G-B2 (secondary inbox) confirmed
- Operator formally acknowledged the escalation matrix on 2026-05-17 for the current single-node SQLite pilot scope
- SMS/webhook escalation remains deferred outside current pilot scope

### Operator Inputs Required
- None for current pilot scope. Acknowledgment is complete.

### Escalation Tiers (Skeleton)

| Tier | Role | Contact | Channel | Timeout | Escalation To |
|------|------|---------|---------|---------|---------------|
| L1 — Primary on-call | *(operator to fill)* | `PRIMARY_CONTACT` | Email | 15 min (critical) / 1 hour (warning) | L2 |
| L2 — Secondary / Manager | *(operator to fill)* | `SECONDARY_CONTACT` | Email | 30 min (critical) / 2 hours (warning) | L3 |
| L3 — Engineering / Domain owner | Engineering | TBD per incident | Email or bridge channel | N/A | — |

### Acknowledgment Statement
> "I have reviewed the escalation matrix for FerrumGate v1 single-node SQLite pilot alerting. I confirm primary and secondary email contacts are configured and tested. I acknowledge additional channels (SMS/webhook) may be added later if required."
>
> Operator signature: _______________________ Date: ___________

> **Update**: Formal acknowledgment recorded on 2026-05-17 in [`2026-05-17-escalation-matrix-acknowledgment.md`](./2026-05-17-escalation-matrix-acknowledgment.md). Block B is now **CLOSED**.

---

## Engineering Hand-Off Checklist

Before operator begins Block A/B actions, engineering confirms:

- [x] `scripts/gcp/phase3g_configure_real_domain.sh` exists and is executable
- [x] Runbook R4 (`2026-05-15-r4-production-blocker-execution-runbook.md`) contains exact commands for Blocks A/B/C
- [x] `make audit` passes locally (`cargo-deny` + `cargo-audit`)
- [x] `bash scripts/run_pre_target_gate.sh --full` passes locally
- [x] No secrets or real tokens are present in this document
- [x] All live actions require `--confirm` or operator dashboard access

---

## Post-Unblock Evidence Expected

After Block A is closed (moved from WAIVED/CONDITIONAL to CLOSED) and Block B is closed, the operator must produce:

1. **Block A**: Screenshot or log showing G-A1, G-A2, G-A3 all pass
2. **Block B SendGrid**: Evidence of G-B3 pass (new key active, test alert delivered, old key revoked)
3. **Block B Escalation**: Signed escalation matrix acknowledgment
4. **Path 2 readiness refresh**: Updated `54-operator-signoff-packet.md` with Block A/B closure dates

---

## Non-Claims

- **NOT production-ready**: Closing Block A/B does not make FerrumGate production-ready.
- **NOT full G2 completion**: This packet unblocks specific operator items only.
- **NOT PostgreSQL authorization**: Single-node SQLite remains the only supported runtime.
- **NOT HA/multi-node**: Out of v1 scope.

---

*Packet created: 2026-05-17. Operator unblock packet — planning artifact only. No execution claimed.*
