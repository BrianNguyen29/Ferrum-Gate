# FerrumGate Implementation Roadmap Pack

## Mục tiêu gói tài liệu

Bộ tài liệu này chuyển roadmap chiến lược của FerrumGate thành kế hoạch triển khai có thể thực thi bởi AI agents hoặc engineer theo từng quý, từng crate, từng API, từng adapter và từng mốc release.

Gói này **không thay thế** project canon. Nó là lớp triển khai nằm trên bộ tài liệu gốc.

## Phạm vi

- Bám vào trạng thái hiện tại của FerrumGate v1 single-node
- Không giả định các tính năng chưa có là đã production-ready
- Ưu tiên đường đi thực dụng: từ governance kernel -> governed engineering changes -> self-hosted product -> MCP/open runtime governance -> enterprise evidence plane

## Trật tự đọc đề xuất

1. `00-roadmap-charter.md`
2. `01-quarterly-plan.md`
3. `02-release-plan.md`
4. `03-crate-workplan.md`
5. `04-api-roadmap.md`
6. `05-adapter-roadmap.md`
7. `06-testing-and-quality-gates.md`
8. `07-operator-and-deployment-plan.md`
9. `08-agent-execution-rules.md`
10. `09-backlog-and-deferred-tracks.md`
11. `10-master-checklist.md`
12. `11-current-state-baseline.md` ← baseline facts about the repo at time of writing
13. `12-doc-governance-and-status-tags.md` ← rules for how roadmap docs are maintained and tagged
14. `13-q1-work-packages.md` ← execution-ready work packages for Q1 kernel hardening
15. `14-q2-work-packages.md` ← execution-ready work packages for Q2 adapter beta
16. `15-q1-q2-evidence-workflow.md` ← evidence recording workflow, naming conventions, note template, and gate evidence checklist

Reading order for new contributors or agents:
- First read `11-current-state-baseline.md` to understand what exists today
- Then read `00-roadmap-charter.md` to understand where the project is headed
- Then proceed through 01–10 for execution planning
- `12-doc-governance-and-status-tags.md` is a living reference for anyone editing roadmap docs
- `15-q1-q2-evidence-workflow.md` is the evidence workflow reference for Q1/Q2 gate passes; read this before recording any gate evidence

## Relationship between roadmap docs and v1 support contract

**The v1 single-node support contract (`19-v1-single-node-support-contract.md`) is the canon
boundary for FerrumGate v1.** It defines what is and is not supported in the v1 release.

This roadmap pack (`00`–`12`) describes **planning work layered on top of the v1 support baseline**.
It includes v1 hardening work plus post-v1 roadmap work, but it does not modify the v1 support contract.

- Implemented code beyond the v1 support baseline may exist in the repo
  (e.g., adapter crate shapes, routes not in the v1 router, CLI commands marked post-v1 scope).
  The mere existence of such code does not expand the v1 support contract.
- The v1 support contract is the **only** authoritative reference for what is supported in v1.
- Roadmap docs in this pack describe planned work that is **post-v1 scope** unless
  explicitly gated by the v1 support contract or a later formal amendment to it.
- Every item in `01`–`10` is a plan item, not a claim of current support.

## Cách dùng gói này với AI agents

- Mỗi agent chỉ nên nhận một work package nhỏ
- Mọi thay đổi schema/API/invariants phải cập nhật đồng thời code + docs + contracts + schemas + openapi
- Mỗi PR hoặc task phải tham chiếu đúng quarter, release, crate và API milestone trong pack này
- Không triển khai feature ngoài release scope nếu chưa có thay đổi chính thức ở `02-release-plan.md`
- **Critical**: when working in the v1 support scope, always check `19-v1-single-node-support-contract.md`
  before making any claim about what is or is not supported

## Canonical v1 roadmap cross-reference

For canonical v1 implementation status and release/pilot routing, see
[`../../ferrumgate-roadmap-v1/09-implementation-path.md`](../../ferrumgate-roadmap-v1/09-implementation-path.md)
and `../../implementation-path/01-current-state.md`. This roadmap-v2 pack remains a planning
reference and does not override the v1 support contract.

## Relationship to `docs/production-readiness-v2/`

The `docs/production-readiness-v2/` doc pack is the **current active post-pilot
execution and evidence planning layer**. It supplements—but does not
supersede—this roadmap-v2 pack.

- This roadmap-v2 pack remains a **historical/baseline planning reference**.
- `docs/production-readiness-v2/` contains the scoped post-pilot production path
  (SLO/SLA, PostgreSQL hardening, target-host MCP validation, security/tenant
  ADR) and is anchored to the canonical [`docs/ROADMAP.md`](../../ROADMAP.md).
- No doc in either pack claims production-ready status.
