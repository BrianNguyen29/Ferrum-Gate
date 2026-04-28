# 12 — Sync Probe Adapter Boundary

**Slice**: Docs-first adapter implementation plan  
**Status**: Skeleton only — no real sync implementation exists yet  
**Constraints**: Read-only, fakeable, no real network, no write-path

## Context

Following the probe facade boundary work, this slice establishes the smallest safe adapter boundary for sync operations. The goal is a **service-internal adapter boundary wrapping a minimal provider trait** rather than committing to HTTP or gRPC now.

## Goals for This Slice

1. **Minimal `Transport` trait** — read-only operations only, fakeable in-memory
2. **`FakeTransport`** — in-memory implementation for tests/dev, no network
3. **`ProbeFacade`** — read-only diagnostic facade using the transport
4. **Internal DTOs/errors** — all transport types stay within `ferrum-sync`

## What This Slice Does NOT Include

- No real HTTP/gRPC client
- No gateway integration
- No CLI integration
- No write/apply path
- No ledger mutation

## Files

```
crates/ferrum-sync/
├── src/
│   ├── lib.rs          # Public API re-exports
│   ├── error.rs        # SyncError (internal)
│   ├── facade.rs       # ProbeFacade (read-only diagnostic)
│   └── transport.rs   # Transport trait + FakeTransport
└── Cargo.toml
```

## Transport Trait Design

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn probe(&self, probe_kind: &str) -> Result<ProbeResponse>;
    async fn health_check(&self) -> Result<()>;
}
```

Key properties:
- **Read-only**: `probe` and `health_check` have no side effects
- **Fakeable**: `FakeTransport` satisfies this without network
- **No write-path**: No mutation operations exposed

## ProbeFacade API

```rust
impl ProbeFacade {
    pub async fn health(&self) -> Result<()>;
    pub async fn ready(&self) -> Result<ProbeResponse>;
    pub async fn status(&self) -> Result<ProbeResponse>;
    pub async fn probe(&self, kind: &str) -> Result<ProbeResponse>;
}
```

## Next Steps (Out of Scope for This Slice)

1. Real transport implementation (HTTP, gRPC, or IPC)
2. Integration with gateway/runtime
3. Write/apply path for actual sync operations
4. Ledger mutation support

## Verification

```bash
cargo test -p ferrum-sync
```

All tests pass with `FakeTransport` — no real network required.

---

**Linked from**: `08-next-issue-backlog.md` (P2: adapters), `11-remaining-tasks.md` (future adapter work)
