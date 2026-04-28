# 91 — Phase Success Criteria and KPIs

> ⚠️ Historical / superseded snapshot: this roadmap-v1 KPI document predates the current
> feature-completeness audit and contains stale phase/test-count wording. Treat it as
> historical context only. Current feature truth lives in
> `../implementation-path/01-current-state.md`,
> `../implementation-path/32-feature-completeness-audit.md`, and
> `../implementation-path/33-feature-completion-backlog.md`.

> Tài liệu này định nghĩa **success criteria**, **KPI mục tiêu**, **release gates**, và **evidence cần có** cho từng phase của FerrumGate.
>
> Tài liệu này đã **bao gồm** góc nhìn về kế hoạch nâng cấp và tích hợp: nghĩa là success criteria không chỉ đo “code chạy được”, mà còn đo hệ đã sẵn sàng để nâng cấp/tích hợp theo hướng khác biệt hay chưa.
>
> Tài liệu này **không thay thế** `09-implementation-path.md`, mà là lớp đo lường cụ thể hơn để ra quyết định “phase đã xong chưa”.

## Current progress snapshot (2026-03-29)

### Phase status now

- **Phase A**: DONE — workspace and core shapes stable; `cargo check --workspace` passes; crates scaffolded and building.
- **Phase B**: DONE — SQLite-backed persistence for intents/proposals/capabilities/executions/rollback contracts/provenance/events/approvals confirmed via integration tests.
- **Phase C**: DONE — firewall logic present (trust labels, taint scorer, sanitize, contradiction checks); curated poisoned-context regression fixtures implemented (6 fixture tests).
- **Phase D**: PARTIAL — adapter skeletons exist (fs, sqlite, maildraft, git, http); real implementations are post-v1 backlog. NoopRollbackAdapter used for integration tests.
- **Phase E**: DONE for SQLite-backed single-node flow — gateway orchestrates `evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate` (internal lifecycle: commit/rollback not exposed in v1 router); negative paths: deny, quarantine, RequireApproval (R3), draft-only gated at evaluate (before prepare), scope-mismatch (P0 resolved).
- **Phase F**: DONE — integration tests strong; poisoned-context regression fixtures curated (6 tests); supported flows list and open gaps list documented in `25-v1-single-node-rc-evidence.md`.

### Latest evidence snapshot

- `cargo check --workspace`: workspace compiles successfully.
- Integration test suite (`crates/ferrum-integration-tests/src/integration_gateway_flow.rs`): capability single-use, R3 no-auto-commit, rollback/compensate distinct ops, taint-based quarantine, compensate end-to-end flow, pending-approvals pagination and filter, lineage endpoint shape/validation — all present and passing.
- Lineage integration tests (`tests/integration_lineage_chain.rs`): empty lineage for unknown execution, invalid UUID rejection, correct content-type, max_hops clamping, direction variants — all present.
- Gateway negative-path coverage: deny, quarantine, rollback, compensate, RequireApproval (R3), draft-only gated at evaluate (before prepare), scope-mismatch (P0 resolved).
- `scripts/generate_rc_evidence.py`: exists and PASS with all five checks.

### Working interpretation of release gates

- **Phase A**: complete.
- **Phase B**: complete for SQLite-backed single-node flow.
- **Phase C**: complete — firewall logic present, regression fixtures curated.
- **Phase D**: partial — adapter skeletons present, real implementations are post-v1.
- **Phase E**: complete for SQLite-backed single-node flow (scope-mismatch P0 now resolved).
- **Phase F**: complete for single-node v1 RC — poisoned-context regression fixtures curated, supported flows and open gaps documented, evidence automation script present.

---

# 1. Cách đọc tài liệu này

Mỗi phase gồm 5 phần:

1. **Objective** — mục tiêu của phase  
2. **Success criteria** — điều kiện hoàn thành mang tính chức năng  
3. **KPIs / target metrics** — chỉ số mục tiêu  
4. **Release gate** — điều kiện cho phép chuyển phase  
5. **Evidence** — bằng chứng phải có trong repo  

