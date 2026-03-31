# 00 — Project canon

Tai lieu nay la **nguon mo ta trung tam** cua FerrumGate.

## 1. FerrumGate la gi

FerrumGate la mot control plane dung giua:
- user / operator
- agent runtime
- MCP tools / adapters
- audit / rollback infrastructure

de ep moi hanh dong co side effect di qua cac buoc kiem soat chuan.

## 2. Product thesis

FerrumGate phai la:
- intent-first
- capability-scoped
- provenance-aware
- rollback-by-default
- agent-followable
- trien khai duoc nhu control plane / sidecar / service

## 3. Bai toan no giai

Cac agent/tool runtimes hien nay thuong co cac diem yeu:
- quyen qua rong theo session
- scope drift
- prompt/tool output poisoning
- plugin/tool trust boundary yeu
- thieu transactional semantics cho side effects
- kho audit vi thieu lineage
- kho recover khi action sai

FerrumGate giai bai toan **execution governance** chu khong phai thay the agent.

## 4. Pham vi v1 va support contract

### Supported — single-node governance core with SQLite-backed persistence

> **Canonical reference**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

FerrumGate v1 Supported scope:
- evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate flow (single-node, SQLite)
  - Compensate is the primary recovery endpoint; commit and rollback routes are also exposed in the v1 router
- approvals queries (GET /v1/approvals, GET /v1/approvals/{id})
- provenance lineage/query APIs (GET /v1/provenance/lineage/{id}, POST /v1/provenance/lineage, POST /v1/provenance/query)
- Trust labels, taint scoring, scope-bounds enforcement
- R0/R1/R2/R3 rollback contract classes with auto_commit semantics

### Partial — adapter surfaces (crate/API shape only, not production-verified side-effect integrations)

- `ferrum-adapter-fs` — filesystem adapter skeleton (no real implementation)
- `ferrum-adapter-sqlite` — SQLite adapter skeleton (no real implementation)
- `ferrum-adapter-maildraft` — maildraft adapter skeleton (no real implementation)
- `ferrum-adapter-git` — git adapter skeleton (no real implementation)
- `ferrum-adapter-http` — HTTP adapter skeleton (no real implementation)

### Deferred / post-v1

- real adapter implementations (fs, sqlite, maildraft, git, http)
- multi-node / HA / read-replica
- U1-U4 upgrade tracks (Outcome-aware Governance, Reversible Execution Planner, Cross-runtime Provenance Fabric, MCP/local/NemoClaw runtime integrations)

### Not supported

- claiming distributed deployment or production external integrations via adapter skeletons
- GUI computer-use, full PKG, multi-tenant SaaS complete deployment
- sandbox/OS isolation at kernel level

---

## 5. Bon tru khong duoc pha

### 5.1 Intent
Khong co mutating execution nao hop le neu chua co intent ro rang.

### 5.2 Capability
Khong cap quyen rong theo session; chi cap quyen hep, ngan han, single-use.

### 5.3 Provenance
Moi side effect meaningful phai co lineage du de truy nguon.

### 5.4 Rollback
Moi mutation dang ke phai co recovery path phu hop.

## 6. Luat cung

- Khong bypass gateway cho mutation
- Khong reuse capability
- Khong auto-commit action R3
- Khong bo provenance chain
- Khong tra raw internal control data ra user plane
- Khong mo rong scope ngoai intent
