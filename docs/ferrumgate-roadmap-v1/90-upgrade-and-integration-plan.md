# 90 — Upgrade and Integration Plan

> Tài liệu này mô tả kế hoạch **nâng cấp** và **tích hợp** cho FerrumGate sau khi hoàn thành MVP, theo hướng giữ nguyên 4 trụ cốt lõi:
> **Intent → Capability → Provenance → Rollback**.
>
> Tài liệu này **không thay thế** các docs hiện có như `09-implementation-path.md` hay `10-crate-by-crate-plan.md`.
> Nó chỉ tập trung vào câu hỏi: **khi nền tảng đã chạy được, nâng cấp theo hướng nào để tạo khác biệt, tích hợp với ai, và rollout ra sao**.

---

# 1. Mục tiêu của kế hoạch nâng cấp và tích hợp

FerrumGate sau khi hoàn thành bản nền tảng không nên dừng ở mức “control plane nội bộ chạy được”.
Giá trị dài hạn của dự án nằm ở việc trở thành một **execution governance layer có thể đứng trên nhiều agent runtimes**, không khóa vào một vendor hoặc một runtime duy nhất.

Kế hoạch nâng cấp và tích hợp có 5 mục tiêu:

1. **Giữ vững lõi FerrumGate**
   - intent-scoped execution
   - single-use capability
   - provenance-first lineage
   - rollback-by-default cho side effects

2. **Mở rộng bề mặt tích hợp**
   - MCP runtimes
   - local tool runtimes
   - NemoClaw / OpenShell path
   - enterprise services nội bộ

3. **Tăng tính deployable**
   - local dev
   - staging
   - production-like
   - multi-runtime environment

4. **Tạo “chất riêng”**
   - outcome-aware governance
   - reversible execution planner
   - cross-runtime provenance fabric
   - policy packs theo use case

5. **Giảm rủi ro drift**
   - không để dự án trôi thành “một gateway nữa”
   - không để dự án biến thành “wrapper cho một vendor cụ thể”

---

# 2. Nguyên tắc nâng cấp

## 2.1 Không phá 4 trụ lõi

Mọi nâng cấp hoặc tích hợp đều phải giữ nguyên:

- **Intent** là điểm bắt đầu bắt buộc của execution
- **Capability** là quyền cực hẹp, không suy từ session
- **Provenance** là bắt buộc cho mọi side effect meaningful
- **Rollback** là primitive, không phải tính năng phụ

## 2.2 Không đổi định vị theo hướng quá hẹp

FerrumGate không nên bị định vị thành:

- chỉ là “bảo mật cho OpenClaw”
- chỉ là “MCP gateway”
- chỉ là “sandbox wrapper”
- chỉ là “policy engine”

FerrumGate nên giữ định vị là:

> **Vendor-neutral execution governance and reversible control plane for agent systems**

## 2.3 Tích hợp phải tách lớp

Mọi integration nên nằm ở lớp riêng, không nhét trực tiếp vào core crates nếu không cần thiết.

Nghĩa là:
- `ferrum-proto`, `ferrum-pdp`, `ferrum-cap`, `ferrum-rollback` vẫn là lõi
- phần “gắn với một runtime/vendor” nên đi vào `integrations` hoặc adapter layer

## 2.4 Mọi tích hợp phải có lineage

Nếu tích hợp với hệ khác mà không đưa được event của nó vào provenance chain, thì tích hợp đó chỉ mới là “kết nối kỹ thuật”, chưa đủ là tích hợp đúng chuẩn FerrumGate.

---

# 3. Lộ trình nâng cấp sau MVP

## 3.1 Giai đoạn U1 — Ổn định lõi

Mục tiêu:
- hoàn thiện workspace compile sạch
- hoàn thiện happy path end-to-end
- có 3 adapters usable đầu tiên:
  - filesystem
  - sqlite
  - maildraft

Nâng cấp ở giai đoạn này tập trung vào:
- state transitions rõ ràng hơn
- store/persistence thực
- lineage query thực
- rollback path có test

Không nên làm ở U1:
- mở rộng vendor integrations quá sớm
- thêm nhiều policy packs phức tạp
- thêm quá nhiều adapters cùng lúc

## 3.2 Giai đoạn U2 — Tăng tính sử dụng được

Mục tiêu:
- FerrumGate usable như một control plane nội bộ

Nâng cấp nên thêm:
- `ferrumctl` hữu dụng hơn
- lineage explorer tối thiểu
- approval UX tối thiểu
- richer firewall rule-based logic
- richer policy matchers

