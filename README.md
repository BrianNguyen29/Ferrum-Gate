# FerrumGate Unified Project

FerrumGate là một **intent-scoped reversible execution plane** cho MCP agents.

Repository này là **khung dự án thống nhất**, gom lại đầy đủ:
- code scaffold Rust
- docs nghiệp vụ và kiến trúc
- machine-readable contracts
- OpenAPI
- JSON Schemas
- prompts cho AI agents
- examples
- configs
- scripts
- test skeleton
- roadmap và handoff notes

Mục tiêu là để **AI agents hoặc engineers khác có thể bám vào và hoàn thiện tiếp**, thay vì phải ghép các bộ rời rạc.

## Bắt đầu từ đâu

1. `docs/00-repo-map.md`
2. `docs/01-business-overview.md`
3. `docs/02-runtime-flow.md`
4. `docs/03-constraints-and-invariants.md`
5. `docs/12-agent-handoff.md`
6. `contracts/ferrumgate-agent-contract.v1.yaml`
7. `prompts/agent_system.md`

## Trạng thái hiện tại

Đây là **project skeleton + starter code**, chưa phải implementation hoàn chỉnh end-to-end.
Nó được thiết kế để:
- làm nền cho `cargo check` / `cargo test` sau khi hoàn thiện tiếp
- làm contract source of truth cho agents
- làm repo thống nhất để triển khai các crate tiếp theo

## Những phần đã có
- `ferrum-proto`
- `ferrum-pdp`
- `ferrum-cap`
- `ferrum-rollback`
- `ferrum-gateway`
- docs + contracts + openapi + schemas + prompts

## Những phần mới được thêm vào trong bản unified
- repo hygiene files
- configs
- scripts
- `.github/workflows`
- `ferrum-firewall`, `ferrum-store`, `ferrum-graph`, `ferrum-ledger`, adapter crates, `ferrum-testkit`, `ferrumctl` skeleton
- diagrams
- roadmap / ADR / delivery plan / handoff docs

## Điểm vào cho agent khác

- `docs/implementation-path/README.md`
- `docs/implementation-path/00-start-here.md`
