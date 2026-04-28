# 08 — Repository structure

> **⚠️ Historical / Planning-era**: This document describes an intended/designed repository structure. Some crates listed here may not exist or may have different names/purposes in the current workspace. Do not treat unchecked boxes as authoritative pending-work status.
>
> **For current state**: See `docs/implementation-path/01-current-state.md` for what actually exists and the current workspace structure.



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
