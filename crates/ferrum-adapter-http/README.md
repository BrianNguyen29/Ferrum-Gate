# ferrum-adapter-http

HTTP adapter for idempotency-aware rollback and compensation.

## Status

Current slice: HTTP execute/verify with approved/bound concrete HTTP methods (GET/POST/PUT/PATCH/DELETE), body handling, header-shape binding, canonical query string binding, and dedicated bearer auth representation. Verify uses execute-time metadata for mutations (no replay).

## Supported Operations

| Operation | Behavior |
|-----------|----------|
| `prepare` | Captures bound scope and approved concrete request digest, including headers and auth when present |
| `execute` | Performs HTTP requests (GET/POST/PUT/PATCH/DELETE); rejects digest mismatch |
| `verify` | Validates status: GET can re-request; mutations use execute-time metadata only |
| `rollback` | Conservative no-op (mutation recovery is R3 boundary) |
| `compensate` | Alias for rollback |

## Body-Aware Digest Semantics

For all HTTP methods, the approved request digest is computed from request shape:

- `GET`: digest = SHA256(method:canonical_url[:headers])
- `POST/PUT/PATCH/DELETE`: digest = SHA256(method:canonical_url:body[:headers]) where body is canonical JSON or empty
- Header names are canonicalized to lowercase and sorted before digesting
- Query strings are canonicalized (sorted by key) before digesting so semantically identical query strings produce the same digest
- Bearer auth tokens are included in the digest when specified via the `auth` field

This lets prepare bind the approved request shape without broadening remote mutation execution or recovery semantics.

## Canonical Query String Handling

Query strings in URLs are canonicalized before computing digests to ensure semantically identical queries produce identical digests:

- `?b=2&a=1` and `?a=1&b=2` are treated as the same shape
- Keys are sorted alphabetically; values are preserved as-is
- Empty values (e.g., `?flag` without `=value`) are preserved

Query metadata stored in prepare/execute receipts:
- `approved_query_present` / `executed_query_present`: boolean indicating if query string was present
- `approved_query_digest` / `executed_query_digest`: SHA256 of the canonical query string (empty string if no query)

## Dedicated Bearer Auth Representation

The adapter supports a dedicated `auth` field for bearer authentication, providing an alternative to passing auth via headers:

```json
{
  "url": "https://example.com/api/users",
  "method": "GET",
  "auth": {
    "type": "bearer",
    "token": "my-secret-token"
  }
}
```

### Auth Parsing Rules

- The adapter fail-closed on malformed auth (missing token, empty token, unsupported type)
- Ambiguous auth is rejected: if both `headers.authorization` AND `auth` are supplied, the request is rejected
- Only `bearer` auth type is supported

### Auth Metadata

Auth presence and digest (not raw token) are stored in metadata:
- `approved_auth_present` / `executed_auth_present`: boolean indicating if auth was present
- `approved_auth_digest` / `executed_auth_digest`: SHA256 of the auth token

### Firewall Allowlist Enforcement

When `auth.bearer` is present, the firewall treats it like having the `authorization` header for header allowlist checking purposes. The binding's `header_allowlist` must include `authorization` to permit bearer auth.

## Verify Behavior by Method

### GET Requests
- If explicit `HttpStatusExpected` check: re-requests to verify actual current server state
- If no explicit check: uses execute-time status metadata fallback

### Mutation Requests (POST/PUT/PATCH/DELETE)
- **Always uses execute-time metadata only** - does NOT replay the mutating request
- Fail-closed: if no execute-time status in metadata, verify returns `verified=false`
- Explicit `HttpStatusExpected` check acts as crosscheck against execute-time metadata, not a live request
- Without an explicit check, only execute-time `2xx` statuses auto-verify; `4xx/5xx` stay unverified

## Limitations (This Slice)

- Response bodies are not captured or compared
- rollback/compensate are no-ops for all methods (mutation recovery is R3 boundary)
- Only bearer auth type is supported

## Usage

```rust
use ferrum_adapter_http::{HttpRollbackAdapter, register_http_adapter};
use ferrum_rollback::AdapterRegistry;

let mut registry = AdapterRegistry::default();
register_http_adapter(&mut registry);
```

## Execute Payload Format

```json
{
  "url": "https://example.com/api/users",
  "method": "POST",
  "body": {"name": "test", "email": "test@example.com"},
  "headers": {
    "x-request-id": "req-123"
  },
  "auth": {
    "type": "bearer",
    "token": "my-secret-token"
  }
}
```

All fields are optional. If omitted, bound values from prepare are used. For GET, body is ignored for digest purposes. Headers and auth are validated by gateway allowlist enforcement and bound into request digest when present.

**Note**: Use either `headers.authorization` OR `auth` field, not both. Using both is treated as ambiguous and rejected.

## Verification Checks

To verify an HTTP status, add an `HttpStatusExpected` check to `verify_checks`:

```json
{
  "check_type": "HttpStatusExpected",
  "config": { "status": 201 }
}
```

For mutation methods, this check validates against the execute-time status (no replay).
