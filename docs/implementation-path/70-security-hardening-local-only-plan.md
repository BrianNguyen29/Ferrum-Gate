# 70 — Security Hardening: Local-Only Plan

> **Status**: Documentation-only. Security hardening proposal matrix and operator todo-list.
> **Scope**: Single-node SQLite v1 only. No CI additions. No production-ready claim.
> **Constraint**: Do not add GitHub Actions/CI jobs; do not add cargo-deny/cargo-audit to CI; do not claim G2 or production-ready.

---

## Purpose

This document captures all proposed non-CI security and code-quality hardening additions as a
complete, prioritized todo-list. It groups related controls, makes explicit non-claims, and
provides operator-owned procedural items (token rotation, manual audit commands).

**This is NOT a CI security scanning plan.** CI dependency scanning is explicitly deferred due to
CI cost constraints. Local/manual alternatives are provided.

---

## Explicit Non-Claims

- **No CI security scan**: cargo-deny, cargo-audit, or similar are NOT added to CI
- **No scheduled scan**: No nightly/weekly security scans in CI
- **No production-ready claim**: This plan does not make FerrumGate "production-ready"
- **No G2 complete**: G2 gates remain pending operator signoff per doc 59/doc 54
- **No permissive CORS**: CORS is disabled by default; browser client support requires explicit opt-in
- **No PostgreSQL/HA**: PostgreSQL/multi-node is Path 3, not in scope for Phase 1
- **No RBAC/JWT**: Role-based access control and JWT are not required for v1 scope

---

## Security Posture Groups

### Group A — Current Controls (Implemented)

| Control | Status | Notes |
|---------|--------|-------|
| Bearer token authentication | ✅ Implemented | Constant-time comparison; `auth_mode = "Bearer"` |
| Rate limiting | ✅ Implemented | 2 req/s sustained, burst 50; per-IP via `tower_governor` |
| Health/readiness unauthenticated | ✅ Implemented | `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics` always accessible |
| Capability TTL enforcement | ✅ Implemented | Hardcoded 300s max in `ferrum-cap`; expired capabilities return `CapabilityExpired` |
| Single-use capability enforcement | ✅ Implemented | Capabilities are single-use by design |
| SQLite write queue | ✅ Implemented | Serializes writes; eliminates lock thrash |
| FK constraint chain | ✅ Implemented | Cascading FKs: intents → proposals → capabilities → executions → rollback_contracts |

---

### Group B — Operator-Owned Controls (Require Operator Action)

| Control | Owner | Procedure | Evidence |
|---------|-------|-----------|----------|
| Bearer token generation | Operator | `openssl rand -hex 32`; store in env file | Token file path in doc 65 |
| Bearer token rotation | Operator | See §Token Rotation Procedure below | Evidence of rotation |
| TLS termination | Operator | Configure reverse proxy (nginx/etc.) | TLS config in doc 65 |
| Backup automation | Operator | External scheduler (cron/systemd timer) | Doc 65 §Backup |
| Restore drill | Operator | Execute per doc 62 §Phase 5 | `PRAGMA integrity_check` pass |
| CORS opt-in | Operator | Only if browser client explicitly required | CORS config documentation |

---

### Group C — Deferred Controls (Post-v1 / Phase 3)

| Control | Rationale | Priority |
|---------|-----------|----------|
| Durable capability persistence | Post-v1: capability revocation survives process restart | S4 |
| PostgreSQL store | Path 3: higher write throughput + HA | S4 |
| DLP (Data Loss Prevention) stub | Post-v1 scope; content inspection not required | S4 |
| HTTP replay breadth | fs: local slices verified; git/http/sqlight/maildraft: partial | S4 |

---

### Group D — Local/Manual Audit Commands (Not CI)

| Check | Command | Purpose |
|-------|---------|---------|
| Dependency audit (local) | `cargo deny check advisories` | Scan for known vulnerabilities in dependencies; run manually |
| License audit (local) | `cargo deny check licenses` | Enforce allowlist/denylist of licenses; run manually |
| Securing the toolchain | `cargo install --locked cargo-deny` then `cargo deny check` | Ensure reproducible toolchain |
| Advisory check (local) | `cargo audit` | Scan for crates with security advisories; `cargo-audit v0.22.1` installed and `make audit` passing (1090 advisories, 384 dependencies scanned, 0 actionable issues) |
| Format/lint check | `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings` | Pre-commit hook candidate |

