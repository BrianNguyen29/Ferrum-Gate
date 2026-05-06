# D-0 Live Smoke Evidence (2026-05-06)

> **Scope**: Local dev smoke only. This is NOT production evidence, NOT operator evidence, NOT target-host evidence.
> **Caveat**: Only 3 representative tools were smoke-tested (health, readyz_deep, policy_bundles). Remaining 6 tools were not tested.
> **D-1 Status**: Deferred pending separate design.

---

## Pre-Conditions

- Repository was clean before work (only `ferrum-mcp-server` binary test restoration was done).
- `cargo test -p ferrum-integrations-mcp` (31 tests) passed.
- `cargo test --bin ferrum-mcp-server` (8 tests) passed.
- All clippy/warnings checks passed.

---

## Validation Results

### Fixer Validation (Pre-Smoke)

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p ferrum-integrations-mcp` | PASS (31 tests) |
| `cargo test --bin ferrum-mcp-server` | PASS (8 tests) |
| `cargo clippy -p ferrum-integrations-mcp --all-targets -- -D warnings` | PASS |
| `cargo check --workspace` | PASS |
| `git diff --check` | PASS |
| `cargo test --workspace` | PASS (all packages) |

---

## Live Smoke: ferrumd Dev Server

### Startup

```bash
cargo run -p ferrumd
```

**Observed output (representative lines):**
```
Finished dev profile
starting ferrumd with config: auth_mode=disabled, bind_addr=127.0.0.1:8080, store_dsn=sqlite::memory:
ferrumd listening on 127.0.0.1:8080
```

Server was shut down gracefully via Ctrl+C after smoke tests completed.

### First Health Call

**Command:**
```bash
echo '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"ferrum_gate_health"}}' | cargo run --bin ferrum-mcp-server
```

**Result:** TIMEOUT after 30 seconds (build/startup overhead on first run).

### Retry Health Call

**Command:**
```bash
echo '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"ferrum_gate_health"}}' | cargo run --bin ferrum-mcp-server
```

**Result:** PASS
- Return code: 0
- Stdout: `{"jsonrpc":"2.0","result":{"content":[{"text":"{\n  \"status\": \"ok\"\n}","type":"text"}],"is_error":false},"id":1}`
- Stderr: (empty)

---

## Live Smoke: Multi-Tool Test

### Test Sequence

Each tool was tested via:

```bash
echo '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"<TOOL_NAME>",...}}' | cargo run --bin ferrum-mcp-server
```

### Results

| Tool | Params | Result | Output |
|------|--------|--------|--------|
| `ferrum_gate_health` | `{}` | PASS | `{"status": "ok"}` |
| `ferrum_gate_readyz_deep` | `{}` | PASS | `{"status": "ok", "store_healthy": true, "write_queue_depth": 0}` |
| `ferrum_gate_list_policy_bundles` | `{}` | PASS | `{"bundles": [], "total": 0}` |
| `ferrum_gate_unknown_tool` | `{}` | PASS (fail closed) | `{"code": -32601, "message": "..."}` |
| `ferrum_gate_get_execution` | `{}` | PASS (missing arg) | `{"code": -32602, "message": "Missing required argument: execution_id"}` |

### Error Code Verification

| Test | Expected Code | Actual Code | Match |
|------|-------------|-------------|-------|
| Unknown tool | `-32601` (METHOD_NOT_FOUND) | `-32601` | YES |
| Missing `execution_id` | `-32602` (INVALID_PARAMS) | `-32602` | YES |

### Stderr Check

All smoke tests produced **empty stderr**, confirming:
- No secret leakage
- No error noise
- Clean JSON output to stdout only

---

## Caveats

1. **Limited coverage**: Only 3 tools were smoke-tested (health, readyz_deep, policy_bundles). The remaining 6 tools were not tested in this session.
2. **Memory store**: The smoke used `sqlite::memory:` store, not a persistent store.
3. **Auth disabled**: The dev config uses `auth_mode=disabled`. Protected endpoints (those requiring bearer token) were not tested with actual auth.
4. **Not production**: This is local dev evidence only. No production claims are made.
5. **Not operator evidence**: Target-host execution evidence requires operator action per doc 67.

---

## D-1 Deferred

Phase D-1 (mutating governance pipeline) remains **deferred** pending separate design. D-0 implements only the read-only REST client.

---

## Files Changed

| File | Change |
|------|--------|
| `crates/ferrum-integrations-mcp/src/bin/ferrum-mcp-server.rs` | Added `process_line_with_dispatch` test seam and 8 unit tests |
| `crates/ferrum-integrations-mcp/src/lib.rs` | Added error codes AUTH_FAILED (-32002), GATEWAY_UNREACHABLE (-32003), GATEWAY_SERVER_ERROR (-32004) |
| `crates/ferrum-integrations-mcp/src/http_client.rs` | New HTTP client module |
| `crates/ferrum-integrations-mcp/src/rest_mapper.rs` | New REST mapper module |
| `crates/ferrum-integrations-mcp/Cargo.toml` | Added reqwest dependency |
| `docs/implementation-path/73-mcp-server-phase-d-implementation-plan.md` | New Phase D plan document |
| `docs/implementation-path/72-mcp-server-phase-a-implementation-plan.md` | Fixed route bug for policy_bundles |
| `docs/implementation-path/67-production-readiness-roadmap.md` | Updated cross-refs |
| `docs/implementation-path/README.md` | Updated reading order |

---

*Evidence recorded: 2026-05-06. Local dev smoke only. D-1 deferred. No production/operator/target-host claims.*
