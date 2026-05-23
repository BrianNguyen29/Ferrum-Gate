# 13 — Token API Contract

> **Status**: Implemented — token admin endpoints (POST/GET/DELETE/rotate) implemented 2026-05-21. Conditional on operator review boundaries. See [`10-evidence-checklist.md`](./10-evidence-checklist.md) §Phase 4.
> **Owner**: Engineering
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **Depends on**: [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md), [`12-endpoint-to-scope-mapping.md`](12-endpoint-to-scope-mapping.md)

---

## Goal

Define the request/response contracts for the scoped-token lifecycle admin APIs so that:
1. Engineering can implement them once BLK-SEC-PH4 is unblocked.
2. ferrumctl can be wired to them for UX-4.
3. The operator can review and approve the contract before implementation.

## Base assumptions

- Auth mode transitions from `Bearer` (global token) to `Scoped` (per-token scopes) via config flag.
- Until scoped-token enforcement is enabled, the global bearer token retains full access.
- Token values are generated server-side and returned **exactly once** at creation time.
- Token storage stores a **hash** of the token value, not the plaintext token.
- The `admin:tokens` scope is required to call any token admin API.

## Data model

```json
{
  "token_id": "tok_2vPqN5L8wX9mK3J",
  "actor_id": "operator-alice",
  "role": "operator",
  "scopes": ["approval:resolve", "provenance:read", "policy:read"],
  "description": "Alice's operator token for on-call shifts",
  "expires_at": "2026-08-20T00:00:00Z",
  "created_at": "2026-05-20T00:00:00Z",
  "last_used_at": "2026-05-20T12:34:56Z",
  "revoked_at": null,
  "revoked_reason": null,
  "rotated_from": null
}
```

- `token_id`: Server-generated opaque ID. Not a secret.
- `actor_id`: Free-form identifier chosen at creation time (e.g., username, service name).
- `role`: One of `admin`, `operator`, `policy_author`, `auditor`, `agent`, `read_only`.
- `scopes`: Explicit scope list. If omitted at creation, populated from role defaults.
- `description`: Human-readable note (max 256 chars).
- `expires_at`: Required. ISO 8601 UTC. Max TTL subject to operator Q5 decision.
- `created_at`: Server-set.
- `last_used_at`: Updated on each authenticated request using this token.
- `revoked_at`: Set on revocation. Irreversible.
- `revoked_reason`: Optional free-form string (max 256 chars).
- `rotated_from`: If this token was created by rotation, the `token_id` of the predecessor.

## Endpoints

### `POST /v1/admin/tokens`

Create a new scoped token.

#### Request headers

| Header | Value |
|--------|-------|
| `Authorization` | `Bearer <global-bearer-token>` (if scoped enforcement not yet enabled) or `Bearer <scoped-token-with-admin:tokens>` |
| `Content-Type` | `application/json` |

#### Request body

```json
{
  "actor_id": "operator-alice",
  "role": "operator",
  "scopes": ["approval:resolve", "provenance:read", "policy:read"],
  "description": "Alice's operator token for on-call shifts",
  "expires_at": "2026-08-20T00:00:00Z"
}
```

- `actor_id`: Required. String, 1–128 chars.
- `role`: Required. Must be a known role.
- `scopes`: Optional. If omitted, defaults to the role's standard scope set.
- `description`: Optional. Max 256 chars.
- `expires_at`: Required. Must be in the future and within the max TTL bound.

#### Response `201 Created`

```json
{
  "token_id": "tok_2vPqN5L8wX9mK3J",
  "token_value": "fgt_abc123...xyz789",
  "actor_id": "operator-alice",
  "role": "operator",
  "scopes": ["approval:resolve", "provenance:read", "policy:read"],
  "description": "Alice's operator token for on-call shifts",
  "expires_at": "2026-08-20T00:00:00Z",
  "created_at": "2026-05-20T00:00:00Z",
  "last_used_at": null,
  "revoked_at": null,
  "revoked_reason": null,
  "rotated_from": null
}
```

**IMPORTANT**: `token_value` is returned **exactly once**. The server stores only a hash. There is no "get token value" endpoint.

#### Response `400 Bad Request`

```json
{
  "error": "invalid_request",
  "message": "expires_at exceeds maximum TTL of 90 days"
}
```

#### Response `403 Forbidden`

```json
{
  "error": "forbidden",
  "message": "required scope admin:tokens"
}
```

#### Response `409 Conflict`

```json
{
  "error": "duplicate_actor_id",
  "message": "actor_id 'operator-alice' already has an active token"
}
```

*Note: The duplicate-actor-id check is optional and can be disabled per operator preference. It is listed here as a safe default.*

---

### `GET /v1/admin/tokens`

List scoped tokens. Returns metadata only; never returns `token_value`.

#### Query parameters