## 1.1 Lưu ý về KPI

Các KPI ở đây là **target KPIs cho triển khai**, không phải số liệu đã đo sẵn.
Chúng được dùng như ngưỡng mục tiêu để agent/dev biết khi nào có thể coi một phase là “đủ tốt”.

---

# 2. Phase A — Compile and Shape Stability

## 2.1 Objective

Ổn định workspace, shape objects và mối quan hệ giữa:
- code
- contracts
- schemas
- openapi
- docs

## 2.2 Success criteria

Phase A được xem là thành công khi:

- Rust workspace build được ở mức `cargo check --workspace`
- tất cả crate trong workspace khớp members/dependencies
- không còn missing modules/imports obvious
- domain shapes trong `ferrum-proto` không drift rõ ràng khỏi docs/spec
- root repo đủ ổn định để phase sau xây tiếp

## 2.3 KPIs / target metrics

### Build KPIs
- Workspace compile success rate: **100%**
- Missing crate/module blockers: **0**
- Broken cargo member references: **0**

### Consistency KPIs
- Drift giữa `ferrum-proto` và `schemas/`: **0 unresolved critical mismatches**
- Drift giữa `ferrum-proto` và `contracts/`: **0 unresolved critical mismatches**
- Drift giữa `openapi/` và API structs lõi: **0 unresolved critical mismatches**

### Hygiene KPIs
- `cargo fmt --all`: **pass**
- `clippy` critical warnings: **0 blocker warnings**
- repo layout validation: **pass**

## 2.4 Release gate

Chỉ được sang Phase B khi:
- workspace check pass
- root docs vẫn phản ánh reality
- object model lõi ổn định đủ để persistence layer bám vào

## 2.5 Evidence cần có

- CI/log chứng minh `cargo check --workspace` pass
- danh sách crate/members cuối cùng
- ghi chú sync giữa code và schemas/contracts/openapi

## 2.6 Liên hệ với kế hoạch nâng cấp/tích hợp

Nếu Phase A không sạch, mọi integration sau này sẽ rơi vào drift.
Phase A là điều kiện tiên quyết để:
- thêm integrations layer
- thêm policy packs
- thêm runtime integrations

---

# 3. Phase B — Storage Boundary

## 3.1 Objective

Xây `ferrum-store` đủ để persist core state.

## 3.2 Success criteria

Phase B thành công khi hệ lưu và đọc lại được ít nhất:

- intents
- proposals
- capabilities
- executions
- rollback contracts
- provenance events

và relation giữa chúng không bị mất.

## 3.3 KPIs / target metrics

### Functional KPIs
- CRUD coverage cho core objects: **>= 90% object families**
- Persistable core object families: **6/6**
- Recovery of execution lineage by ID: **>= 1 hop chain query works**

### Quality KPIs
- Serialization/deserialization failures trong happy path tests: **0**
- Orphaned core records trong test fixtures: **0 known unresolved**
- Persistence smoke tests: **100% pass**

### Queryability KPIs
- Query by `intent_id`: **pass**
- Query by `execution_id`: **pass**
- Query by `capability_id`: **pass**

## 3.4 Release gate

Chỉ sang Phase C khi:
- state lõi lưu được bền vững
- execution state không còn chỉ ở memory tạm
- provenance có chỗ bám thật để phase sau query và verify

## 3.5 Evidence cần có

- sqlite schema / migrations
- tests cho persist/load objects
- ví dụ query lineage tối thiểu

## 3.6 Liên hệ với kế hoạch nâng cấp/tích hợp

Cross-runtime provenance và future integrations sẽ cần storage nền ổn định.
Nếu không có storage boundary tốt, rất khó tích hợp:
- MCP runtime signals
- NemoClaw/OpenShell events
- approval backend events

---

# 4. Phase C — Firewall MVP

## 4.1 Objective

Thay `NoopFirewall` bằng một lớp rule-based tối thiểu nhưng có ý nghĩa.

## 4.2 Success criteria

Phase C thành công khi hệ có thể:

