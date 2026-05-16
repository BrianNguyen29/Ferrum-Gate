# 122 — Completion Roadmap and Hardening Tracker

> **Status**: Planning tracker. No execution claimed. No production-ready claim.
> **Purpose**: Durable todo list for the 10 requested completion items and hardening tasks following May 13–16 evidence.
> **Scope**: Single-node SQLite v1 conditional pilot. Docs-only tracker.
> **Constraint**: `production-ready = NO`. Block A remains blocked. No secrets.

---

## Executive Summary

Following May 13–16 operator execution and evidence collection, plus May 16 engineering updates:
- **Block C (keyless backup)**: CLOSED — C1 path verified, residual key removed, offsite sync confirmed.
- **Block B (off-VM alerting)**: PARTIAL — operator confirmed inbox receipt for at least one contact (G-B1 partial); secondary contact delivery confirmed (TEST_ID `fg-secondary-check-20260516-153221`, G-B2); escalation matrix populated enough for primary+secondary email path (G-B4). G-B3 bearer token rotation executed on VM (new token 200, old token 401, ROTATION_RESULT=PASS); SendGrid API key rotation remains pending/operator-blocked.
- **Block A (real domain)**: BLOCKED — operator confirmed no real owned domain and no DNS configuration available yet.
- **P0 items**: All closed (CI hardened, D1–D6 passed, restore drill passed, backup automation verified, G2 signed, operator signoff obtained).
- **Engineering items 7–9**: Completed — ferrum-cap fix verified (atomic `update_status_if_active`, gateway durable path wired, 9 tests pass); local/manual security audit gate added (`scripts/run_security_audit.sh` + `make audit`); `cargo-audit v0.22.1` and `cargo-deny v0.19.6` installed; `make audit` passes with both tools (cargo-deny advisories ok, cargo-audit 384 dependencies scanned, PASS; SECURITY AUDIT GATE: PASS).
- **Production posture**: `production-ready = NO`; PostgreSQL production = `NO`; HA/multi-node = `NO`.

---

## Tracker Items

| # | Item | Owner | Status | Blocker | Evidence | Next Action |
|---|------|-------|--------|---------|----------|-------------|
| 1 | Commit/push docs commit `801eb59` | Engineering | ✅ Done | — | Pushed to origin/main by orchestrator | — |
| 2 | Update AGENTS.md stale status | Engineering | ✅ Done | — | This tracker and doc updates reflect current state | — |
| 3 | Refresh `01-current-state.md` per May 13–16 evidence | Engineering | ✅ Done | — | Updated with Block A/B/C statuses and closed P0 items | — |
| 4 | Create Block B escalation matrix | Operator | 🟡 Populated for primary+secondary email path | Operator must define contacts/channels | Primary and secondary email contacts configured in active AlertManager config; skeleton below; full template in `R1` artifact §4 | Operator acknowledges matrix and adds additional channels (SMS/webhook) if required |
| 5 | Run key rotation drill (SendGrid + bearer token) | Operator | 🟡 Partial — bearer token rotated; SendGrid key rotation pending | Bearer token rotation executed on VM (new token 200, old token 401, ROTATION_RESULT=PASS); SendGrid API key rotation requires dashboard/API credential workflow and secure operator channel | Rotation procedure in `R1` artifact §4.1 and `70-security-hardening-local-only-plan.md` §Token Rotation Procedure | Operator generates new SendGrid key via secure dashboard workflow and rotates VM secret at `/etc/ferrumgate/secrets/alert-provider-api-key` |
| 6 | Confirm secondary alert contact inbox delivery | Operator | ✅ Done | Secondary contact configured in active AlertManager config (`/etc/prometheus/alertmanager.yml`); `ACTIVE_CONFIG_CHECK=PASS`, `ALERTMANAGER_SERVICE=active`, `ACTIVE_SECONDARY_PRESENT=YES`, `ACTIVE_EMAIL_TO_COUNT=4`; synthetic alert posted (`ALERT_POST_HTTP=200`, `ALERT_VISIBLE=YES`, TEST_ID `fg-secondary-check-20260516-153221`, START_AT_UTC `2026-05-16T15:32:21Z`); operator confirmed secondary inbox receipt | G-B2 gate in `67-production-readiness-roadmap.md` | — |
| 7 | Oracle review ferrum-cap single-use durability/concurrency | Engineering | ✅ Done | — | Fix verified: atomic `update_status_if_active` for SQLite/Postgres; gateway durable path wired; risk documented as accepted for v1 | Post-v1: durable capability persistence (revocation list survives restart) remains deferred to Phase 3 |
| 8 | Add ferrum-cap tests | Engineering | ✅ Done | — | 9 tests pass (4 TTL boundaries + 5 mark_used paths: success, already_used, concurrent_single_use, revoked, expired) | — |
| 9 | Add local/manual cargo-audit or cargo-deny gate | Engineering | ✅ Done | — | `cargo-audit v0.22.1` and `cargo-deny v0.19.6` installed; `scripts/run_security_audit.sh` created; `make audit` target added; checks for `cargo-deny` and `cargo-audit`, runs available tools, fails with install instructions if neither present; **dual-tool PASS** (cargo-deny advisory DB fetched, advisories ok; cargo-audit loaded 1090 advisories, scanned 384 dependencies, 0 actionable issues); `RUSTSEC-2023-0071` ignored because the affected crate path (`rsa` via `sqlx-mysql`) is an uncompiled optional dependency blocked by `default-features = false` on `sqlx` | — |
| 10 | Run Block A domain/TLS path when real domain exists | Operator | ☐ Blocked | No real owned domain or DNS available | `scripts/gcp/phase3g_configure_real_domain.sh` ready; requires `REAL_DOMAIN` + DNS A record → `34.158.51.8` | Operator procures domain, configures DNS A record, then executes Block A runbook (`R4` §A) |

