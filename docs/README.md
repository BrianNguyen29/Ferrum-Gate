# FerrumGate Docs

Đây là **thư mục docs duy nhất** dùng làm nền triển khai cho dự án FerrumGate.

Mục tiêu của bộ docs này:
- mô tả đầy đủ, nhất quán và rõ ràng về dự án
- giúp AI agents hoặc engineers khác có thể bám vào để triển khai
- tránh phải tra cứu rời rạc giữa nhiều file/spec khác nhau

## Cách dùng

Nếu bạn chỉ có thời gian đọc ít tài liệu, hãy đọc theo đúng thứ tự sau:

1. `00-project-canon.md`
2. `01-quickstart.md`
3. `02-project-overview.md`
4. `03-architecture.md`
5. `04-runtime-flow.md`
6. `05-domain-model.md`
7. `06-constraints-and-invariants.md`
8. `07-policy-and-security-model.md`
9. `08-repository-structure.md`
10. `09-implementation-path.md`

Sau đó mới đọc:
- `10-crate-by-crate-plan.md`
- `11-testing-strategy.md`
- `12-persistence-and-data-model.md`
- `13-adapter-contracts.md`
- `14-api-and-contracts-map.md`
- `15-deployment-and-operations.md`
- `16-release-checklist.md`
- `17-troubleshooting.md`

## Thư mục con
- .agents - các kĩ năng chuyên biệt về rust, hãy luôn sử dụng trong quá trình làm việc, và yêu cầu các sub-agents cũng sử dụng khi cần
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
