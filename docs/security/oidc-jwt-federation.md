# FerrumGate OIDC/JWT Federation Design

> **Status:** Design-only — Phase 4.1 deliverable.  
> **Phase:** 4.1 — Identity federation design.  
> **Parent Plan:** [`../PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md)  
> **Related boundary doc:** [`non-claims.md`](./non-claims.md)

This document defines the design for OIDC/JWT federation in FerrumGate. It covers the authentication mode decision, JWT validation flow, claim-to-role mapping, TOML configuration schema, security considerations, and the implementation plan for Phase 4.2+. It is **not** a production-readiness claim.

---

## 1. Purpose and Boundary

FerrumGate is an execution-governance gateway, not an identity provider (IdP). Phase 4.1 adds the ability to accept externally-issued OIDC JWTs as bearer tokens and map their claims into FerrumGate's existing `actor_id + role + scope` authorization model. This enables enterprise operators to reuse their existing IdP (Google Workspace, Azure AD/Entra, Keycloak, etc.) without building a login/callback/session UI inside FerrumGate.

OIDC/JWT federation is **not**:

- A full SSO login flow with session cookies, callback endpoints, or consent UI.
- An identity provider; FerrumGate does not mint or issue OIDC tokens.
- Multi-tenant SaaS identity isolation.
- SOC 2 / compliance certification.
- A production-ready claim.

---

## 2. Design Decision: `AuthMode::Oidc`

### 2.1 New mode, not composition with `Scoped`

The design adds a new **`AuthMode::Oidc`** variant to the gateway authentication mode enum. This is a **JWT-bearer validation mode** that reuses the existing scope-enforcement semantics already defined for `AuthMode::Scoped`, but replaces the opaque-token lookup/hash/revocation path with a stateless JWT signature and claim-validation path.

Rationale:

- **Composing with `Scoped`** would conflate two fundamentally different token models: opaque database-backed tokens (with revocation, salted hashes, and `last_used_at`) versus stateless JWTs (with signature-based trust and IdP-managed lifecycle). Mixing them in one mode creates confusion in middleware, config, and operational telemetry.
- **A distinct mode** keeps the middleware branch clean: `Disabled` → `Bearer` → `Scoped` → `Oidc`, each with a single, explicit token source and validation path.
- **Scope enforcement is reused**: after JWT validation and claim mapping succeed, the middleware proceeds to the same `required_scope_for_path()` lookup and exact/wildcard scope matching used by `AuthMode::Scoped`. No new scope semantics are introduced.

### 2.2 Duplicate `AuthMode` enum risk

There are currently **two `AuthMode` enums** in the workspace:

- `crates/ferrum-proto/src/token.rs` — shared proto/model enum.
- `crates/ferrum-gateway/src/state.rs` — gateway-specific runtime enum.

**Phase 4.2 must synchronize both enums** by adding `Oidc` to each and, ideally, converging to a single source of truth (e.g., re-exporting from `ferrum-proto`) to eliminate future drift. This is tracked in the implementation plan (Section 8).

---

## 3. TOML Configuration Schema

The `auth` table in `ferrumd.toml` gains an optional `oidc` sub-table. When `auth_mode = "oidc"`, the `oidc` table is required.

```toml
[auth]
mode = "oidc"

[auth.oidc]
# Required: OIDC issuer URL. Must use HTTPS.
issuer = "https://accounts.google.com"

# Required: client ID issued by the IdP for this FerrumGate deployment.
# Used as the primary audience validator.
client_id = "ferrumgate.example.com"

# Optional: additional allowed audiences. If absent, only client_id is accepted.
allowed_audiences = [ "https://ferrumgate.internal/api" ]

# Required: JWKS endpoint. Usually derived from issuer discovery,
# but explicit config is preferred to avoid runtime discovery failure.
jwks_url = "https://accounts.google.com/.well-known/jwks.json"

# Optional: JWKS cache TTL in seconds. Default: 3600 (1 hour).
jwks_cache_ttl_secs = 3600

# Optional: clock skew tolerance in seconds. Default: 30.
clock_skew_secs = 30

# Optional: require email_verified claim to be true. Default: true.
require_email_verified = true

# Claim mapping rules.
[auth.oidc.claims]
# Which JWT claim provides the actor_id. Default: "sub".
actor_id = "sub"

# Which JWT claim provides the email (used for logging / audit only).
email = "email"

# Which JWT claim provides group/role membership. Default: "groups".
# Supports array of strings (e.g. "groups": ["fg-admins", "fg-operators"]).
role_source = "groups"

# Role mapping: IdP group/role name → FerrumGate TokenRole.
# Unmapped roles result in deny-by-default (see Section 5).
[auth.oidc.claims.role_mapping]
"fg-admins"      = "Admin"
"fg-operators"   = "Operator"
"fg-policy"      = "PolicyAuthor"
"fg-auditors"    = "Auditor"
"fg-agents"      = "Agent"
"fg-readonly"    = "ReadOnly"
```

### 3.1 Validation rules

- `issuer` must be a valid HTTPS URL.
- `client_id` must be non-empty.
- `jwks_url` must be a valid HTTPS URL.
- `role_mapping` must contain at least one entry (empty mapping would deny every request).
- If `auth.mode != "oidc"`, the `[auth.oidc]` table must be ignored (not validated) to avoid boot failures on non-OIDC deployments.

---

## 4. JWT Validation Flow

When `AuthMode::Oidc` is enabled, the request authorization flow is:

1. **Public whitelist check** — health/readiness/metrics endpoints bypass auth (same as `Scoped`).
2. **Bearer token extraction** — `Authorization: Bearer <jwt>` header.
3. **JWT structure validation** — three base64url segments, valid JSON header and payload.
4. **Algorithm restriction** — reject `none` and any asymmetric algorithm not in an explicit allowlist (`RS256`, `RS384`, `RS512`, `ES256`, `ES384`, `ES512`, `EdDSA`).
5. **JWKS key resolution** — fetch/cache JWKS from `jwks_url`; locate the key matching `kid` in the JWT header. If the key is not in cache and refresh fails, return `401` (do not fall back to stale or missing keys).
6. **Signature verification** — verify the JWT signature against the resolved public key.
7. **Issuer validation** — `iss` must exactly match `auth.oidc.issuer`.
8. **Audience validation** — `aud` must match `client_id` or one of `allowed_audiences`.
9. **Time validation** —
   - `exp` must be in the future (with `clock_skew_secs` tolerance).
   - `nbf` (if present) must be in the past (with `clock_skew_secs` tolerance).
   - `iat` (if present) must not be unreasonably far in the future (reject if `iat > now + clock_skew_secs`).
10. **Email verification check** — if `require_email_verified = true`, `email_verified` claim must be `true`.
11. **Claim mapping** — extract `actor_id`, `email`, and `role_source` values per config.
12. **Role mapping** — map IdP role/group names to FerrumGate `TokenRole` via `role_mapping`. If no mapped role is found, deny.
13. **Scope derivation** — resolve scopes from the mapped `TokenRole` using the existing `TokenRole::default_scopes()` logic (same as `Scoped`).
14. **Required-scope lookup** — look up required scope for `(method, path)` via `required_scope_for_path()`.
15. **Scope match** — exact or wildcard (`*`) match.
16. **Audit log entry** — record authentication event with `actor_id`, mapped role, JWT `jti` (if present), and success/deny outcome.

### 4.1 Error responses

| Failure | HTTP Status | Log level | Notes |
|---------|-------------|-----------|-------|
| Missing `Authorization` header | `401 Unauthorized` | `warn` | Same as `Scoped`. |
| Malformed JWT | `401 Unauthorized` | `warn` | Structure or Base64 decode failure. |
| Unsupported algorithm | `401 Unauthorized` | `warn` | Explicitly reject `none`. |
| Key not found in JWKS | `401 Unauthorized` | `error` | Do not retry per-request; rely on cache refresh. |
| Signature verification failed | `401 Unauthorized` | `warn` | |
| Issuer mismatch | `401 Unauthorized` | `warn` | |
| Audience mismatch | `401 Unauthorized` | `warn` | |
| Expired JWT | `401 Unauthorized` | `warn` | Include `exp` vs `now` in structured log. |
| `nbf` not yet valid | `401 Unauthorized` | `warn` | |
| `email_verified` false / missing | `401 Unauthorized` | `warn` | Only when `require_email_verified = true`. |
| Unmapped role/group | `403 Forbidden` | `warn` | JWT is valid but claim mapping denies access. |
| Missing required scope | `403 Forbidden` | `warn` | Same as `Scoped`. |

---

## 5. Claim Mapping and Deny-by-Default

### 5.1 Actor identity

- `actor_id` is sourced from the configured claim (default `sub`).
- If the claim is missing or empty, deny (`401`).
- `email` is sourced for audit/logging only; it is **not** used as the primary actor identifier.

### 5.2 Role mapping

- The `role_source` claim (default `groups`) is read as an array of strings. If it is a single string, treat it as a one-element array for convenience.
- For each element in the array, look it up in `role_mapping`.
- **First match wins** (order is IdP claim array order, not config order).
- If no element matches any `role_mapping` key, deny (`403`).
- If the claim is missing or empty, deny (`403`).

### 5.3 Scope derivation

- After a `TokenRole` is resolved, scopes are derived from `TokenRole::default_scopes()` — the same function used by `AuthMode::Scoped`.
- No custom per-user scope overrides in Phase 4. This keeps the model simple and avoids overgrant. Future phases may add explicit scope filtering if needed.

### 5.4 Deny-by-default summary

| Condition | Outcome |
|-----------|---------|
| JWT signature invalid | Deny (`401`) |
| Issuer / audience mismatch | Deny (`401`) |
| Expired / not-yet-valid | Deny (`401`) |
| `email_verified` required but false | Deny (`401`) |
| `actor_id` claim missing | Deny (`401`) |
| No mapped role | Deny (`403`) |
| Required scope not in derived scopes | Deny (`403`) |

---

## 6. Security Considerations

### 6.1 JWKS availability and cache

- FerrumGate caches the JWKS response with a configurable TTL. If the cache expires and the IdP is unreachable, new requests cannot be authenticated until JWKS is refreshed.
- **Mitigation**: set a reasonable TTL (e.g., 1 hour) and expose a metric/gauge for JWKS cache age and refresh failures. Operators should monitor this.
- **Do not** fall back to a hardcoded key or skip signature verification if JWKS is unavailable.

### 6.2 Claim-to-scope overgrant

- The highest risk in JWT federation is mapping a broad IdP group to a powerful FerrumGate role (e.g., mapping `Domain Users` to `Admin`).
- **Mitigation**: `role_mapping` is explicit and deny-by-default. Operators must intentionally configure every mapping. Docs and logs should warn when wildcard or broad IdP groups are mapped.
- Future hardening: add a `scope_filter` allowlist that further restricts derived scopes even after role mapping.

### 6.3 Algorithm confusion

- Reject `alg: none` unconditionally.
- Maintain an explicit allowlist of permitted signature algorithms.
- If the JWKS contains a key with an algorithm outside the allowlist, do not use it for verification.

### 6.4 Clock skew

- Use `clock_skew_secs` (default 30s) to tolerate NTP drift between FerrumGate hosts and the IdP.
- Log a warning if local clock skew exceeds 5 seconds relative to `iat` over multiple requests.

### 6.5 Token revocation

- Stateless JWTs cannot be revoked by FerrumGate. Reliance is on short TTLs (e.g., 5–60 minutes) and IdP-level revocation.
- **Mitigation**: document that FerrumGate does not maintain a revocation list for OIDC tokens; operators must rely on IdP session/policy management and short token lifetimes.
- Future: optional `jti` blocklist in store (out of scope for Phase 4).

---

## 7. Non-Claims

The following boundaries from [`non-claims.md`](./non-claims.md) apply unchanged:

- `production-ready` = **NO**
- `Tier 2` = **NOT COMPLETE**
- Full G2 = **NOT COMPLETE**
- Real domain / public endpoint = **missing**
- `HA-4` unattended automated failover = **NOT COMPLETE**
- Sustained SLO window (7–30 days) = **NOT COMPLETE**

Additionally:

- FerrumGate is **not** an IdP. It validates externally-issued JWTs; it does not issue them.
- There is **no** login page, callback endpoint (`/oauth/callback`), session cookie, consent screen, or logout flow in Phase 4.
- OIDC/JWT federation in Phase 4 is **design-only** (4.1) followed by **minimal implementation** (4.2–4.4). It is not a complete SSO integration.
- Multi-tenant identity isolation is **not** supported.

---

## 8. JWT Dependency Strategy

### 8.1 Core dependency: `jsonwebtoken`

The recommended core JWT dependency for FerrumGate is **`jsonwebtoken`** (currently v9 in the ecosystem; v10 requires explicit crypto backend feature selection and should be evaluated when v10 stabilizes).

- **Why `jsonwebtoken`**: It is the most widely used, well-maintained Rust JWT crate. It supports the required algorithm allowlist (`RS256`, `RS384`, `RS512`, `ES256`, `ES384`, `ES512`, `EdDSA`) and provides `decode`/`validate` helpers.
- **Why NOT `openidconnect`**: The `openidconnect` crate is heavier and designed for full RP (Relying Party) flows with discovery, authorization endpoints, and token exchange. FerrumGate only needs stateless JWT validation, not a full RP implementation.
- **Network dependency (`reqwest`)**: JWKS fetching requires an HTTP client. `reqwest` is already a workspace dependency. It should **not** be added to `ferrum-gateway` until Phase 4.4 (JWKS fetch/cache) to keep the compile boundary minimal. Phase 4.2–4.3 can validate locally-minted test JWTs without network I/O.
- **Feature selection**: When `jsonwebtoken` is added, use only the features required for the algorithm allowlist (e.g., `rsa`, `ecdsa`, `eddsa`). Avoid enabling `openssl` if `ring` is sufficient.

### 8.2 Dependency timeline

| Phase | Dependency action |
|-------|-------------------|
| 4.2 | Document strategy only; do **not** add `jsonwebtoken` or `reqwest` to `ferrum-gateway` yet. |
| 4.3 | Add `jsonwebtoken` to `ferrum-gateway`/`workspace.dependencies` with minimal features. Implement offline JWT validation (test JWKS loaded from file/static JSON). |
| 4.4 | Add `reqwest` to `ferrum-gateway` with `rustls-tls`. Implement live JWKS fetch + TTL cache, RSA JWK support, config loading from TOML/env, and fail-closed fallback. |

---

## 9. Phase 4.2+ Implementation and Test Plan

### 9.1 Implementation order

| Step | Task | Owner | Type | Status |
|------|------|-------|------|--------|
| 4.2.1 | Add `Oidc` variant to canonical `AuthMode` (`ferrum-proto`) and re-export through `ferrum-gateway`. Remove duplicate. | Dev | Build | **DONE** |
| 4.2.2 | Add JWT validation dependencies (`jsonwebtoken`) to `ferrum-gateway` with minimal feature set. | Dev | Build | **DONE** |
| 4.2.3 | Implement `OidcConfig` struct, validation rules, and static key loading. | Dev | Build | **DONE** |
| 4.2.4 | Implement JWKS fetch + cache (in-memory TTL cache; no external cache service). | Dev | Build | **DONE** |
| 4.2.5 | Implement JWT validation middleware branch for `AuthMode::Oidc` following Section 4 flow. | Dev | Build | **DONE** |
| 4.2.6 | Implement claim mapping and role-to-scope derivation reusing `TokenRole::default_scopes()`. | Dev | Build | **DONE** |
| 4.2.7 | Add OIDC config loading from TOML/env into `ServerConfig.oidc_config`. | Dev | Build | **DONE** |
| 4.3.1 | Implement role mapping middleware and deny-by-default for unmapped roles. | Dev | Build | **DONE** |
| 4.4.1 | Write `docs/security/oidc-jwt-federation.md` (this document). | Dev | Document | **DONE (design)** |

> **Phase 4.2 boundary**: Enum sync, compile-safe OIDC placeholder (middleware attaches but fails closed with 401), dependency strategy documented. No JWT verification, no JWKS fetch, no OIDC config structs in Rust.
>
> **Phase 4.3 boundary**: `jsonwebtoken` v9 added; `OidcConfig`, `KeyMaterial`, and offline JWT validation middleware implemented; claim-to-role-to-scope mapping is deny-by-default; hermetic tests cover valid JWT, expired JWT, wrong issuer, wrong audience, unmapped role, missing scope, invalid signature, missing actor_id, and public endpoint bypass. No live JWKS fetch, no network I/O, no `reqwest` in `ferrum-gateway`.
>
> **Phase 4.4 boundary**: `reqwest` added to `ferrum-gateway` with `rustls-tls`; live JWKS fetch with lazy cache (`OidcJwksCache`); RSA JWK support via `KeyMaterial::RsaJwk`; static keys take precedence over fetched JWKS; config loading from TOML (`[oidc]` section) and env vars (`FERRUMD_OIDC_*`); validation allows empty `static_keys` when `jwks_url` is present; fail-closed on fetch errors (returns 401); tests cover JWKS fetch, unavailable JWKS, and JWK parsing. No OIDC discovery, no login/callback/session/PKCE, no background refresh task. |

### 8.2 Required tests

| Test | Scenario | Expected |
|------|----------|----------|
| Valid JWT | Signature, issuer, audience, expiry all valid; mapped role has required scope. | `200 OK`; actor_id and scopes correctly resolved. |
| Expired JWT | `exp` in the past (beyond clock skew). | `401 Unauthorized`; structured log cites `exp`. |
| Wrong issuer | `iss` does not match configured `issuer`. | `401 Unauthorized`. |
| Wrong audience | `aud` does not match `client_id` or `allowed_audiences`. | `401 Unauthorized`. |
| Unmapped role | JWT is valid but `groups` contains no key in `role_mapping`. | `403 Forbidden`; log cites unmapped role source. |
| Missing actor_id | `sub` claim missing or empty. | `401 Unauthorized`. |
| Invalid signature | Signature tampered or wrong key. | `401 Unauthorized`. |
| Unsupported algorithm | `alg: none` or `HS256` when only asymmetric is allowed. | `401 Unauthorized`. |
| JWKS cache miss | Key ID not in cache and refresh fails. | `401 Unauthorized`; metric incremented. |
| Missing required scope | Valid JWT + mapped role, but role's default scopes do not include the route's required scope. | `403 Forbidden` (same as `Scoped`). |
| Email verification required | `require_email_verified = true` but claim is `false` or absent. | `401 Unauthorized`. |
| Clock skew tolerance | `exp` within `clock_skew_secs` of now. | `200 OK`. |

### 8.3 Integration test strategy

- Use a test JWKS generated with a local RSA/Ed25519 keypair.
- Use `jsonwebtoken` (or equivalent) to mint test JWTs in test code.
- Do **not** call external IdPs in CI tests. All tests must be hermetic.
- Provide a test helper that spins up a mock JWKS endpoint or loads a static JWKS JSON file.

---

## 10. Evidence Links

- [`non-claims.md`](./non-claims.md)
- [`scoped-tokens-rbac.md`](./scoped-tokens-rbac.md)
- [`../PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md)

---

*End of Phase 4.1/4.2 OIDC/JWT federation design document.*
