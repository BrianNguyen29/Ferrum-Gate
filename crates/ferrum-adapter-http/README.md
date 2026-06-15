# ferrum-adapter-http

HTTP adapter for FerrumGate.

## Responsibilities

- HTTP mutation requests (POST/PUT/PATCH) with idempotency-key replay
- Validate target URL, method, and expected status codes
- Limited rollback via replay for supported methods

## Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| HttpMutation | Validate target/method/URL | Send request | Status/code matches | Replay with idempotency key (POST/PUT/PATCH only) |

## Rollback and risk

- Rollback/compensate succeeds only for strict one-step `http.replay_v1` POST/PUT/PATCH with exact URL/digest binding and strict `expected_statuses`.
- Fails closed otherwise.
- Default risk class: R2.

## Configuration / allowlist gotchas

- The HTTP adapter is disabled until its allowlist is configured.
- External API availability is outside FerrumGate control; verify step may fail due to transient errors.

## Reference

Full details, examples, and risk class mapping: [`docs/guides/adapter-reference.md`](../../docs/guides/adapter-reference.md)
