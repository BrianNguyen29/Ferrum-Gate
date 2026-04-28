# 00 — Roadmap charter

## 1. FerrumGate phải trở thành gì

FerrumGate là lớp execution governance cho agentic systems. Bản roadmap này chốt định vị sản phẩm như sau:

> FerrumGate là execution governance layer cho các agent chạm vào hệ thống thật, bắt đầu từ file, git, database, controlled HTTP mutation và sau đó mở rộng sang MCP/open runtime.

Nó không phải:
- agent platform tổng quát
- chatbot app
- AI firewall chung chung
- observability-only product

## 2. Vì sao chọn hướng này

Hướng này khớp nhất với lõi hiện có của FerrumGate:
- intent-first
- capability-scoped
- rollback-by-default
- provenance-aware
- agent-followable

Nó cũng khớp nhất với phạm vi tài liệu hiện tại:
- gateway/interceptor làm lớp chặn tool call
- rollback contract và adapter contracts đã được định nghĩa
- provenance/lineage đã có API và CLI inspect
- policy model đã có Allow / Deny / Quarantine / RequireApproval / AllowDraftOnly

## 3. Điểm xuất phát thực tế

Trạng thái hiện tại cần được hiểu chính xác. **Để biết chính xác v1 hỗ trợ gì và không hỗ trợ gì,
xem `19-v1-single-node-support-contract.md` — đó là canon boundary duy nhất cho v1.**

### Đã có (theo v1 support contract)
- governance core single-node với SQLite-backed persistence
- các route health, readiness, evaluate, mint, authorize, prepare, inspect execution, approvals, provenance query/lineage
- CLI inspect cơ bản
- test evidence cho scope mismatch deny, single-use capability test, R3 no auto-commit, quarantine path, compensate flow, approvals pagination/filter, lineage endpoint shape

### Chưa có hoặc mới một phần
- adapter implementations thật cho fs/sqlite/maildraft/git/http — các crate tồn tại dưới dạng skeleton nhưng chưa có production-verified side-effect integration
- commit/rollback routes trong v1 router — **không exposed** trong v1 router; gateway flow kết thúc ở compensate
- HA / multi-node / read replica — out of v1 scope hoàn toàn
- operator UI hoàn chỉnh — post-v1 scope
- end-to-end enforcement sạch cho mọi invariant đã nêu trong docs — có accepted risks đã được ghi trong v1 support contract

### Điều quan trọng về codebase hiện tại

- Code ngoài v1 support contract có thể tồn tại trong repo (adapter crate shapes, non-v1 routes, CLI commands v.v.)
- Sự tồn tại của code đó **không mở rộng** v1 support contract
- Chỉ `19-v1-single-node-support-contract.md` là authoritative cho v1 scope

## 4. Chiến lược phát triển

Roadmap này đi theo 5 nguyên tắc:

1. **Fix correctness trước khi mở rộng category**
2. **Làm adapter thật cho side effect có ROI rõ**
3. **Đóng gói thành product wedge trước khi nói về standard**
4. **Ưu tiên self-hosted / private deployment trước cloud-first**
5. **Mọi expansion phải bám theo intent -> policy -> capability -> rollback -> provenance**

## 5. Wedge thương mại hóa đầu tiên

Wedge đầu tiên:

> FerrumGate for governed engineering changes

Bao gồm:
- governed filesystem mutation
- governed git changes
- governed database mutation
- approval / quarantine / verify / recovery cho các action trên

## 6. Mốc 12 tháng

- **Q1**: v1.1 Kernel Hardening
- **Q2**: Governed Engineering Changes Beta
- **Q3**: Self-hosted Commercial Beta
- **Q4**: MCP Governance Beta + Enterprise Evidence Alpha

## 7. Anti-roadmap

Không làm trong chu kỳ này:
- full agent orchestration platform
- general-purpose AI assistant product
- multi-tenant SaaS complete platform
- marketplace cho agent
- sandbox/OS isolation toàn diện ở mức kernel
- overdesign cryptographic capability chain nếu chưa có product wedge
