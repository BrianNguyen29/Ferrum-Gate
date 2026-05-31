# ferrum-adapter-http

HTTP adapter for FerrumGate.

## Responsibilities

- HTTP mutation requests (POST/PUT/PATCH) with idempotency-key replay
- Validate target URL, method, and expected status codes
- Limited rollback via replay for supported methods
