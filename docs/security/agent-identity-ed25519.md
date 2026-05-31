# Agent Identity with Ed25519 — Design

> **Scope:** This document specifies the cryptographic agent identity layer.

## 1. Goal

Provide a lightweight, self-contained agent identity mechanism for FerrumGate that:
- Does **not** require an external IdP for machine agents.
- Uses Ed25519 public-key signatures (compact, fast, well-supported).
- Integrates naturally with the existing capability and scoped-token model.
- Is simpler than a full DID or AGT-style trust mesh, but stronger than bearer tokens for programmatic agents.

## 2. Schema

### 2.1 Agent Registry Record

Stored in the gateway store (SQLite/PostgreSQL). Each record is immutable after creation; revocation is handled via `revoked_at`.

| Field | Type | Constraints |
|-------|------|-------------|
| `agent_id` | `TEXT` (UUIDv7 or `agent_` prefix) | Primary key, immutable |
| `public_key` | `TEXT` (Base64-encoded Ed25519 raw 32-byte) | Immutable, unique index |
| `key_fingerprint` | `TEXT` (Base64url-encoded SHA-256 of `public_key`) | Immutable, indexed for lookups |
| `allowed_scopes` | `JSON` array of scope strings | Subset of FerrumGate scopes; deny-by-default for unlisted scopes |
| `created_at` | `TIMESTAMP` | Auto-set on registration |
| `revoked_at` | `TIMESTAMP` or `NULL` | Set on revocation; revoked agents are rejected unconditionally |
| `description` | `TEXT` | Optional human-readable label |

### 2.2 Request Envelope

Every authenticated agent request carries the envelope in HTTP headers (or a structured body field for POSTs). Headers are preferred to avoid body parsing before auth.

```
X-Ferrum-Agent-Id: <agent_id>
X-Ferrum-Timestamp: <RFC 3339 UTC, e.g. 2026-05-28T12:34:56Z>
X-Ferrum-Nonce: <128-bit hex, e.g. 64 chars>
X-Ferrum-Body-Hash: <BLAKE3 hex of raw request body, or "null" for empty body>
X-Ferrum-Signature: <Base64-encoded Ed25519 signature>
```

Signature payload (canonical, UTF-8, no extra whitespace):
```
<agent_id>:<timestamp>:<nonce>:<body_hash>:<http_method>:<http_path>
```

Example:
```
agent_abc123:2026-05-28T12:34:56Z:a1b2c3d4...:null:POST:/v1/intents/compile
```

## 3. Verification Flow

1. **Extract headers** — missing any required header → `401`.
2. **Look up agent** by `agent_id` → not found or `revoked_at` set → `401`.
3. **Verify signature** using stored `public_key` against canonical payload → invalid → `401`.
4. **Verify timestamp** — `|now - timestamp|` ≤ `agent_clock_skew_secs` (default 30s, configurable) → stale → `401`.
5. **Verify nonce** — query a bounded in-memory or store-backed nonce cache (e.g., 5-minute TTL, keyed by `nonce`). Replayed nonce → `401`.
6. **Verify body hash** — recompute `BLAKE3(raw_body)` and compare to header → mismatch → `401`.
7. **Scope enforcement** — derive required scope from `method:path`, check against `allowed_scopes` → missing → `403`.
8. **Proceed** — attach `agent_id` and derived scopes to request extensions for downstream handlers.

> **Fail-closed:** Any step failure short-circuits to `401` (authentication) or `403` (authorization). No partial trust.

## 4. Nonce / Timestamp Replay Protection

- **Nonce store:** In-memory `dashmap` or `moka` cache with TTL = `agent_clock_skew_secs * 2` (minimum 60s). For multi-node deployments, a shared Redis/cache is recommended but not required for single-node pilot.
- **Timestamp bound:** Reject requests with timestamps older than `now - skew` or newer than `now + skew`.
- **Combined effect:** Even if an attacker captures a valid request, replay is blocked by nonce uniqueness and timestamp window.

## 5. Key Registration, Rotation, and Revocation

### 5.1 Registration

`POST /v1/admin/agents` (requires `admin:agents` scope):
- Accept `public_key`, `allowed_scopes`, optional `description`.
- Compute `key_fingerprint`.
- Reject if fingerprint already exists (prevents duplicate keys).
- Return `agent_id` and `key_fingerprint`.

CLI equivalent: `ferrumctl admin agents register --agent-id <id> --public-key <b64> --scope execution:execute --scope provenance:read`

### 5.2 Rotation

Rotation is **register-new + revoke-old** (no in-place key mutation). This preserves audit lineage.
- Register new key with same or updated scopes.
- Revoke old `agent_id`.
- Audit both `AgentRegister` and `AgentRevoke` events.

### 5.3 Revocation

`DELETE /v1/admin/agents/{agent_id}` (requires `admin:agents` scope):
- Sets `revoked_at = now()`.
- Entry is retained for audit/provenance; queries filter `revoked_at IS NULL`.
- Revocation is effective immediately (nonce cache does not need purging because the agent lookup will reject).

CLI equivalent: `ferrumctl admin agents revoke <agent_id>`

## 6. Relation to OIDC and Scoped Tokens

| Mechanism | Identity Type | Use Case | Where It Fits |
|-----------|--------------|----------|---------------|
| **OIDC/JWT** | Human / interactive | Operator login, SSO | Separate integration |
| **Scoped Bearer Tokens** | Service / short-lived | Integrations, CI jobs | Current; opaque, DB-backed |
| **Ed25519 Agent Identity** | Machine / long-lived | MCP agents, autonomous workers | Cryptographic, no DB secret needed at client |

Ed25519 agent identity does **not** replace OIDC or scoped tokens. It coexists:
- Human operators → OIDC.
- Short-lived service integrations → scoped tokens.
- Long-lived agents with cryptographic identity → Ed25519.

The same `required_scope_for_path()` logic applies regardless of auth mode. The middleware normalizes all three mechanisms to a common `(actor_id, scopes)` tuple before hitting governance endpoints.

## 7. Boundaries

- **No DID / trust mesh:** We intentionally avoid W3C DID, VC, or trust scoring.
- **No multi-tenant identity:** `tenant_id` is reserved but not enforced.
- **No mTLS replacement:** mTLS service-to-service remains a separate item.
- **Bounded nonce cache:** in-memory cache for single-process replay protection; shared deployments should provide a shared cache layer.
- **No key escrow:** FerrumGate never holds private keys. `public_key` only.

## 8. Implementation Plan

1. Add `agent_registry` table/schema, `AgentRegistry` trait in `ferrum-store`, in-memory nonce cache, and signature verification middleware.
2. Wire `AuthMode::Agent` into `auth_middleware`.
3. Implement `ferrumctl admin agents register/list/revoke`, gateway admin endpoints `POST/GET/DELETE /v1/admin/agents`, `admin:agents` scope mapping, and audit entries for register/revoke.
4. Add integration tests: signature validation, replay rejection, scope enforcement, revocation immediacy, audit entry emission.
5. Shared nonce cache for multi-node, agent metrics (`ferrumgate_agent_auth_total`), rate-limit per agent_id.

## 9. Audit Events

New audit actions to add for agent identity:
- `AgentRegister`
- `AgentRevoke`
- `AgentAuthSuccess` (optional, off by default to avoid firehose)
- `AgentAuthFailed` (on by default, sanitized, no signature payload)

These will be added to `AuditAction` during implementation.

## 10. References

- OIDC/JWT: `docs/security/oidc-jwt-federation.md`
- Scoped tokens: `docs/security/scoped-tokens-rbac.md`
- Threat model: `docs/security/threat-model-stride.md`