- gắn trust labels
- tính taint score
- phát hiện contradiction cơ bản giữa intent và proposal
- sanitize output
- phát hiện DLP findings cơ bản

## 4.3 KPIs / target metrics

### Coverage KPIs
- Trust labeling coverage cho core input types: **>= 80%**
- Taint scoring path coverage cho risky inputs: **>= 80%**
- Output sanitize coverage cho core tool outputs: **>= 80%**

### Safety KPIs
- Known obvious poisoned-input cases blocked/quarantined: **>= 70% target**
- False-allow rate trên curated risky fixtures: **<= 10% target**
- Secret leakage in sanitized outputs on test fixtures: **0**

### Quality KPIs
- Firewall unit tests pass rate: **100%**
- Contradiction checks active for mutation proposals: **100% of mutation paths**

## 4.4 Release gate

Chỉ sang Phase D khi:
- mutation paths không còn chạy với firewall “trống”
- có ít nhất một tập poisoned/risky fixtures chứng minh firewall có tác dụng

## 4.5 Evidence cần có

- rule-based labeler
- taint scorer
- sanitize tests
- poisoned context regression fixtures đầu tiên

## 4.6 Liên hệ với kế hoạch nâng cấp/tích hợp

Firewall MVP là điều kiện để về sau tích hợp runtime khác mà vẫn giữ chung semantics.
Nếu muốn tích hợp:
- MCP runtimes
- local tools
- NemoClaw/OpenShell events

thì dữ liệu đi vào FerrumGate phải được quy về trust/taint model thống nhất.

---

# 5. Phase D — Adapter-backed Rollback

## 5.1 Objective

Biến rollback từ spec thành hành vi thật thông qua các adapter đầu tiên.

## 5.2 Success criteria

Phase D thành công khi có ít nhất 3 adapters usable:

- filesystem
- sqlite
- maildraft

và mỗi adapter đều có:
- happy path
- verify path
- recovery path

## 5.3 KPIs / target metrics

### Adapter KPIs
- Usable adapters count: **>= 3**
- Adapter test pass rate: **100%**
- Adapter verify coverage on mutating ops: **100% of supported ops**

### Recovery KPIs
- Rollback/compensation success rate trên test fixtures: **>= 90% target**
- R3 auto-commit violations: **0**
- Mutation paths without recovery contract in supported adapters: **0**

### Safety KPIs
- Maildraft “no-send” violations in v1: **0**
- Filesystem restore failures on controlled fixtures: **<= 10% target**
- SQLite transaction recovery failures on controlled fixtures: **<= 10% target**

## 5.4 Release gate

Chỉ sang Phase E khi:
- có ít nhất một full recoverable mutation path hoạt động thật
- recovery semantics không còn chỉ nằm trên giấy

## 5.5 Evidence cần có

- adapter implementations
- rollback/compensation tests
- docs ngắn mô tả contract từng adapter

## 5.6 Liên hệ với kế hoạch nâng cấp/tích hợp

Đây là phase tạo moat thực tế cho FerrumGate.
Các nâng cấp tương lai như:
- Reversible Execution Planner
- Outcome Contract enforcement
- cross-runtime governance

đều cần một adapter/recovery model thật chứ không chỉ mock.

---

# 6. Phase E — Gateway Orchestration

## 6.1 Objective

Nối full execution path trong `ferrum-gateway`.

## 6.2 Success criteria

Phase E thành công khi gateway có thể điều phối một flow đầy đủ:

proposal -> evaluate -> mint -> prepare -> execute -> verify -> compensate -> emit provenance (internal lifecycle: commit/rollback not exposed as v1 routes)

cho ít nhất một happy path và một negative path.

## 6.3 KPIs / target metrics

### Flow KPIs
- End-to-end happy path count: **>= 1 working path**
- End-to-end negative paths covered: **>= 2**
  - deny path
  - quarantine hoặc rollback path

### Governance KPIs
- Mutation executions passing through gateway: **100% for supported flows**
- Capability validation before execution: **100% for supported flows**
- Provenance emission coverage for supported flows: **100%**

