# ferrum-sync

> **Status: Runtime prototype / internal-use only.**
>
> This crate is experimental and not yet stable for external consumers. APIs may change without deprecation. Integration with the main gateway is partial and evolving.

Read-only sync probe facade and fakeable transport adapter for FerrumGate.

## Responsibilities

- [`ProbeFacade`]: A read-only diagnostic facade for health, readiness, and status probes
- [`Transport`]: A minimal provider trait for transport operations (fakeable)
- [`FakeTransport`]: An in-memory transport implementation for tests/development
- [`ExternalEventSource`]: A trait for polling external event sources (e.g., MCP runtimes)
- [`FakeExternalEventSource`]: An in-memory external event source for tests/development
- Decision kernel, preflight checker, and diff classifier for Sync-1/Sync-2
- External event source polling for provenance events (Sync-3)

## Design Principles

- **Read-only only**: No write-path, no ledger mutations, no side effects
- **Fakeable**: Transport trait can be satisfied with in-memory implementations
- **No real network**: Designed to work without HTTP, gRPC, or external dependencies
- **Internal DTOs**: All transport DTOs and errors stay within this crate

## Integration Status

- `ferrum-sync` is used internally by sync and MCP bridge paths.
- It is **not** a required dependency of `ferrum-gateway` or `ferrumd`.
- External users should not depend on this crate until it reaches a stable release status.
