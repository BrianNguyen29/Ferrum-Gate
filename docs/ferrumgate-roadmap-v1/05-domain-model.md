# 05 — Domain model

## 1. IntentEnvelope

Biểu diễn chuẩn hóa của ý định.

Phải mô tả:
- goal
- normalized goal
- allowed outcomes
- forbidden outcomes
- resource scope
- risk tier
- approval mode
- rollback class mặc định
- trust context

## 2. ActionProposal

Đề xuất action cụ thể của agent.

Phải mô tả:
- tool
- args
- expected effect
- requested rollback class
- taint inputs
- step index

## 3. CapabilityLease

Quyền cực hẹp, ngắn hạn, single-use.

Phải mô tả:
- tool binding
- resource bindings
- argument constraints
- taint budget
- approval binding (nếu có)
- TTL
- status

## 4. RollbackContract

Hợp đồng recovery cho side effect.

Phải mô tả:
- action type
- rollback class
- adapter key
- target
- prepare checks
- verify checks
- compensation plan
- auto_commit
- state

## 5. ProvenanceEvent

Event lineage.

Phải mô tả:
- kind
- actor
- object
- occurred_at
- related ids
- trust/sensitivity labels
- parent edges
- hash chain refs nếu có

## 6. ApprovalRequest

Request duyệt cho action nhạy cảm.

Phải bind với:
- action digest
- expiry
- actor/requested_by
- state