Kết quả mong đợi:
- operator có thể debug được
- dev có thể chạy smoke paths rõ ràng
- agent khác có thể bám vào docs mà triển khai tiếp

## 3.3 Giai đoạn U3 — Outcome-aware governance

Đây là nâng cấp quan trọng để FerrumGate có “chất riêng”.

Nên thêm khái niệm:

### Outcome Contract
Là object hoặc layer nằm trên `IntentEnvelope`, mô tả:
- outcome nào là hợp lệ
- bằng chứng hoàn thành là gì
- dấu hiệu drift là gì
- terminal paths nào được phép
- khi nào cần approval/rollback escalation

Giá trị:
- FerrumGate không chỉ authorize tool call
- FerrumGate authorize **quỹ đạo hoàn thành tác vụ**

## 3.4 Giai đoạn U4 — Reversible Execution Planner

Nâng cấp tiếp theo rất đáng giá là planner cho rollback/recovery.

Planner này nên sinh:
- preconditions
- verify checks
- compensation steps
- stop points cần approval
- recovery strategy theo rollback class

Giá trị:
- rollback không còn là danh sách tĩnh
- hệ có thể chủ động đề xuất đường recover tốt hơn

## 3.5 Giai đoạn U5 — Cross-runtime provenance fabric

Nâng cấp này giúp FerrumGate khác rõ với runtime guardrails thông thường.

Mục tiêu:
- gom event từ nhiều runtime/sandbox/tool into one lineage
- query lineage theo execution chứ không theo hệ con riêng lẻ

Ví dụ:
- user goal ở FerrumGate
- tool execution ở MCP runtime
- sandbox event từ runtime khác
- approval event từ operator
- compensation event từ adapter

Tất cả phải quy về cùng một execution trail.

---

# 4. Kế hoạch tích hợp

## 4.1 Nhóm tích hợp ưu tiên cao

### A. MCP integrations
Đây là nhóm ưu tiên đầu tiên vì FerrumGate sinh ra để đứng ở agent-tool boundary.

Nên có:
- `ferrum-integrations-mcp`
- mapping từ tool call -> `ActionProposal`
- mapping từ tool metadata / tool output -> trust labels
- mapping result -> provenance events

### B. Local runtime integrations
Nhóm này phục vụ:
- CLI tools
- scripts nội bộ
- local daemons
- workspace automation

Nên có:
- `ferrum-integrations-local`
- wrapper để mọi local action vẫn đi qua policy/capability/rollback

### C. NemoClaw / OpenShell integrations
Nếu muốn giữ dự án khác biệt trước NVIDIA/OpenClaw narrative, tích hợp đúng phải theo hướng:

- NemoClaw/OpenShell lo runtime security / sandbox / egress controls
- FerrumGate lo intent/capability/provenance/rollback

Nên thêm:
- `ferrum-integrations-nemoclaw`
- mapping sandbox/egress/approval events -> provenance
- mapping runtime policy signals -> FerrumGate decision context

Điều này biến FerrumGate thành **lớp governance nằm trên runtime security**, thay vì đối đầu trực diện với nó.

## 4.2 Nhóm tích hợp ưu tiên trung bình

### A. Git workflows
- local repo mutation
- before_ref / after_ref capture
- revert/reset semantics

### B. SQLite-first data workflows
- transaction mutation
- predicate verification
- rollback transaction
- safe reporting/update workflows

### C. Mail drafting workflows
- create draft
- verify draft
- delete draft
- no-send hard rule trong v1

## 4.3 Nhóm tích hợp ưu tiên thấp hơn

### A. HTTP enterprise service integrations
Chỉ nên làm sau khi:
- allowlist rõ
- idempotency key strategy rõ
- R3 handling đủ chặt

### B. Approval backends
- Slack/Teams/email approval flows
- chỉ nên thêm sau khi approval object trong lõi ổn định

---

# 5. Đề xuất thay đổi cấu trúc repo để hỗ trợ tích hợp

Hiện cấu trúc lõi có thể giữ gần như nguyên vẹn.
Phần nên thêm là một lớp integrations tách biệt.

## 5.1 Cấu trúc khuyến nghị

```text
crates/
  ferrum-proto
  ferrum-pdp
  ferrum-cap
  ferrum-firewall
  ferrum-rollback
  ferrum-gateway
  ferrum-store
  ferrum-graph
  ferrum-ledger
  ferrum-adapter-fs
  ferrum-adapter-git
  ferrum-adapter-sqlite
  ferrum-adapter-http
  ferrum-adapter-maildraft
  ferrum-integrations-mcp
  ferrum-integrations-local
  ferrum-integrations-nemoclaw
```