**Note**: These commands are for local operator use only. They are NOT added to CI.

---

### Group E — Proxy-Owned Controls (Not Application-Level)

| Control | Owner | Rationale |
|---------|-------|-----------|
| Request body size limit | Proxy/reverse proxy | ferrumd does not implement application-level body size limits; deploy behind TLS-terminating proxy that enforces body limits |
| CORS preflight | Proxy or explicit opt-in | No CORS by default; browser clients require explicit opt-in via proxy or application-level CORS configuration |
| TLS client certs | Proxy/infrastructure | TLS termination at proxy; ferrumd does not terminate TLS |

---

## CORS Posture

**Default: No CORS (disabled)**

FerrumGate v1 does not implement CORS. Browser clients are not a supported client type for v1.

- If a browser client is **explicitly required** and approved by security policy, CORS must be
  explicitly configured at the proxy layer or application layer with restrictive origin allowlist.
- CORS should never be permissive (`Access-Control-Allow-Origin: *`) in production.
- This document does not provide CORS implementation; if needed, operator must configure or
  engage engineering.

**Declaration**: CORS is not a current v1 requirement. It is documented here for completeness.

---

## Request Body Size Limit Posture

**Proxy-owned**: Application-level request body size limits are not implemented in ferrumd.

- Operators deploying behind a reverse proxy (nginx, traefik, etc.) should configure body size
  limits at the proxy layer.
- Recommended proxy-level configuration: `client_max_body_size 1m` (nginx) or equivalent.
- Application-level body limits may be considered in a future iteration if workload demonstrates need.

---

## Capability Durability

### Current: Single-Use During Process Lifetime

Capabilities in v1 are:
- **Single-use**: Each capability can only be consumed once
- **In-memory revocation tracking**: Revoked capabilities are tracked in-memory; process restart
  clears the revocation list
- **TTL enforcement**: Maximum 300 seconds; expired capabilities return `CapabilityExpired`

### Deferred: Durable Capability Persistence (Post-v1)

After v1 hardening and Phase 3, capability persistence may be extended:
- Revocation lists persisted to SQLite store
- Survive process restarts
- Enable "revoke all" operations across process lifetime

**This is a Phase 3 / post-v1 item. It is not in scope for current hardening.**

---

## Bearer Token Rotation Procedure

> **Owner**: Operator
> **When**: When token is suspected compromised, per policy, or as periodic hygiene

### Token Rotation Steps

1. **Generate new token** (on secure admin host):
   ```bash
   openssl rand -hex 32
   ```

2. **Update env file** (e.g., `/etc/ferrumgate/ferrumd.env`):
   ```
   FERRUMD_BEARER_TOKEN=<new-token>
   ```

3. **Restart ferrumd** to pick up new token:
   ```bash
   systemctl restart ferrumd   # or your init system
   ```

4. **Verify auth works** with new token:
   ```bash
   curl -H "Authorization: Bearer <new-token>" http://<host>:8080/v1/approvals
   # expect 200
   ```

5. **Verify old token is rejected**:
   ```bash
   curl -H "Authorization: Bearer <old-token>" http://<host>:8080/v1/approvals
   # expect 401
   ```

6. **Update all clients** to use new token

7. **Securely destroy** old token value (do not log it)

### Token Rotation Evidence

| Step | Evidence |
|------|----------|
| New token generated | Admin host shell history (redacted) |
| Env file updated | Path documented in operator records |
| New token works | curl output saved to evidence dir |
| Old token rejected | curl output saved to evidence dir |

---

## Prioritized Phases: S1–S4