### Outcome KPIs
- Verified commits for supported happy paths: **100%**
- Unsupported silent bypasses detected: **0 allowed**
- Terminal state completeness for supported executions: **100%**

## 6.4 Release gate

Chỉ sang Phase F khi:
- gateway không còn chỉ là HTTP scaffold
- supported flows đi qua đầy đủ governance chain
- lineage đầy đủ cho happy path và negative path

## 6.5 Evidence cần có

- integration test end-to-end
- logs/provenance examples
- CLI/server demo path
- docs cập nhật flow thực tế

## 6.6 Liên hệ với kế hoạch nâng cấp/tích hợp

Phase E là điểm tối thiểu để sau này cắm integrations layer:
- `ferrum-integrations-mcp`
- `ferrum-integrations-local`
- `ferrum-integrations-nemoclaw`

Nếu gateway orchestration chưa xong, tích hợp với runtime khác sẽ chỉ làm tăng complexity mà không tăng giá trị.

---

# 7. Phase F — Hardening, Evidence, and Integration Readiness

## 7.1 Objective

Biến repo từ “MVP chạy được” thành “nền tảng đủ để người khác triển khai tiếp và tích hợp tiếp”.

## 7.2 Success criteria

Phase F thành công khi repo có:

- integration tests có ý nghĩa
- poisoned-context tests
- lineage/replay tests
- docs đủ để handoff
- release checklist
- bằng chứng rõ ràng rằng invariants cốt lõi vẫn giữ được

## 7.3 KPIs / target metrics

### Testing KPIs
- Happy path integration tests: **pass**
- Deny path tests: **pass**
- Quarantine path tests: **pass**
- Rollback path tests: **pass**
- Poisoned-context test suite pass rate: **>= 80% target on curated fixtures**

### Documentation KPIs
- Docs coverage for core architecture: **100%**
- Docs coverage for supported adapters: **100%**
- Implementation-path docs completeness: **100%**
- Release checklist presence: **yes**

### Handoff KPIs
- New agent bootstrap time to first meaningful task: **<= 1 reading session target**
- Number of unresolved “where to start?” blockers in docs: **0**
- Source-of-truth ambiguity on critical invariants: **0**

### Integration readiness KPIs
- Integration boundary docs for runtime/vendor integration: **present**
- Event mapping readiness for external runtime signals: **defined**
- Vendor-neutral positioning retained in docs: **yes**

## 7.4 Release gate

FerrumGate được xem là “đủ nền tảng để nâng cấp và tích hợp tiếp” khi:
- docs, tests, governance flow và recovery flow không còn mâu thuẫn nhau
- người khác có thể vào repo và đi tiếp mà không cần tái thiết kế lại toàn bộ

## 7.5 Evidence cần có

- final docs pack
- test reports / smoke evidence
- supported flows list
- open gaps list
- clear next-step backlog

## 7.6 Liên hệ với kế hoạch nâng cấp/tích hợp

Đây là phase xác nhận FerrumGate đã sẵn sàng cho các nâng cấp ở tài liệu 90:
- outcome contracts
- reversible execution planner
- cross-runtime provenance fabric
- vendor-neutral integrations

---

# 8. KPI cho các nâng cấp sau khi MVP hoàn thành

Phần này nối trực tiếp với **kế hoạch nâng cấp và tích hợp**.

---

# 8.1 Upgrade Track U1 — Outcome-aware Governance

## Objective
Thêm `Outcome Contract` hoặc lớp tương đương để đo “điểm kết thúc hợp lệ” của workflow.

## Success criteria
- mỗi supported workflow có outcome expectations rõ
- proposal evaluation không chỉ nhìn tool call, mà còn nhìn outcome alignment
- drift detection có căn cứ tốt hơn

## KPIs
- Supported workflows with explicit outcome contract: **>= 2 use cases**
- Outcome contract coverage on supported happy paths: **100%**
- Drift detection precision on curated fixtures: **>= 70% target**