---

## Block Status Detail

### Block A — Real Owned Domain

| Attribute | State |
|-----------|-------|
| Status | **BLOCKED** |
| Blocker | Operator has no real owned domain and no DNS configuration available yet |
| VM endpoint | `ferrumgate.duckdns.org` (non-production) |
| Static IP | `34.158.51.8` |
| Script ready | `scripts/gcp/phase3g_configure_real_domain.sh` |
| Unblock condition | Operator procures domain + configures DNS A record → `34.158.51.8` |

### Block B — Off-VM Alerting

| Gate | Status | Evidence |
|------|--------|----------|
| G-B1 | 🟡 Partial | Operator confirmed inbox receipt of `TEST_ID=fg-inbox-check-20260516-052910` for at least one contact; email content verified (subject `FerrumGate Alert: FerrumGateInboxDeliveryCheck`, status `resolved`, severity `warning`, service `ferrumgate`) |
| G-B2 | ✅ Done | Secondary contact configured in active AlertManager config; `ACTIVE_CONFIG_CHECK=PASS`, `ALERTMANAGER_SERVICE=active`, `ACTIVE_SECONDARY_PRESENT=YES`, `ACTIVE_EMAIL_TO_COUNT=4`; synthetic alert posted (`ALERT_POST_HTTP=200`, `ALERT_VISIBLE=YES`, TEST_ID `fg-secondary-check-20260516-153221`, START_AT_UTC `2026-05-16T15:32:21Z`); operator confirmed secondary inbox receipt |
| G-B3 | 🟡 Partial | Bearer token rotation executed on VM (new token 200, old token 401, ROTATION_RESULT=PASS); SendGrid API key rotation remains pending/operator-blocked |
| G-B4 | 🟡 Populated for primary+secondary email path | Escalation matrix skeleton exists; primary and secondary email contacts are configured in active AlertManager config; full operator acknowledgment and additional channels (SMS/webhook) remain pending |

### Block C — Keyless Backup

| Attribute | State |
|-----------|-------|
| Status | **CLOSED** |
| Path selected | C1 (stop-start VM with `set-service-account`) |
| Scope update | `devstorage.read_write` added successfully |
| Keyless probe | PASS (isolated HOME, no key env) |
| Offsite sync | PASS (`OFFSITE_SYNC_RC=0`, 15.3 MiB copied) |
| Residual key | Removed (`/etc/ferrumgate/gcs-service-account.json` ABSENT) |
| Machine type | `n2-standard-2` (temporary; `e2-medium` revert deferred due to `ZONE_RESOURCE_POOL_EXHAUSTED`) |

---

