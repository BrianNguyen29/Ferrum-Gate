# 00 — Project canon

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

## 4. Phạm vi v1

FerrumGate v1 hỗ trợ:
- MCP tools
- filesystem workspace
- Git local
- SQLite
- HTTP allowlist
- email draft-only
- policy decisions
- capability leasing
- provenance
- rollback / compensation

FerrumGate v1 chưa nhằm tới:
- GUI computer-use
- full PKG
- multi-tenant SaaS hoàn chỉnh
- full distributed deployment
- sandbox/OS isolation cấp kernel

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
