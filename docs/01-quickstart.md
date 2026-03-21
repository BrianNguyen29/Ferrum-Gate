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

# Run (no runtime config knobs yet)
cargo run -p ferrumd

# Binary also available after build:
./target/debug/ferrumd
```

ferrumd starts a single HTTP server on `127.0.0.1:8080` and connects to an in-memory SQLite store (`sqlite::memory:?cache=shared`). All state is lost when the process exits.

## Điều không được làm

- dùng session như quyền ngầm
- gọi mutation mà không qua gateway
- bỏ qua capability validation
- bỏ qua rollback prepare
- commit R3 mà không approval / draft-only
- coi action là "xong" nếu chưa verify và chưa có lineage
