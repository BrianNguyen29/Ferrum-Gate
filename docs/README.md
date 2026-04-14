# FerrumGate Docs

Đây là **thư mục docs duy nhất** dùng làm nền triển khai cho dự án FerrumGate.

Mục tiêu của bộ docs này:
- mô tả đầy đủ, nhất quán và rõ ràng về dự án
- giúp AI agents hoặc engineers khác có thể bám vào để triển khai
- tránh phải tra cứu rời rạc giữa nhiều file/spec khác nhau

**H1 completion snapshot:** 10/17 slices done (H1.1a, H1.1b, H1.1c, H1.1d, H1.2a, H1.2b, H1.3a, H1.4b, H1.4c, H1.5a) ✅ 2026-04-13; remaining H1.3b–H1.3c, H1.4a, H1.4d–H1.4e, H1.5b–H1.5c ⬜ PLANNED — full detail in `50-post-v2-roadmap.md`

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
- `18-phase-f-evidence-pack.md` — consolidated Phase F evidence pack (supported flows, poisoned-context status, open gaps, handoff readiness)
- `90-docs-governance.md` — **documentation governance policy** (canonical hierarchy, doc family inventory, ownership, review cadence, deprecation/archival policy) — see this doc for all governance questions
- `implementation-path/41-production-execution-plan.md` — sequential production evaluation plan (G-E1 → G-E5), per-phase doc update protocol, and commit/PR merge cadence
- `implementation-path/44-v2-production-execution-plan.md` — v2 production scope and execution plan (Phase 1–6)
- `implementation-path/45-v2-adapter-promotion-criteria.md` — **✅ RATIFIED** concrete per-adapter T2→T1 promotion gates (fs/sqlite/git/http; maildraft T2-only)
- `implementation-path/50-post-v2-roadmap.md` — post-v2 backlog structured as Horizons H1/H2/H3 (policy tooling, U1 backlog, HA, U2/U3/U4)
- `implementation-path/60-long-term-vision.md` — **non-binding** long-term strategic intent; maps future planes to FerrumGate invariants; distinct from contract (v1/v2) and roadmap (30/50) docs
- `implementation-path/42-p2-performance-baseline-evidence.md` — in-repo G-E2 benchmark baseline evidence for SQLite/store and adapter paths under concurrent load
- `implementation-path/43-production-readiness-signoff.md` — G-E5 sign-off declaring broader production-ready with explicit T1/T2/T3 scope boundaries (v1 ✅ RATIFIED)
- `implementation-path/46-v2-readiness-signoff.md` — **✅ RATIFIED** v2 sign-off artifact
- `runbooks/` — operator runbooks for specific production scenarios

## Thư mục con
- .agents - các kĩ thuật chuyên biệt về rust, hãy luôn sử dụng trong quá trình làm việc, và yêu cầu các sub-agents cũng sử dụng khi cần
- `implementation-path/` — lộ trình triển khai cực cụ thể cho agent khác
- `diagrams/` — sơ đồ trực quan về kiến trúc, flow, state machine, constraints
- `artifacts/2026-04-09/` — fs-first beta slice evidence bundle (before_hash/after_hash wiring closure)

## Fast Status Index

For a quick orientation on current production state, start here:

| Topic | File | What it tells you |
|-------|------|-------------------|
| Support contract (T1/T2/T3) | `19-v1-single-node-support-contract.md` | What's supported, partially supported, out-of-scope (v1) |
| Support contract (T1/T2/T3) | `20-v2-single-node-production-support-contract.md` | **✅ RATIFIED** — canonical v2 single-node support contract |
| Production sign-off (v1) | `implementation-path/43-production-readiness-signoff.md` | G-E5 ✅ DONE — broader production-ready declaration |
| v2 sign-off | `implementation-path/46-v2-readiness-signoff.md` | **✅ RATIFIED** — v2 production-ready declared |
| v2 execution plan | `implementation-path/44-v2-production-execution-plan.md` | **✅ RATIFIED** — all phases complete |
| v2 adapter promotion criteria | `implementation-path/45-v2-adapter-promotion-criteria.md` | **✅ RATIFIED** — concrete T2→T1 gates confirmed |
| Current state | `implementation-path/01-current-state.md` | Where the project stands now |
| Remaining tasks | `implementation-path/11-remaining-tasks.md` | P0/P1/P2 done; P3 post-v1 backlog now indexed to roadmap doc |
| Production roadmap | `implementation-path/30-production-roadmap.md` | Priority 1–6 tracks, all P2 slices ✅ DONE |
| H1 shipped summary | `implementation-path/01-current-state.md` | H1 sub-slices shipped post-v2 ratification (H1.1a, H1.1b, H1.1c, H1.1d, H1.2a, H1.2b, H1.3a, H1.4b, H1.4c, H1.5a); remaining H1.3b–H1.3c, H1.4a, H1.4d–H1.4e, H1.5b–H1.5c ⬜ PLANNED |
| Post-v2 roadmap (H1/H2/H3) | `implementation-path/50-post-v2-roadmap.md` | Structured backlog: near-term H1, next-capability H2, long-term H3 |
| fs-first artifact | `artifacts/2026-04-09/closure-note.txt` | Narrow PR #165 evidence: fs before_hash/after_hash wiring closed |

## Source-of-Truth Priority

When content conflicts, resolve in this order:

1. `00-project-canon.md` — project definition and intent
2. `06-constraints-and-invariants.md` — invariant specification
3. `09-implementation-path.md` — build plan and phasing
4. `10-crate-by-crate-plan.md` — crate decomposition
5. remaining `docs/` — derivative and reference material

## Kết luận ngắn

FerrumGate là một **intent-scoped reversible execution plane** cho MCP/tool-using agents.
Mọi action có side effect phải đi qua:
- intent
- policy
- capability
- provenance
- rollback
