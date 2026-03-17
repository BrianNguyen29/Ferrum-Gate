# 02 — Project overview

## 1. Mục tiêu sản phẩm

FerrumGate là lớp execution governance cho agentic systems.

Nó phải giúp hệ thống:
- chỉ cho phép action hợp lệ theo intent
- giảm drift khỏi intent
- audit được mọi quyết định quan trọng
- recover được khi action sai

## 2. Đối tượng sử dụng

### Agent runtime
Dùng FerrumGate như lớp xin quyền và thực thi an toàn hơn.

### Operator
Dùng FerrumGate để:
- xem approval
- xem lineage
- rollback / compensate
- audit decisions

### Engineer / integrator
Dùng FerrumGate như:
- control plane
- gateway
- governance sidecar
- repo nền để build implementation thật

## 3. Khác gì với một gateway thông thường

FerrumGate không chỉ routing request.
Nó gộp:
- intent boundary
- policy boundary
- capability boundary
- rollback boundary
- provenance boundary

## 4. Một câu mô tả cuối cùng

FerrumGate là một **intent-scoped reversible execution plane** cho MCP/tool-using agents.