| Param | Type | Description |
|-------|------|-------------|
| `actor_id` | string | Filter by exact actor_id match |
| `role` | string | Filter by role |
| `active_only` | boolean | If true, exclude revoked and expired tokens |
| `limit` | integer | Max items per page (default 50, max 200) |
| `cursor` | string | Pagination opaque cursor |

#### Response `200 OK`

```json
{
  "items": [
    {
      "token_id": "tok_2vPqN5L8wX9mK3J",
      "actor_id": "operator-alice",
      "role": "operator",
      "scopes": ["approval:resolve", "provenance:read", "policy:read"],
      "description": "Alice's operator token for on-call shifts",
      "expires_at": "2026-08-20T00:00:00Z",
      "created_at": "2026-05-20T00:00:00Z",
      "last_used_at": "2026-05-20T12:34:56Z",
      "revoked_at": null,
      "revoked_reason": null,
      "rotated_from": null
    }
  ],
  "next_cursor": null,
  "total": 1
}
```

---

### `DELETE /v1/admin/tokens/{token_id}`

Revoke a scoped token. Irreversible.

#### Request body (optional)

```json
{
  "reason": "Compromised; rotated to tok_7xYzA1B2cD3eF4G"
}
```

#### Response `204 No Content`

Token revoked successfully.

#### Response `404 Not Found`

Token does not exist or is already revoked.

#### Response `403 Forbidden`

Caller lacks `admin:tokens` scope.

---

### `POST /v1/admin/tokens/{token_id}/rotate`

Revoke an existing token and create a new one with the same `actor_id`, `role`, and `scopes`. The new token gets a fresh `token_value` and `expires_at`.

#### Request body

```json
{
  "expires_at": "2026-11-20T00:00:00Z",
  "reason": "Scheduled 90-day rotation"
}
```

- `expires_at`: Optional. Defaults to current time + max TTL.
- `reason`: Optional. Recorded in `revoked_reason` of the old token.

#### Response `201 Created`

Returns the new token object with `token_value` (exactly once) and `rotated_from` set to the old `token_id`.

```json
{
  "token_id": "tok_7xYzA1B2cD3eF4G",
  "token_value": "fgt_def456...uvw012",
  "actor_id": "operator-alice",
  "role": "operator",
  "scopes": ["approval:resolve", "provenance:read", "policy:read"],
  "description": "Alice's operator token for on-call shifts",
  "expires_at": "2026-11-20T00:00:00Z",
  "created_at": "2026-05-20T12:00:00Z",
  "last_used_at": null,
  "revoked_at": null,
  "revoked_reason": null,
  "rotated_from": "tok_2vPqN5L8wX9mK3J"
}
```

#### Response `404 Not Found`

Old token does not exist or is already revoked.

#### Response `409 Conflict`

Old token is already revoked or expired; cannot rotate.

## Error response schema

All error responses use the existing gateway error format:

```json
{
  "error": "error_code",
  "message": "Human-readable description"
}
```

## Token value format

- Prefix: `fgt_` (FerrumGate Token)
- Length: 64 characters after prefix
- Alphabet: `[A-Za-z0-9_-]` (URL-safe base64)
- Entropy: ~384 bits (48 bytes before base64)
- Example: `fgt_aB3x9...zK7m2`

## Token hash storage

- Algorithm: `argon2id` or `blake3` (TBD based on performance requirements; `blake3` recommended for speed)
- Salt: Random 16 bytes per token, stored alongside hash
- Verification: Hash provided token value, compare to stored hash + salt

## Enforcement behavior

When scoped-token enforcement is enabled (`auth_mode = Scoped`):

1. Extract token from `Authorization: Bearer <token>` header.
2. Hash token value, look up in token store.
3. If not found → `401 Unauthorized`.
4. If `revoked_at` is set → `401 Unauthorized`.
5. If `expires_at` is in the past → `401 Unauthorized`.
6. Update `last_used_at` (best-effort, async).
7. Check requested endpoint against token's `scopes` using this mapping.
8. If scope missing → `403 Forbidden`.
9. Proceed to handler.

## Non-claims

- **NOT production-ready**: Scoped-token enforcement requires explicit operator enablement; bearer-only remains the production pilot auth mode until then.
- **NOT final**: Operator review may request changes to fields, error codes, or behavior.
- **Engineering evidence only**: Implementation evidence compiled 2026-05-22; full operator signoff and Phase 4 signoff remaining.

## Related docs

- [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) — Security and tenant model ADR
- [`12-endpoint-to-scope-mapping.md`](12-endpoint-to-scope-mapping.md) — Endpoint-to-scope mapping
- [`14-ferrumctl-admin-tokens-cli-spec.md`](14-ferrumctl-admin-tokens-cli-spec.md) — CLI surface spec
- [`15-revocation-durability-tradeoff.md`](15-revocation-durability-tradeoff.md) — Revocation durability tradeoff

---

*End of file — Token API Contract (planning artifact only).*
