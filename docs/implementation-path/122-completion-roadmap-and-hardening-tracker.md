# 122 — Completion Roadmap and Hardening Tracker

> **Status**: Planning tracker. No execution claimed. No production-ready claim.
> **Purpose**: Durable todo list for the 10 requested completion items and hardening tasks following May 13–16 evidence.
> **Scope**: Single-node SQLite v1 conditional pilot. Docs-only tracker.
> **Constraint**: `production-ready = NO`. Block A WAIVED/CONDITIONAL for single-node SQLite pilot only. No secrets.

---

## Executive Summary

Following May 13–16 operator execution and evidence collection, plus May 16–18 engineering updates:
- **Block C (keyless backup)**: CLOSED — C1 path verified, residual key removed, offsite sync confirmed.
- **Block B (off-VM alerting)**: CLOSED — operator confirmed primary and secondary inbox delivery (G-B1/G-B2); G-B3 verified/closed (bearer token rotation + SendGrid API key rotation, primary+secondary delivery confirmed, old SendGrid key revoked/deleted); G-B4 formally acknowledged on 2026-05-17.
- **Block A (real domain)**: WAIVED/CONDITIONAL — DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; operator acknowledged Path A conditional pilot closure on 2026-05-18 with no real domain available; real owned domain still required for production-ready or full G2 closure. See `artifacts/2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md`
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
| 4 | Create Block B escalation matrix | Operator | ✅ Done — formally acknowledged for primary+secondary email path | Operator acknowledged primary/secondary paths and timeout targets on 2026-05-17; SMS/webhook deferred | Primary and secondary email contacts configured in active AlertManager config; acknowledgment artifact recorded | No further action for current pilot scope |
| 5 | Run key rotation drill (SendGrid + bearer token) | Operator | ✅ Done — bearer token rotated; SendGrid key rotation verified | Bearer token rotation executed on VM; SendGrid API key rotation verified on VM with active secret path `/etc/ferrumgate/secrets/sendgrid-api-key`, primary+secondary delivery confirmed, and old key revoked/deleted | Rotation procedure in `R1` artifact §4.1 and `70-security-hardening-local-only-plan.md` §Token Rotation Procedure; evidence in `artifacts/2026-05-17-sendgrid-rotation-evidence.md` | No further action for G-B3 |
| 6 | Confirm secondary alert contact inbox delivery | Operator | ✅ Done | Secondary contact configured in active AlertManager config (`/etc/prometheus/alertmanager.yml`); `ACTIVE_CONFIG_CHECK=PASS`, `ALERTMANAGER_SERVICE=active`, `ACTIVE_SECONDARY_PRESENT=YES`, `ACTIVE_EMAIL_TO_COUNT=4`; synthetic alert posted (`ALERT_POST_HTTP=200`, `ALERT_VISIBLE=YES`, TEST_ID `fg-secondary-check-20260516-153221`, START_AT_UTC `2026-05-16T15:32:21Z`); operator confirmed secondary inbox receipt | G-B2 gate in `67-production-readiness-roadmap.md` | — |
| 7 | Oracle review ferrum-cap single-use durability/concurrency | Engineering | ✅ Done | — | Fix verified: atomic `update_status_if_active` for SQLite/Postgres; gateway durable path wired; risk documented as accepted for v1 | Post-v1: durable capability persistence (revocation list survives restart) remains deferred to Phase 3 |
| 8 | Add ferrum-cap tests | Engineering | ✅ Done | — | 9 tests pass (4 TTL boundaries + 5 mark_used paths: success, already_used, concurrent_single_use, revoked, expired) | — |
| 9 | Add local/manual cargo-audit or cargo-deny gate | Engineering | ✅ Done | — | `cargo-audit v0.22.1` and `cargo-deny v0.19.6` installed; `scripts/run_security_audit.sh` created; `make audit` target added; checks for `cargo-deny` and `cargo-audit`, runs available tools, fails with install instructions if neither present; **dual-tool PASS** (cargo-deny advisory DB fetched, advisories ok; cargo-audit loaded 1090 advisories, scanned 384 dependencies, 0 actionable issues); `RUSTSEC-2023-0071` ignored because the affected crate path (`rsa` via `sqlx-mysql`) is an uncompiled optional dependency blocked by `default-features = false` on `sqlx` | — |
| 10 | Run Block A domain/TLS path when real domain exists | Operator | ☐ WAIVED/CONDITIONAL — real domain still required for production-ready or full G2 closure | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; Path A conditional pilot closure acknowledged on 2026-05-18 with no real domain available. See `artifacts/2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md` | `scripts/gcp/phase3g_configure_real_domain.sh` ready; requires `REAL_DOMAIN` + DNS A record → `34.158.51.8` | Operator procures domain, configures DNS A record, then executes Block A runbook (`R4` §A) to move Block A from WAIVED to CLOSED |

---

## Block Status Detail

### Block A — Real Owned Domain

| Attribute | State |
|-----------|-------|
| Status | **WAIVED/CONDITIONAL** |
| Blocker | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| VM endpoint | `ferrumgate.duckdns.org` (non-production) |
| Static IP | `34.158.51.8` |
| Script ready | `scripts/gcp/phase3g_configure_real_domain.sh` |
| Unblock condition | Operator procures domain + configures DNS A record → `34.158.51.8` |

### Block B — Off-VM Alerting

