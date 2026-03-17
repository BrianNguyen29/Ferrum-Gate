# 08 — Repository structure

## Top-level nên có
- `.github/`
- `bins/`
- `configs/`
- `contracts/`
- `crates/`
- `docs/`
- `examples/`
- `openapi/`
- `prompts/`
- `schemas/`
- `scripts/`
- `tests/`

## Crates chính
- `ferrum-proto`
- `ferrum-pdp`
- `ferrum-cap`
- `ferrum-rollback`
- `ferrum-gateway`

## Crates hỗ trợ
- `ferrum-firewall`
- `ferrum-store`
- `ferrum-graph`
- `ferrum-ledger`
- `ferrum-adapter-fs`
- `ferrum-adapter-git`
- `ferrum-adapter-sqlite`
- `ferrum-adapter-http`
- `ferrum-adapter-maildraft`
- `ferrum-testkit`

## Binaries
- `ferrumd`
- `ferrumctl`

## Repo rule
- `ferrum-proto` ở tầng thấp nhất
- gateway ở tầng orchestration trên cùng
- adapters không phụ thuộc gateway
