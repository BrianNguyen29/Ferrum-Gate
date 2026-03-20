# ferrum-adapter-http

HTTP adapter for idempotency-aware rollback and compensation.

## Status

Current slice: HTTP execute/verify with approved/bound concrete HTTP methods (GET/POST/PUT/PATCH/DELETE) with body handling. Verify uses execute-time metadata for mutations (no replay).

## Supported Operations

| Operation | Behavior |
|-----------|----------|
| `prepare` | Captures bound scope and approved concrete request digest |
| `execute` | Performs HTTP requests (GET/POST/PUT/PATCH/DELETE); rejects digest mismatch |
| `verify` | Validates status: GET can re-request; mutations use execute-time metadata only |
| `rollback` | Conservative no-op (mutation recovery is R3 boundary) |
| `compensate` | Alias for rollback |

## Body-Aware Digest Semantics

For all HTTP methods, the approved request digest is computed as:

- `GET`: digest = SHA256(method:url) - no body involved
- `POST/PUT/PATCH/DELETE`: digest = SHA256(method:url:body) where body is canonical JSON or empty

This lets prepare bind the approved request shape without broadening remote mutation execution or recovery semantics.

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
- No authentication or custom headers in this slice

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
  "body": {"name": "test", "email": "test@example.com"}
}
```

All fields are optional. If omitted, bound values from prepare are used. For GET, body is ignored for digest purposes.

## Verification Checks

To verify an HTTP status, add an `HttpStatusExpected` check to `verify_checks`:

```json
{
  "check_type": "HttpStatusExpected",
  "config": { "status": 201 }
}
```

For mutation methods, this check validates against the execute-time status (no replay).