| Gate | Status | Evidence |
|------|--------|----------|
| G-B1 | ✅ Done | Operator confirmed primary inbox receipt of `TEST_ID=fg-inbox-check-20260516-052910`; email content verified (subject `FerrumGate Alert: FerrumGateInboxDeliveryCheck`, status `resolved`, severity `warning`, service `ferrumgate`) |
| G-B2 | ✅ Done | Secondary contact configured in active AlertManager config; `ACTIVE_CONFIG_CHECK=PASS`, `ALERTMANAGER_SERVICE=active`, `ACTIVE_SECONDARY_PRESENT=YES`, `ACTIVE_EMAIL_TO_COUNT=4`; synthetic alert posted (`ALERT_POST_HTTP=200`, `ALERT_VISIBLE=YES`, TEST_ID `fg-secondary-check-20260516-153221`, START_AT_UTC `2026-05-16T15:32:21Z`); operator confirmed secondary inbox receipt |
| G-B3 | ✅ Done | Bearer token rotation executed on VM; SendGrid API key rotation verified on VM with active secret path permissions fixed, synthetic alert delivered to primary+secondary inboxes, old key revoked/deleted. See `artifacts/2026-05-17-sendgrid-rotation-evidence.md` |
| G-B4 | ✅ Done | Escalation matrix formally acknowledged on 2026-05-17 for primary/secondary email path; SMS/webhook deferred. See `artifacts/2026-05-17-escalation-matrix-acknowledgment.md` |

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

> **Status**: Primary and secondary email contacts configured in active AlertManager config; operator acknowledgment recorded on 2026-05-17. SMS/webhook remain deferred outside current pilot scope.
> **Owner**: Operator
> **Closed by**: `artifacts/2026-05-17-escalation-matrix-acknowledgment.md` for current single-node SQLite pilot scope.

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
| `PRIMARY_CONTACT` | ✅ Configured in active AlertManager config | Email configured in `/etc/prometheus/alertmanager.yml` |
| `SECONDARY_CONTACT` | ✅ Configured in active AlertManager config | Escalation email configured in `/etc/prometheus/alertmanager.yml` |
| `ALERT_PROVIDER` | ✅ Done | SendGrid active for current pilot scope |
| `ALERT_SENDER` | ✅ Done | Verified sender identity sufficient for confirmed primary+secondary delivery |
| Acknowledgment signature | ✅ Done | Operator acknowledged matrix on 2026-05-17; see acknowledgment artifact |

### Non-Claims

- **NOT production-ready**: This skeleton does not constitute live alerting configuration.
- **NOT real contacts stored**: All contact fields are placeholders; no PII in version control.
- **NOT SMS/webhook escalation**: SMS/webhook channels remain deferred outside the current pilot scope.

---

## Cross-References

| Document | Purpose |
|----------|---------|
| [`67-production-readiness-roadmap.md`](./67-production-readiness-roadmap.md) | Authoritative blocker status and evidence gates |
| [`artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md`](./artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md) | C1 execution evidence and Block B SendGrid smoke-test state |
| [`artifacts/2026-05-17-sendgrid-rotation-evidence.md`](./artifacts/2026-05-17-sendgrid-rotation-evidence.md) | SendGrid API key rotation, permission root-cause/fix, delivery verification, old-key revocation, and SSH firewall restoration |
| [`artifacts/2026-05-17-escalation-matrix-acknowledgment.md`](./artifacts/2026-05-17-escalation-matrix-acknowledgment.md) | Formal escalation matrix acknowledgment; closes Block B |
| [`artifacts/2026-05-15-r1-alerting-rotation-policy.md`](./artifacts/2026-05-15-r1-alerting-rotation-policy.md) | Block B rotation policy and escalation matrix template |
| [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) | Exact command sequences for Blocks A/B/C |
| [`70-security-hardening-local-only-plan.md`](./70-security-hardening-local-only-plan.md) | Token rotation procedure and local audit commands |
| [`01-current-state.md`](./01-current-state.md) | Current engineering and operator status |
| [`AGENTS.md`](../../AGENTS.md) | Repo toolchain, invariants, and verification status |
| [`docs/ROADMAP.md`](../ROADMAP.md) | Post-pilot phased completion roadmap (production-candidate path, deferring real domain) |
| [`docs/production-readiness-v2/`](../production-readiness-v2/) | P1 planning docs (SLO/SLA, PG hardening, MCP target, security/tenant ADR, evidence checklist) |
| [`docs/guides/`](../guides/) | P2 product/user guide scaffolds (quickstart, concepts, MCP, policy, operator, deployment) |
| [`artifacts/2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md`](./artifacts/2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md) | Path A conditional pilot closure acknowledgment (2026-05-18) |
| [`artifacts/2026-05-18-local-confidence-polish-evidence.md`](./artifacts/2026-05-18-local-confidence-polish-evidence.md) | D1–D6 API live local, G3.6 bounded local execute, MCP lifecycle smoke, WAL drill, pre-target gate (2026-05-18) |

---

## Non-Claims

- **NOT production-ready**: This tracker does not make FerrumGate production-ready.
- **NOT full production posture**: Block A (real domain) is WAIVED/CONDITIONAL for single-node SQLite pilot only. Block B is closed (G-B1/G-B2/G-B3/G-B4 done). Real-domain evidence is still required for production-ready or full G2 closure.
- **NOT PostgreSQL production**: Remains deferred; single-node SQLite only.
- **NOT HA/multi-node**: Out of v1 scope.
- **NOT full P0/G2 production claim**: Primary and secondary email delivery confirmed; Block B is closed, but Block A domain evidence is WAIVED/CONDITIONAL (not CLOSED).
- **NOT production-ready despite Block B closure**: Block B is now CLOSED, but Block A is WAIVED/CONDITIONAL (not CLOSED) and production-ready remains NO.
- **NOT full security audit**: `make audit` passes with both cargo-deny and cargo-audit. This is a local/manual gate, not CI.

---

*Tracker created: 2026-05-16. Completion roadmap and hardening tracker — planning artifact only. No execution claimed.*