## Block B Escalation Matrix Skeleton

> **Status**: Skeleton only. No contacts fabricated. Operator must populate.
> **Owner**: Operator
> **Blocked until**: Operator provides primary contact, secondary contact, and preferred channels.

### Escalation Tiers

| Tier | Role | Contact | Channel | Timeout | Escalation To |
|------|------|---------|---------|---------|---------------|
| L1 — Primary on-call | *(operator to fill)* | `PRIMARY_CONTACT` | *(operator to fill: email/SMS/webhook)* | 15 min (critical) / 1 hour (warning) | L2 |
| L2 — Secondary / Manager | *(operator to fill)* | `SECONDARY_CONTACT` | *(operator to fill: email/SMS/webhook)* | 30 min (critical) / 2 hours (warning) | L3 |
| L3 — Engineering / Domain owner | Engineering | TBD per incident | Email or bridge channel | N/A | — |

### Severity Routing

| Severity | L1 Action | L1 Timeout | L2 Action | L2 Timeout | L3 Action |
|----------|-----------|------------|-----------|------------|-----------|
| `critical` | Page/call L1 | 15 min | Page/call L2 | 30 min | Eng bridge if unacknowledged |
| `warning` | Email/message L1 | 1 hour | Email/message L2 | 2 hours | Log + review at next standup |
| `info` | Log only | N/A | N/A | N/A | N/A |

### Required Operator Inputs

| Input | Status | Description |
|-------|--------|-------------|
| `PRIMARY_CONTACT` | ☐ Pending — operator-blocked | Email, phone, or webhook URL for L1 |
| `SECONDARY_CONTACT` | ☐ Pending — operator-blocked | Email, phone, or webhook URL for L2 |
| `ALERT_PROVIDER` | ☐ Pending — operator-blocked | SendGrid, SES, PagerDuty, Slack, SMTP relay |
| `ALERT_SENDER` | ☐ Pending — operator-blocked | Verified sender identity for email providers |
| Acknowledgment signature | ☐ Pending — operator-blocked | Operator signs and dates matrix acknowledgment |

### Non-Claims

- **NOT production-ready**: This skeleton does not constitute live alerting configuration.
- **NOT real contacts stored**: All contact fields are placeholders; no PII in version control.
- **NOT acknowledged**: Operator has not yet reviewed, populated, or signed this matrix.

---

## Cross-References

| Document | Purpose |
|----------|---------|
| [`67-production-readiness-roadmap.md`](./67-production-readiness-roadmap.md) | Authoritative blocker status and evidence gates |
| [`artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md`](./artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md) | C1 execution evidence and Block B SendGrid smoke-test state |
| [`artifacts/2026-05-15-r1-alerting-rotation-policy.md`](./artifacts/2026-05-15-r1-alerting-rotation-policy.md) | Block B rotation policy and escalation matrix template |
| [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) | Exact command sequences for Blocks A/B/C |
| [`70-security-hardening-local-only-plan.md`](./70-security-hardening-local-only-plan.md) | Token rotation procedure and local audit commands |
| [`01-current-state.md`](./01-current-state.md) | Current engineering and operator status |
| [`AGENTS.md`](../../AGENTS.md) | Repo toolchain, invariants, and verification status |

---

## Non-Claims

- **NOT production-ready**: This tracker does not make FerrumGate production-ready.
- **NOT full production posture**: Block A (real domain) remains blocked. Block B G-B2 secondary contact confirmed; G-B3 bearer token rotation done; SendGrid API key rotation remains pending. G-B4 escalation matrix populated for primary+secondary email path but not fully acknowledged.
- **NOT PostgreSQL production**: Remains deferred; single-node SQLite only.
- **NOT HA/multi-node**: Out of v1 scope.
- **NOT full P0/G2 production claim**: Primary and secondary email delivery confirmed; additional channels (SMS/webhook) and operator acknowledgment remain pending.
- **NOT SendGrid key rotation executed**: Item 5 bearer token rotation done; SendGrid API key rotation remains pending/operator-blocked.
- **NOT full security audit**: `make audit` passes with both cargo-deny and cargo-audit. This is a local/manual gate, not CI.

---

*Tracker created: 2026-05-16. Completion roadmap and hardening tracker — planning artifact only. No execution claimed.*
