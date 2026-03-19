# ferrum-adapter-http

HTTP adapter for idempotency-aware rollback and compensation.

## Status

Initial slice implemented: status-only HTTP GET verification with conservative rollback.

## Supported Operations

| Operation | Behavior |
|-----------|----------|
| `prepare` | Captures HTTP method, URL, and computes request digest |
| `execute` | Performs real HTTP GET request; returns status code |
| `verify` | Validates expected HTTP status from `HttpStatusExpected` check |
| `rollback` | Conservative no-op (HTTP GET has no side effects) |
| `compensate` | Alias for rollback |

## Limitations (This Slice)

- Only HTTP GET is supported for execute/verify
- Response bodies are not captured or compared
- rollback/compensate are no-ops (GET is inherently read-only)
- No authentication or custom headers in this slice

## Usage

```rust
use ferrum_adapter_http::{HttpRollbackAdapter, register_http_adapter};
use ferrum_rollback::AdapterRegistry;

let mut registry = AdapterRegistry::default();
register_http_adapter(&mut registry);
```

## Verification Checks

To verify an HTTP status, add an `HttpStatusExpected` check to `verify_checks`:

```json
{
  "check_type": "HttpStatusExpected",
  "config": { "status": 200 }
}
```
