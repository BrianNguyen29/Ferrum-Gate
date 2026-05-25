# FerrumGate Docs

> **Repository workspace**: `/home/uong_guyen/work/Ferrum-Gate`. Older workspace paths in dated evidence artifacts are historical references; use the path above for current operations.

Đây là **thư mục docs duy nhất** dùng làm nền triển khai cho dự án FerrumGate.

Mục tiêu của bộ docs này:
- mô tả đầy đủ, nhất quán và rõ ràng về dự án
- giúp AI agents hoặc engineers khác có thể bám vào để triển khai
- tránh phải tra cứu rời rạc giữa nhiều file/spec khác nhau

## Cách dùng

Nếu bạn chỉ có thời gian đọc ít tài liệu, hãy đọc theo đúng thứ tự sau:

1. `implementation-path/00-start-here.md`
2. `implementation-path/01-current-state.md`
3. `ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md`
4. `implementation-path/23-production-readiness-assessment.md`
5. `implementation-path/31-release-paths-todo.md`
6. `implementation-path/54-operator-signoff-packet.md`
7. `implementation-path/55-phase-3-go-no-go-review.md`

Sau đó mới đọc các tài liệu nền tảng khi cần:
- `ferrumgate-roadmap-v1/06-constraints-and-invariants.md`
- `ferrumgate-roadmap-v1/14-api-and-contracts-map.md`
- `ferrumgate-roadmap-v1/18-single-node-operations-runbook.md`
- `ferrumgate-roadmap-v1/20-v1-single-node-operator-checks.md`
- `ferrumgate-roadmap-v1/21-v1-single-node-observability-minimums.md`
- `implementation-path/25-EV-v1-single-node-rc-evidence.md`
- `implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`
- `ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/README.md` — planning reference cho post-v1/Phase 3; không override support contract

Các tài liệu `ferrumgate-roadmap-v1/00-project-canon.md` và roadmap-v1
đời đầu là historical/superseded cho trạng thái v1 hiện tại; chỉ dùng để
hiểu bối cảnh, không dùng làm nguồn quyết định feature/status.

## Thư mục con
- `90-docs-governance-phase1.md` — Phase 1 docs inventory, canonical map, overlap matrix (governance artifact)
- `implementation-path/` — lộ trình triển khai cực cụ thể cho agent khác
- `diagrams/` — sơ đồ trực quan về kiến trúc, flow, state machine, constraints

## Source of truth ưu tiên

Khi có mâu thuẫn, ưu tiên theo thứ tự:

1. `ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md`
2. `implementation-path/01-current-state.md`
3. `implementation-path/31-release-paths-todo.md`
4. `implementation-path/23-production-readiness-assessment.md`
5. `ferrumgate-roadmap-v1/06-constraints-and-invariants.md`
6. phần còn lại của `docs/`

## Bước tiếp theo hiện tại

- Path 1 đã xong: `v0.1.0-rc.1` là GitHub prerelease.
- Path 2 đang chờ operator: hoàn tất/ký `implementation-path/54-operator-signoff-packet.md` trước production pilot.
- Path 3 chưa bắt đầu: PostgreSQL/Phase 3 chỉ mở sau khi G2 pilot confirmation và G3.2–G3.4 thỏa mãn.

## Kết luận ngắn

FerrumGate là một **intent-scoped reversible execution plane** cho MCP/tool-using agents.
Mọi action có side effect phải đi qua:
- intent
- policy
- capability
- provenance
- rollback