## 5.2 Vì sao cần tách `integrations/*`

Nếu không tách:
- core bị khóa vào vendor/runtime cụ thể
- khó giữ project định vị vendor-neutral
- khó test boundaries độc lập
- docs sẽ bị lẫn giữa core semantics và runtime-specific details

---

# 6. Kế hoạch nâng cấp theo từng capability của hệ

## 6.1 Intent layer upgrades

Nâng cấp nên có:
- richer outcome normalization
- ambiguity markers
- intent revision history
- outcome contract binding

Không nên làm quá sớm:
- semantic intent mining quá nặng
- PKG sâu ngay trong v1/v2

## 6.2 Policy layer upgrades

Nâng cấp nên có:
- richer rule matchers
- policy packs theo domain
- explicit deny/quarantine reasons
- decision explanation quality cao hơn

Ví dụ policy packs:
- coding agent pack
- document drafting pack
- finance/reporting pack
- email triage pack

## 6.3 Capability layer upgrades

Nâng cấp nên có:
- stronger arg constraints
- delegated approval binding
- scoped leases theo workflow step
- revocation audit trails

## 6.4 Provenance layer upgrades

Nâng cấp nên có:
- graph query rõ hơn
- lineage explorer
- replay tooling
- audit views theo intent/execution/capability

## 6.5 Rollback layer upgrades

Nâng cấp nên có:
- reversible execution planner
- compensation catalog
- adapter capability matrix
- recovery confidence scoring

---

# 7. Kế hoạch rollout tích hợp

## 7.1 Rollout strategy

### Stage 1 — Internal only
Dùng cho:
- local dev
- simulation
- filesystem/sqlite/maildraft safe paths

### Stage 2 — Controlled integration
Bật:
- gateway happy path
- policy + provenance + rollback trên vài use case cố định

### Stage 3 — Cross-runtime integration
Thêm:
- MCP runtime integration
- NemoClaw/OpenShell integration
- lineage merge qua nhiều runtime

### Stage 4 — Domain packs
Thêm:
- policy packs
- adapter packs
- outcome contracts theo use case

## 7.2 Không rollout kiểu “big bang”
Không nên:
- bật tất cả adapters cùng lúc
- tích hợp nhiều runtime cùng lúc
- mở rộng approval flows trước khi lineage ổn

---

# 8. Kế hoạch tài liệu đi kèm khi nâng cấp

Mỗi lần nâng cấp lớn phải update tối thiểu:

- docs kiến trúc
- docs implementation path
- adapter contracts
- testing strategy
- release checklist
- troubleshooting

Nếu thêm integration mới, nên có:
- một file riêng trong docs mô tả integration
- input/output/events mapping
- risk model
- done criteria
- known limitations

Ví dụ:
- `docs/18-mcp-integration.md`
- `docs/19-nemoclaw-integration.md`

---

# 9. Rủi ro nếu nâng cấp sai hướng

## 9.1 Bị kéo thành wrapper cho vendor
Nếu tích hợp NemoClaw/OpenShell mà nhét thẳng vào core, FerrumGate sẽ mất tính độc lập.

## 9.2 Bị trôi thành gateway chung chung
Nếu chỉ thêm routing/logging mà không tăng giá trị ở outcome/provenance/rollback, FerrumGate sẽ mất bản sắc.

## 9.3 Bị quá tải scope
Nếu thêm quá nhiều adapters/integrations cùng lúc, repo sẽ khó hoàn thiện.

## 9.4 Drift giữa docs và code
Càng nhiều integration, nguy cơ docs/spec drift càng cao.

---

# 10. Kết luận

Khi dự án hoàn thành bản nền tảng, hướng nâng cấp đúng không phải là “thêm nhiều tool hơn”, mà là:

1. giữ lõi governance thật chắc  
2. thêm outcome-aware execution  
3. thêm reversible planning  
4. thêm cross-runtime provenance  
5. tích hợp với runtimes khác theo lớp riêng  

Nếu làm đúng, FerrumGate sẽ có định vị rất rõ:

> Không phải runtime security product thuần túy.  
> Không phải agent framework.  
> Mà là **execution governance layer** có thể đứng trên nhiều runtimes, kể cả các hệ như MCP hoặc NemoClaw/OpenShell.
