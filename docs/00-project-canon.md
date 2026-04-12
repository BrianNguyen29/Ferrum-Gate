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

### Production-ready status (2026-04-08)

FerrumGate v1 single-node is now **broader production-ready** with an explicit
scope boundary:

- **T1**: production-supported
- **T2**: partial, but hardened to the partial contract level
- **T3**: deferred / out of scope

This declaration does not claim multi-node / HA readiness, broadly production-
verified external adapter integrations, or completion of the U2-U4 upgrade
tracks.

### v2 planning status

FerrumGate v2 single-node is **planned / proposed**, not yet ratified. Draft docs
exist at:
- `docs/20-v2-single-node-production-support-contract.md` — proposed v2 scope
- `docs/implementation-path/44-v2-production-execution-plan.md` — proposed v2 execution plan
- `docs/implementation-path/45-v2-adapter-promotion-criteria.md` — **DRAFT** concrete T2→T1 promotion gates per adapter (fs/sqlite/git/http; maildraft T2-only)

The v2 docs describe a **target state** for single-node production support,
predicated on successful completion of the v2 execution plan. v1 remains the
authoritative support contract until v2 is formally ratified.

### Supported — single-node governance core with SQLite-backed persistence

> **Canonical reference**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

FerrumGate v1 Supported scope:
- evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate flow (single-node, SQLite)
  - Compensate is the primary recovery endpoint; commit and rollback routes are also exposed in the v1 router
- approvals queries (GET /v1/approvals, GET /v1/approvals/{id})
- provenance lineage/query APIs (GET /v1/provenance/lineage/{id}, POST /v1/provenance/lineage, POST /v1/provenance/query)
- Trust labels, taint scoring, scope-bounds enforcement
- R0/R1/R2/R3 rollback contract classes with auto_commit semantics

### Partial — adapter surfaces (bounded local implementations hardened to the partial contract level, not promoted to full production-verified external integrations)

- `ferrum-adapter-fs` — filesystem adapter (local file write/delete with hash-based verify; broader external/integration hardening deferred)
- `ferrum-adapter-sqlite` — SQLite adapter (bounded local row mutation rollback, including atomic multi-row support)
- `ferrum-adapter-maildraft` — maildraft adapter (SQLite-backed draft persistence and verify semantics; send/provider integration deferred)
- `ferrum-adapter-git` — git adapter (local HEAD capture/reset and branch-create rollback; broader remote/external workflows deferred)
- `ferrum-adapter-http` — HTTP adapter (bounded HTTP execute/verify with body-aware digest, header-shape binding, canonical query strings, auth support, and conservative rollback no-op; mutation recovery is R3 boundary)

### Deferred / post-v1

- broader production-verified external adapter integrations and hardening beyond the T2 partial contract (fs, sqlite, maildraft, git, http)
- policy bundle lifecycle tooling
- multi-node / HA / read-replica
- deeper U1-U4 upgrade-track work (Outcome-aware Governance, Reversible Execution Planner, Cross-runtime Provenance Fabric, MCP/local/NemoClaw runtime integrations)

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