## Gate
Chỉ xem U1 thành công khi outcome layer thực sự ảnh hưởng đến decision hoặc verification path.

---

# 8.2 Upgrade Track U2 — Reversible Execution Planner

## Objective
Planner tự sinh hoặc hỗ trợ sinh:
- verify checks
- compensation steps
- stop points
- recovery plan quality cao hơn

## Success criteria
- rollback contract generation bớt thủ công
- verify/compensate path giàu hơn
- adapter recovery semantics dùng được tốt hơn

## KPIs
- Supported adapters using planner-generated artifacts: **>= 2**
- Recovery plan completeness on supported adapters: **>= 80% target**
- Manual rollback-contract authoring burden reduction: **qualitative decrease required**

## Gate
Planner phải tạo được giá trị thực, không chỉ thêm abstraction.

---

# 8.3 Upgrade Track U3 — Cross-runtime Provenance Fabric

## Objective
Gộp event từ nhiều runtime/sandbox/tool sources vào cùng execution lineage.

## Success criteria
- lineage không còn chỉ phản ánh event nội bộ FerrumGate
- event từ runtime bên ngoài có thể map vào execution graph
- query theo execution vẫn coherent

## KPIs
- External runtime event source count integrated: **>= 1 initially**
- Execution lineages containing both internal + external events: **>= 1 supported integration**
- Orphan external events on supported integrations: **0 target**

## Gate
Phải chứng minh event bên ngoài không chỉ được log riêng, mà được liên kết vào lineage.

---

# 8.4 Upgrade Track U4 — Runtime Integrations

## Objective
Tích hợp với:
- MCP runtimes
- local runtimes
- NemoClaw/OpenShell path hoặc tương đương

## Success criteria
- integration tách lớp, không khóa core vào vendor
- mapping policy/runtime signals vào FerrumGate lineage
- runtime integration không phá 4 trụ lõi

## KPIs
- Runtime integrations available: **>= 1 serious integration**
- Core crate changes required per new runtime integration: **minimized**
- Vendor-specific assumptions leaked into core crates: **0 target**

## Gate
Nếu integration làm core phụ thuộc vendor/runtime quá mạnh, coi như chưa đạt.

---

# 9. Cross-phase KPI summary

## 9.1 Core correctness KPIs
- Workspace compile success: **100%**
- Critical spec drift: **0**
- R3 auto-commit violations: **0**
- Capability single-use violations: **0**
- Supported mutation paths without provenance: **0**
- Supported mutation paths without recovery semantics: **0**

## 9.2 Quality KPIs
- Core tests pass: **100%**
- Integration tests for supported flows: **pass**
- Poisoned-context regressions caught on curated fixtures: **>= 80% target**
- Docs coverage for supported behavior: **100%**

## 9.3 Integration-readiness KPIs
- Integration boundary docs present: **yes**
- Runtime-specific logic isolated from core: **yes**
- Event mapping model for integrations defined: **yes**
- Upgrade tracks explicitly documented: **yes**

---

# 10. Final evaluation rule

FerrumGate chỉ được coi là “thành công” khi đạt đồng thời 3 tầng:

## 10.1 Technical success
- build được
- flow chạy được
- rollback/provenance hoạt động

## 10.2 Governance success
- intent/capability/provenance/rollback được enforce thật
- mutation không chạy theo kiểu quyền rộng và mù lineage

## 10.3 Strategic success
- repo đủ rõ để người khác tiếp tục phát triển
- sẵn sàng cho nâng cấp và tích hợp
- không bị trôi thành wrapper cho một vendor hoặc một runtime duy nhất

---

# 11. Kết luận

Tài liệu này tồn tại để trả lời câu hỏi:

> “Khi nào một phase thực sự xong, và khi nào FerrumGate thực sự sẵn sàng nâng cấp / tích hợp?”

Câu trả lời là:
- không chỉ khi code compile
- không chỉ khi có demo
- mà khi **governance semantics được giữ**, **evidence tồn tại**, và **repo sẵn sàng để người khác đi tiếp mà không bị mơ hồ**.
