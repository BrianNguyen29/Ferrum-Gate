# FerrumGate Docs

Đây là **thư mục docs duy nhất** dùng làm nền triển khai cho dự án FerrumGate.

Mục tiêu của bộ docs này:
- mô tả đầy đủ, nhất quán và rõ ràng về dự án
- giúp AI agents hoặc engineers khác có thể bám vào để triển khai
- tránh phải tra cứu rời rạc giữa nhiều file/spec khác nhau

## Cách dùng

Nếu bạn chỉ có thời gian đọc ít tài liệu, hãy đọc theo đúng thứ tự sau:

1. `ferrumgate-roadmap-v1/00-project-canon.md`
2. `ferrumgate-roadmap-v1/01-quickstart.md`
3. `ferrumgate-roadmap-v1/02-project-overview.md`
4. `ferrumgate-roadmap-v1/03-architecture.md`
5. `ferrumgate-roadmap-v1/04-runtime-flow.md`
6. `ferrumgate-roadmap-v1/05-domain-model.md`
7. `ferrumgate-roadmap-v1/06-constraints-and-invariants.md`
8. `ferrumgate-roadmap-v1/07-policy-and-security-model.md`
9. `ferrumgate-roadmap-v1/08-repository-structure.md`
10. `ferrumgate-roadmap-v1/09-implementation-path.md`

Sau đó mới đọc:
- `ferrumgate-roadmap-v1/10-crate-by-crate-plan.md`
- `ferrumgate-roadmap-v1/11-testing-strategy.md`
- `ferrumgate-roadmap-v1/12-persistence-and-data-model.md`
- `ferrumgate-roadmap-v1/13-adapter-contracts.md`
- `ferrumgate-roadmap-v1/14-api-and-contracts-map.md`
- `ferrumgate-roadmap-v1/15-deployment-and-operations.md`
- `ferrumgate-roadmap-v1/16-release-checklist.md`
- `ferrumgate-roadmap-v1/17-troubleshooting.md`
- `ferrumgate-roadmap-v1/18-single-node-operations-runbook.md`
- `ferrumgate-roadmap-v1/20-v1-single-node-operator-checks.md`
- `ferrumgate-roadmap-v1/21-v1-single-node-observability-minimums.md`

## Thư mục con
- `90-docs-governance-phase1.md` — Phase 1 docs inventory, canonical map, overlap matrix (governance artifact)
- `implementation-path/` — lộ trình triển khai cực cụ thể cho agent khác
- `diagrams/` — sơ đồ trực quan về kiến trúc, flow, state machine, constraints

## Source of truth ưu tiên

Khi có mâu thuẫn, ưu tiên theo thứ tự:

1. `00-project-canon.md`
2. `06-constraints-and-invariants.md`
3. `09-implementation-path.md`
4. `10-crate-by-crate-plan.md`
5. phần còn lại của `docs/`

## Kết luận ngắn

FerrumGate là một **intent-scoped reversible execution plane** cho MCP/tool-using agents.
Mọi action có side effect phải đi qua:
- intent
- policy
- capability
- provenance
- rollback
