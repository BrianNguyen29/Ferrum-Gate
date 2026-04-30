# 00 — Project canon

> **⚠️ Historical / Superseded**: This document reflects pre-P6/P7 planning-era descriptions. It predates the v1 single-node support contract finalization and the P6/P7 validation pass. Do not use this as an authoritative reference for v1 scope or feature status.
>
> **For current state**: See `docs/implementation-path/01-current-state.md`, `./19-v1-single-node-support-contract.md`, and `docs/implementation-path/32-feature-completeness-audit.md`.
>
> **Canonical reference**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

Tài liệu này là **nguồn mô tả trung tâm** của FerrumGate.

## 1. FerrumGate là gì

FerrumGate là một control plane đứng giữa:
- user / operator
- agent runtime
- MCP tools / adapters
- audit / rollback infrastructure

để ép mọi hành động có side effect đi qua các bước kiểm soát chuẩn.

## 2. Product thesis

FerrumGate phải là:
- intent-first
- capability-scoped
- provenance-aware
- rollback-by-default
- agent-followable
- triển khai được như control plane / sidecar / service

## 3. Bài toán nó giải

Các agent/tool runtimes hiện nay thường có các điểm yếu:
- quyền quá rộng theo session
- scope drift
- prompt/tool output poisoning
- plugin/tool trust boundary yếu
- thiếu transactional semantics cho side effects
- khó audit vì thiếu lineage
- khó recover khi action sai

FerrumGate giải bài toán **execution governance** chứ không phải thay thế agent.

## 4. Phạm vi v1 và support contract

### Supported — single-node governance core with SQLite-backed persistence

> **Canonical reference**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

FerrumGate v1 Supported scope:
- evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate flow (single-node, SQLite)
  - Note: commit and rollback routes are not exposed in the v1 router; compensate is the provided recovery endpoint
- approvals queries (GET /v1/approvals, GET /v1/approvals/{id})
- provenance lineage/query APIs (GET /v1/provenance/lineage/{id}, POST /v1/provenance/lineage, POST /v1/provenance/query)
- Trust labels, taint scoring, scope-bounds enforcement
- R0/R1/R2/R3 rollback contract classes with auto_commit semantics

### Partial — adapter surfaces (crate/API shape only, not production-verified side-effect integrations)

> **⚠️ Note**: As of P6/P7 validation, verified local slices exist for all five adapter crates. The characterization below as "skeleton (no real implementation)" is stale. See `docs/implementation-path/01-current-state.md` for current adapter test counts and verified surface areas.

- `ferrum-adapter-fs` — filesystem adapter (verified local slice: 146 tests)
- `ferrum-adapter-sqlite` — SQLite adapter (verified local slice: 16 tests)
- `ferrum-adapter-maildraft` — maildraft adapter (verified local slice: 16 tests)
- `ferrum-adapter-git` — git adapter (verified local slice: 86 tests)
- `ferrum-adapter-http` — HTTP adapter (verified local slice: 103 tests)

### Deferred / post-v1

- real adapter implementations beyond verified local slices (permissions/symlink/cross-fs for fs; remote push/pull/submodule for git; broader replay/idempotency for http)
- multi-node / HA / read-replica
- PostgreSQL (Phase 3 path — recommended for production scale)
- U1-U4 upgrade tracks (Outcome-aware Governance, Reversible Execution Planner, Cross-runtime Provenance Fabric, MCP/local/NemoClaw runtime integrations) — all implemented but explicitly out-of-v1-contract per support contract
- ledger hash chain (beyond current bounded SHA-256 chain)

### Not supported

- claiming distributed deployment or production external integrations via adapter skeletons
- GUI computer-use, full PKG, multi-tenant SaaS complete deployment
- sandbox/OS isolation at kernel level

---

## 5. Bốn trụ không được phá

### 5.1 Intent
Không có mutating execution nào hợp lệ nếu chưa có intent rõ ràng.

### 5.2 Capability
Không cấp quyền rộng theo session; chỉ cấp quyền hẹp, ngắn hạn, single-use.

### 5.3 Provenance
Mọi side effect meaningful phải có lineage đủ để truy nguyên.

### 5.4 Rollback
Mọi mutation đáng kể phải có recovery path phù hợp.

## 6. Luật cứng

- Không bypass gateway cho mutation
- Không reuse capability
- Không auto-commit action R3
- Không bỏ provenance chain
- Không trả raw internal control data ra user plane
- Không mở rộng scope ngoài intent
