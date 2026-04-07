# 04 — Runtime flow

> **Role**: End-to-end execution sequencing and state flow. Walks through the 10-step runtime path from user goal to terminal state. For component definitions, see [`03-architecture.md`](./03-architecture.md). For per-adapter execute/verify/rollback contract details, see [`13-adapter-contracts.md`](./13-adapter-contracts.md). For decision invariants (Allow/Deny/Quarantine/RequireApproval/AllowDraftOnly), see [`06-constraints-and-invariants.md`](./06-constraints-and-invariants.md).

## Flow chuẩn

### Bước 1 — User goal
Người dùng đưa mục tiêu.

### Bước 2 — Agent proposal
Agent tạo `ActionProposal`.

### Bước 3 — Compile intent
FerrumGate tạo `IntentEnvelope`.

### Bước 4 — Trust / taint labeling
Firewall gắn nhãn và tính rủi ro dữ liệu đầu vào.

### Bước 5 — Policy evaluate
PDP đưa ra decision.

### Bước 6 — Capability mint
Nếu pass, FerrumGate phát `CapabilityLease`.

### Bước 7 — Rollback prepare
Tạo `RollbackContract` và pre-check. Per-adapter rollback mechanics (how each adapter captures state, verifies, and recovers) are defined in [`13-adapter-contracts.md`](./13-adapter-contracts.md).

### Bước 8 — Execute qua gateway
Gateway mới forward sang tool/adapters.

### Bước 9 — Verify + sanitize
Output được sanitize, side effect được verify.

### Bước 10 — Terminal path
Một trong bốn:
- commit
- compensate
- rollback
- quarantine

> Rollback class semantics (R0–R3, auto-commit rules, `EmailSend` as always-R3) are in [`06-constraints-and-invariants.md`](./06-constraints-and-invariants.md). Per-adapter compensation behavior is in [`13-adapter-contracts.md`](./13-adapter-contracts.md).

## Nhánh decision

### Allow
Tiếp tục flow bình thường.

### RequireApproval
Dừng action nhạy cảm và chờ operator duyệt.

### AllowDraftOnly
Ep action về draft mode.

### Deny
Dừng ngay, không được “lách” bằng action tương đương.

### Quarantine
Dừng flow mutation, giữ lại để điều tra / xem xét.

## Minimum lineage chain

1. ActionProposalSubmitted
2. PolicyEvaluated
3. CapabilityMinted
4. ToolCallPrepared
5. ToolCallExecuted
6. SideEffectPrepared
7. SideEffectVerified
8. terminal event
