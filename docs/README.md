# FerrumGate Docs

> **Repository workspace**: `/home/uong_guyen/work/Ferrum-Gate`.

Đây là **thư mục docs duy nhất** dùng làm nền triển khai cho dự án FerrumGate.

Mục tiêu của bộ docs này:
- mô tả đầy đủ, nhất quán và rõ ràng về dự án
- giúp AI agents hoặc engineers khác có thể bám vào để triển khai
- tránh phải tra cứu rời rạc giữa nhiều file/spec khác nhau

## Cách dùng

Nếu bạn chỉ có thời gian đọc ít tài liệu, hãy đọc theo đúng thứ tự sau:

1. `guides/README.md`
2. `guides/concepts.md`
3. `guides/quickstart.md`
4. `guides/operator.md`
5. `guides/security-model.md`
6. `PRODUCTION_NOTES.md`

## Thư mục con
- `guides/` — hướng dẫn sử dụng, vận hành và triển khai
- `architecture/` — tài liệu kiến trúc
- `security/` — mô hình bảo mật
- `api/` — tài liệu API
- `mcp/` — tài liệu MCP
- `operations/` — hướng dẫn vận hành
- `releases/` — thông tin phát hành
- `diagrams/` — sơ đồ trực quan về kiến trúc, flow, state machine, constraints

## Kết luận ngắn

FerrumGate là một **intent-scoped reversible execution plane** cho MCP/tool-using agents.
Mọi action có side effect phải đi qua:
- intent
- policy
- capability
- provenance
- rollback