| Phase | Items | Owner | Timeline |
|-------|-------|-------|----------|
| **S1** (Immediate) | Token rotation procedure documented; no-CORS-by-default documented; proxy-owned body limit documented; local audit commands documented | Engineering (docs) + Operator (token rotation) | Before first production pilot |
| **S2** (Pre-pilot) | Optional `deny.toml` local setup documented; evidence manifest updated with security audit placeholder; CORS opt-in guidance added | Engineering (docs) | Before G2 signoff |
| **S3** (Post-pilot) | Manual cargo-deny/cargo-audit workflow operational; `cargo-audit v0.22.1` installed and `make audit` passing; toolchain securing documented | Operator (manual) | Post-pilot, pre-Phase 3 |
| **S4** (Phase 3) | Durable capability persistence; PostgreSQL; DLP stub | Engineering | Phase 3 scope |

### S1 Recommended Immediate Actions

| # | Action | Owner | Notes |
|---|--------|-------|-------|
| S1.1 | Document token rotation procedure | Engineering | This doc §Token Rotation Procedure |
| S1.2 | Generate real bearer token | Operator | `openssl rand -hex 32`; do not hardcode |
| S1.3 | Verify no-CORS posture | Engineering | Confirmed; no CORS implementation |
| S1.4 | Document proxy-owned body limit | Engineering | Confirmed; proxy must enforce |
| S1.5 | Document local audit commands | Engineering | This doc §Group D |
| S1.6 | Store token securely | Operator | Env file; chmod 600; no logging |

---

## Do Not Do Now

The following are explicitly excluded from current scope:

| Item | Reason |
|------|--------|
| CI security scan (cargo-deny in CI) | CI cost; local manual alternative provided |
| CI scheduled scan | CI cost; operator manual alternative provided |
| cargo-audit in CI | CI cost; local manual alternative provided |
| Permissive CORS | Not required for v1; security risk |
| CORS implementation | Browser clients not supported in v1 |
| PostgreSQL/HA | Path 3 only; blocked until G2 complete |
| RBAC/JWT | Not required for v1 scope |
| Application-level body size limit | Proxy should own this; not ferrumd |
| Durable capability persistence | Post-v1; Phase 3 scope |
| DLP implementation | Post-v1 scope |
| HTTP replay beyond local slices | Part of remaining surface per doc 67 P2.1 |

---

## Evidence Manifest Additions

The evidence bundle manifest (`path2-evidence-bundle-template/MANIFEST.md`) should be updated to
include optional local security audit evidence and token rotation evidence:

| File | Description | Status |
|------|-------------|--------|
| `08-security/token_rotation_procedure.txt` | Token rotation commands output | ☐ Optional |
| `08-security/local_audit_advisories.txt` | `cargo deny check advisories` output | ☐ Optional |
| `08-security/local_audit_licenses.txt` | `cargo deny check licenses` output | ☐ Optional |
| `08-security/local_audit_summary.txt` | Summary of local audit results | 🟡 Available — `make audit` generates this; not yet collected |

**Note**: These are optional operator-collected evidence. They do NOT contribute to G2 gates
and are NOT required for pilot signoff.

---

## Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`67-production-readiness-roadmap.md`](./67-production-readiness-roadmap.md) | Production readiness context; CI deferral noted |
| This doc | [`68-path-2-operator-handoff-packet.md`](./68-path-2-operator-handoff-packet.md) | Operator quick-reference |
| This doc | [`62-path-2-operator-runbook.md`](./62-path-2-operator-runbook.md) | Operator command sequences |
| This doc | [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md) | Operator input template |
| This doc | [`path2-evidence-bundle-template/MANIFEST.md`](./path2-evidence-bundle-template/MANIFEST.md) | Evidence tracker |

---

## S1 Quick-Reference Checklist

- [ ] Token rotation procedure reviewed (this doc §Token Rotation Procedure)
- [ ] Real bearer token generated (`openssl rand -hex 32`)
- [ ] Token stored securely (env file, chmod 600)
- [ ] No-CORS-by-default posture understood
- [ ] Proxy-owned body limit confirmed (proxy config)
- [ ] Local audit commands reviewed (this doc §Group D)
- [ ] Evidence manifest updated with optional security audit placeholders

---

*Document created: 2026-05-06. Security hardening plan — local-only, no CI additions, no production-ready claim.*
