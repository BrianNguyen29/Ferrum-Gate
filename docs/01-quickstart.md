# 01 — Quickstart

## Mục tiêu

Giúp agent hoặc engineer mới hiểu nhanh:
- đọc gì trước
- bắt đầu từ đâu
- không được phá gì

## Thứ tự đọc

1. `00-project-canon.md`
2. `02-project-overview.md`
3. `03-architecture.md`
4. `04-runtime-flow.md`
5. `05-domain-model.md`
6. `06-constraints-and-invariants.md`
7. `09-implementation-path.md`
8. `10-crate-by-crate-plan.md`

## Happy path tối thiểu của FerrumGate

1. compile intent
2. evaluate proposal
3. mint capability
4. prepare rollback
5. execute tool/adapters
6. verify
7. commit hoặc compensate / rollback
8. emit provenance chain

## Control-plane API lifecycle (operator reference)

```
compile -> evaluate -> mint -> authorize -> prepare -> execute -> verify -> commit/rollback
```

| Step | What happens |
|------|-------------|
| compile | Intent is parsed and scoped to a manifest |
| evaluate | PDP engine evaluates policy against intent |
| mint | A limited-capability lease is issued for the scope |
| authorize | Capability is checked at the gateway before execution |
| prepare | Rollback contract is prepared (noop, fs, git, sqlite, http, or maildraft) |
| execute | Adapter runs the tool/action |
| verify | Result is checked against the intent and policy |
| commit/rollback | On success: commit. On failure: rollback via prepared adapter |

Note: for HTTP adapters, rollback is a **no-op by design** today; manual compensation is required if remote state was mutated.

## Running ferrumd (local/dev)

```sh
# Build
cargo build -p ferrumd

# Run with the repo dev config (auto-loaded when present)
cargo run -p ferrumd

# Or point to a specific config file explicitly
cargo run -p ferrumd -- --config configs/ferrumgate.dev.toml

# Print the resolved effective config and exit
cargo run -p ferrumd -- --print-effective-config

# Preflight startup guard without starting the server
cargo run -p ferrumd -- --check-startup-guard

# Override config via CLI/env when needed
FERRUMD_STORE_DSN="sqlite://./tmp/ferrumgate.sqlite" \
FERRUMD_LOG_FILTER=debug \
cargo run -p ferrumd -- --bind 127.0.0.1:8081

# Binary also available after build:
./target/debug/ferrumd
```

Config precedence is `CLI > env > config file > defaults`.

In repo-local development, `ferrumd` auto-loads `configs/ferrumgate.dev.toml` when it exists. That config keeps the default loopback bind (`127.0.0.1:8080`) but uses a file-backed SQLite store (`sqlite://ferrumgate.dev.db`), so state survives restarts.

If no config file is found, `ferrumd` falls back to `sqlite::memory:?cache=shared`.

## Control-plane auth (current operator path)

- `auth.mode = "disabled"` is acceptable for loopback/local dev.
- Non-loopback bind with auth disabled is rejected at startup unless `allow_insecure_nonlocal = true` is set explicitly.
- `auth.mode = "bearer"` requires a bearer token from config or `FERRUMD_BEARER_TOKEN`.
- TLS is still expected to be terminated by a reverse proxy or other ingress layer; in-process TLS is not implemented here.

## ferrumctl quick checks

```sh
# Health
cargo run -p ferrumctl -- server health

# Ready check
cargo run -p ferrumctl -- server ready

# Inspect an execution
cargo run -p ferrumctl -- server inspect-execution <execution_id>

# List pending approvals
cargo run -p ferrumctl -- server inspect-approvals

# Inspect lineage (text)
cargo run -p ferrumctl -- server inspect-lineage <execution_id>

# Inspect lineage (JSON)
cargo run -p ferrumctl -- server inspect-lineage <execution_id> --format json

# Inspect lineage (DOT/Graphviz) and write to file
cargo run -p ferrumctl -- server inspect-lineage <execution_id> --format dot --output lineage.dot

# Query terminal provenance events for an execution
cargo run -p ferrumctl -- server inspect-provenance \
  --execution-id <execution_id> \
  --terminal-only

# Query provenance events with pagination
cargo run -p ferrumctl -- server inspect-provenance \
  --limit 100

# Resume a paginated query using a cursor
cargo run -p ferrumctl -- server inspect-provenance \
  --cursor <next_cursor>

# Export all provenance events across all pages (JSONL, one event per line)
cargo run -p ferrumctl -- server inspect-provenance \
  --all-pages

# Inspect a single provenance event by ID
cargo run -p ferrumctl -- server inspect-event <event_id>

# Inspect event with ancestry and descendants
cargo run -p ferrumctl -- server inspect-event <event_id> --ancestry --descendants

# Ingest an external runtime event into provenance lineage
# (operator boundary: records vendor-neutral external observations)
cargo run -p ferrumctl -- server ingest-external-event \
  --execution-id <uuid> \
  --parent-event-id <uuid> \
  --source-system <string> \
  --source-event-id <string>
```

`ferrumctl` defaults to `http://127.0.0.1:8080`. If control-plane bearer auth is enabled, pass `--bearer-token <token>` or set `FERRUMCTL_BEARER_TOKEN`.

## Điều không được làm

- dùng session như quyền ngầm
- gọi mutation mà không qua gateway
- bỏ qua capability validation
- bỏ qua rollback prepare
- commit R3 mà không approval / draft-only
- coi action là "xong" nếu chưa verify và chưa có lineage
