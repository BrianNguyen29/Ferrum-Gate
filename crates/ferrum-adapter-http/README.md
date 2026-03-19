# ferrum-adapter-http

HTTP adapter for idempotency-aware rollback and compensation.

## Status

Current slice: HTTP GET execute/verify with approved-request digest binding for mutation-capable payloads.

## Supported Operations

| Operation | Behavior |
|-----------|----------|
| `prepare` | Captures bound scope and approved concrete request digest |
| `execute` | Performs HTTP GET only; rejects execute-time digest mismatch |
| `verify` | Validates expected HTTP status from `HttpStatusExpected` check |
| `rollback` | Conservative no-op (mutation recovery is R3 boundary) |
| `compensate` | Alias for rollback |

## Body-Aware Digest Semantics

For mutation-capable payloads (POST/PUT/PATCH/DELETE), the approved request digest includes the request body:

- `GET`: digest = SHA256(method:url) - no body involved
- `POST/PUT/PATCH/DELETE`: digest = SHA256(method:url:body) where body is canonical JSON

This lets prepare bind the approved request shape without broadening remote mutation execution or recovery semantics.

## Limitations (This Slice)

- Response bodies are not captured or compared
- execute/verify still only support GET
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
  "method": "GET",
  "body": {"name": "test", "email": "test@example.com"}
}
```

All fields are optional. If omitted, bound values from prepare are used. For GET, body is ignored for digest purposes.

## Verification Checks

To verify an HTTP status, add an `HttpStatusExpected` check to `verify_checks`:

```json
{
  "check_type": "HttpStatusExpected",
  "config": { "status": 200 }
}
```
