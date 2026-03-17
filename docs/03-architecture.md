# 03 — Architecture

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

### F. Gateway / Interceptor
Chặn và điều phối tool calls trước khi forward upstream.

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
