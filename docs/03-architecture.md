# 03 — Architecture

> **Role**: Structural / component overview. Defines the building blocks (components, layers, dependency rules) of FerrumGate. For execution sequencing and state flow, see [`04-runtime-flow.md`](./04-runtime-flow.md). For adapter interface specs, see [`13-adapter-contracts.md`](./13-adapter-contracts.md). For API route mapping, see [`14-api-and-contracts-map.md`](./14-api-and-contracts-map.md). For invariants that govern component behavior, see [`06-constraints-and-invariants.md`](./06-constraints-and-invariants.md).

## 1. Thành phần chính

### A. Intent Compiler
Goal + plan + resource request + trust context -> `IntentEnvelope`

### B. Semantic Firewall
- trust labels
- taint scoring
- contradiction checks
- DLP
- sanitize output

### C. Policy PDP
Ra quyết định:
- Allow
- Deny
- Quarantine
- RequireApproval
- AllowDraftOnly

### D. Capability Mint
Phát `CapabilityLease` cực hẹp, TTL ngắn, single-use.

### E. Rollback Kernel
Tạo `RollbackContract` và chạy:
- prepare
- verify
- compensate
- rollback

> Per-adapter rollback mechanics (FS, SQLite, Git, HTTP, Maildraft) are defined in [`13-adapter-contracts.md`](./13-adapter-contracts.md). Rollback class invariants (R0–R3, auto-commit rules) are in [`06-constraints-and-invariants.md`](./06-constraints-and-invariants.md).

### F. Gateway / Interceptor
Chặn và điều phối tool calls trước khi forward upstream. Routes mutating bindings to adapters and enforces scope-bounds. API surface is enumerated in [`14-api-and-contracts-map.md`](./14-api-and-contracts-map.md).

### G. Provenance + Ledger
Ghi lineage, audit trail, reasoning trail của execution.

## 2. Tầng kiến trúc

### Core domain layer
- proto
- pdp
- cap
- rollback

### Support layer
- firewall
- store
- graph
- ledger

### Adapter layer
- fs
- git
- sqlite
- http
- maildraft

### Orchestration layer
- gateway
- ferrumd
- ferrumctl

## 3. Luật phụ thuộc

- `ferrum-proto` ở thấp nhất
- adapters không phụ thuộc gateway
- gateway là orchestration top layer
- store không nên phụ thuộc gateway
