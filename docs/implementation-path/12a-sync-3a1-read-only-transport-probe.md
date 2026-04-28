# 12a — Read-Only Transport Probe Skeleton

**Slice**: Transport adapter skeleton implementation  
**Status**: Implemented — `ferrum-sync` crate with `Transport` trait and `FakeTransport`  
**Note**: No real sync transport implementation exists — this is infrastructure only

## What Was Implemented

### `ferrum-sync` Crate

A minimal crate providing:

1. **`Transport` trait** — minimal provider interface for read-only probe operations
2. **`FakeTransport`** — in-memory implementation that is fully fakeable
3. **`ProbeFacade`** — read-only diagnostic facade

### Key Properties

- **Read-only**: All operations (`probe`, `health_check`) are diagnostic only
- **No write-path**: No mutations, no ledger changes
- **No real network**: `FakeTransport` works entirely in-memory
- **Internal DTOs**: `ProbeResponse` and `SyncError` stay within `ferrum-sync`

### Files Changed

| File | Change |
|------|--------|
| `crates/ferrum-sync/Cargo.toml` | New crate |
| `crates/ferrum-sync/src/lib.rs` | Module exports |
| `crates/ferrum-sync/src/error.rs` | `SyncError` type |
| `crates/ferrum-sync/src/transport.rs` | `Transport` trait + `FakeTransport` |
| `crates/ferrum-sync/src/facade.rs` | `ProbeFacade` |
| `Cargo.toml` | Added `ferrum-sync` to workspace |

## Tests

The crate includes tests proving:

- `FakeTransport` can be configured with custom probe responses
- `FakeTransport` health check can be healthy or unhealthy
- `ProbeFacade` correctly delegates to transport
- `ProbeFacade` provides read-only `health()`, `ready()`, `status()`, `probe()` methods

```bash
cargo test -p ferrum-sync
```

## Design Decisions

1. **Service-internal adapter boundary**: Does not expose transport DTOs externally
2. **Minimal provider trait**: `Transport` has only two methods — `probe` and `health_check`
3. **Fakeable by construction**: No async initialization, no connection pools needed
4. **No protocol commitment**: HTTP/gRPC choice deferred to future implementation

## Out of Scope

- Real HTTP/gRPC transport implementation
- Connection pooling or retry logic
- Write/apply operations
- Ledger integration

## Verification Evidence

```
running 9 tests
tests::facade::tests::probe_facade_cloneable ... ok
tests::facade::tests::probe_facade_health_err ... ok
tests::facade::tests::probe_facade_generic_probe ... ok
tests::facade::tests::probe_facade_health_ok ... ok
tests::facade::tests::probe_facade_default_probe ... ok
tests::facade::tests::probe_facade_ready ... ok
tests::facade::tests::probe_facade_status ... ok
tests::transport::tests::fake_transport_probe_wildcard_default ... ok
tests::transport::tests::fake_transport_health_err ... ok
tests::transport::tests::fake_transport_probe_custom ... ok
tests::transport::tests::fake_transport_health_ok ... ok
tests::transport::tests::fake_transport_probe_default ... ok
```

All tests pass with no network, no write-path, fully fakeable.

---

**Links to**:
- `12-sync-3a-probe-api-boundary.md` (this slice's plan)
- `08-next-issue-backlog.md` (P2: fs/git/http adapters; fs/git now have partial verified local slices)
- `11-remaining-tasks.md` (future adapter work)
