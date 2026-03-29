# 91 - Phase Success Criteria and KPIs

> Tai lieu nay dinh nghia **success criteria**, **KPI muc tieu**, **release gates**, va **evidence can co** cho tung phase cua FerrumGate.
>
> Tai lieu nay da **bao gom** goc nhin ve ke hoach nang cap va tich hop: nghia la success criteria khong chi do "code chay duoc", ma con do he da san sang de nang cap/tich hop theo huong khac biet hay chua.
>
> Tai lieu nay **khong thay the** `09-implementation-path.md`, ma la lop do luong cu the hon de ra quyet dinh "phase da xong chua".

## Current progress snapshot (2026-03-27)

### Phase status now

- **Phase A**: coi nhu dat release gate; workspace va shape loi da du on dinh de lam nen cho persistence/runtime flow.
- **Phase B**: coi nhu dat release gate; SQLite-backed persistence cho intents/proposals/capabilities/executions/rollback/provenance da chay qua integration path thuc.
- **Phase C**: da co firewall MVP co y nghia trong branch hien tai; trust labeling, taint scoring, contradiction checks, output sanitization, basic DLP, va execution-time HTTP/File/Sqlite/Git/EmailDraft resource enforcement da duoc wire vao gateway. Tat ca 5 resource binding types (File, Http, Sqlite, Git, EmailDraft) gio day deu co execution-time enforcement.
- **Phase D**: da co adapter-backed rollback evidence toi thieu cho filesystem, sqlite, maildraft draft-only, git local-ref, va HTTP full-parity path (GET/POST/PUT/PATCH/DELETE + body/header/query binding + auth); gateway prepare flow gio co the route sang fs/sqlite/maildraft/git/http adapter va integration tests da chung minh file create/delete, file overwrite/restore, sqlite row restore, maildraft draft create/delete recovery, git ref restore, va HTTP GET/POST verify/rollback no-op path. Co the xem la dat release gate Phase D cho supported adapter set hien tai; `EmailSend` va HTTP remote mutation recovery parity van ngoai scope adapter-backed recovery trong v1; HTTP rollback/compensate van conservative no-op.
- **Phase E**: coi nhu dat cho supported flow hien tai; gateway da di qua `evaluate -> mint -> authorize -> prepare -> execute -> verify -> commit`, cung negative/recovery paths va approval/draft-only governance.
- **Phase F**: da co poisoned-context suite, provenance minimum-chain evidence, provenance query fail-closed endpoint, va docs handoff ro rang cho supported flows + open gaps. Git gateway path va HTTP full-parity adapter gio da co them evidence end-to-end. Operator/runtime hardening co ban da hoan thanh (troubleshooting, diagnostics, deployment docs); P1 observability baseline (tracing + Prometheus) va TLS/ingress runbook con lai. Sync groundwork hoan thanh: Sync-3a probe crate (`ferrum-sync`) da co, cac slice tiep theo sang P2.


### Latest evidence snapshot

- `cargo check --workspace`: pass sau khi wire firewall vao gateway va cap nhat proto request shape.
- `cargo clippy -p ferrum-gateway -p ferrum-adapter-maildraft -p ferrum-rollback -- -D warnings`: pass.
- `cargo test --package ferrum-firewall`: `35/35` pass.
- `cargo test --package ferrum-adapter-git`: `8/8` pass.
- `cargo test --package ferrum-adapter-http`: `11/11` pass.
- `cargo test --package integration-tests --test integration_gateway_flow`: `74/74` pass.
- `cargo test --package integration-tests --test integration_poisoned_context`: `5/5` pass (curated poisoned-context regression suite).
- `cargo test --package integration-tests --test integration_lineage_chain`: `5/5` pass (provenance minimum-chain/lineage evidence tests, including execution lineage endpoint).
- Targeted git adapter/gateway coverage va HTTP initial-slice coverage tren mainline da xanh; full `integration_gateway_flow` suite pass sau PR #13.
- Provenance edges are now persisted to `provenance_edges` table and queryable via `ProvenanceRepo::get_edges_to()`, execution lineage is available via `GET /v1/provenance/lineage/{execution_id}`, and a minimal fail-closed provenance query endpoint exists at `POST /v1/provenance/query`.
- `docs/implementation-path/11-phase-f-evidence.md`: handoff tai lieu cho supported flows, evidence links, va open gaps hien tai.
- Gateway firewall coverage hien da co trust-context derivation, read-only contradiction blocking, MCP scope contradiction blocking, compile-time taint lineage propagation, DLP redact/detect, execution-time enforcement cho ca 5 resource binding types (File, Http, Sqlite, Git, EmailDraft), va regression tests cho tat ca enforcement paths bao gom: empty-scope read-only bypass, host/method/header mismatch, missing binding, file path mismatch, file traversal, write-on-read binding, Sqlite db_path/table violations, Git repo_path/ref violations, va EmailDraft recipient/send violations.
- Gateway hardening/evidence hien da co them capability single-use deny, explicit scope mismatch deny at mint time, direct R3 no-auto-commit evidence, fs/sqlite/maildraft/git/http-full-parity adapter-backed recovery evidence, va explicit prepare-time deny cho `EmailDraft allow_send=true` de tranh silently fall-through sang `noop`.
- Mainline da hap thu cac moc quan trong truoc do:
  - `PR #3` - harden proposal provenance coverage
  - `PR #5` - execute / verify / commit gateway flow
  - `PR #6` - recovery terminal gateway paths
  - `PR #8` - approval and draft-only gateway flow
  - `PR #13` - fix git verify handoff and add initial http adapter flow

### Working interpretation of release gates

- Co the xem **Phase B** la complete theo tai lieu nay.
- Co the xem **Phase E** la complete cho supported SQLite-backed gateway flow hien tai.
- **Phase C** da dat mot MVP co tac dung that cho compile/evaluate va full execution-time enforcement cho ca 5 resource binding types (File, Http, Sqlite, Git, EmailDraft); co the xem la dat release gate cho firewall MVP hien tai voi day du execution-time enforcement coverage.
- Co the xem **Phase F** da dat muc evidence/docs handoff cho supported gateway flow hien tai; cac gap con lai da duoc liet ke ro thanh open gaps de xu ly tiep, voi git gateway path va HTTP full-parity adapter da co them evidence end-to-end.

---

# 1. Cach doc tai lieu nay

Moi phase gom 5 phan:

1. **Objective** - muc tieu cua phase  
2. **Success criteria** - dieu kien hoan thanh mang tinh chuc nang  
3. **KPIs / target metrics** - chi so muc tieu  
4. **Release gate** - dieu kien cho phep chuyen phase  
5. **Evidence** - bang chung phai co trong repo  

## 1.1 Luu y ve KPI

Các KPI o day la **target KPIs cho trien khai**, khong phai so lieu da do san.
Chung duoc dung nhu nguong muc tieu de agent/dev biet khi nao co the coi mot phase la "du tot".

---

# 2. Phase A - Compile and Shape Stability

## 2.1 Objective

On dinh workspace, shape objects va moi quan he giua:
- code
- contracts
- schemas
- openapi
- docs

## 2.2 Success criteria

Phase A duoc xem la thanh cong khi:

- Rust workspace build duoc o muc `cargo check --workspace`
- tat ca crate trong workspace khop members/dependencies
- khong con missing modules/imports obvious
- domain shapes trong `ferrum-proto` khong drift ro rang khoi docs/spec
- root repo du on dinh de phase sau xay tiep

## 2.3 KPIs / target metrics

### Build KPIs
- Workspace compile success rate: **100%**
- Missing crate/module blockers: **0**
- Broken cargo member references: **0**

### Consistency KPIs
- Drift giua `ferrum-proto` va `schemas/`: **0 unresolved critical mismatches**
- Drift giua `ferrum-proto` va `contracts/`: **0 unresolved critical mismatches**
- Drift giua `openapi/` va API structs loi: **0 unresolved critical mismatches**

### Hygiene KPIs
- `cargo fmt --all`: **pass**
- `clippy` critical warnings: **0 blocker warnings**
- repo layout validation: **pass**

## 2.4 Release gate

Chi duoc sang Phase B khi:
- workspace check pass
- root docs van phan anh reality
- object model loi on dinh du de persistence layer bam vao

## 2.5 Evidence can co

- CI/log chung minh `cargo check --workspace` pass
- danh sach crate/members cuoi cung
- ghi chu sync giua code va schemas/contracts/openapi

## 2.6 Lien he voi ke hoach nang cap/tich hop

Neu Phase A khong sach, moi integration sau nay se roi vao drift.
Phase A la dieu kien tien quyet de:
- them integrations layer
- them policy packs
- them runtime integrations

---

# 3. Phase B - Storage Boundary

## 3.1 Objective

Xay `ferrum-store` du de persist core state.

## 3.2 Success criteria

Phase B thanh cong khi he luu va doc lai duoc it nhat:

- intents
- proposals
- capabilities
- executions
- rollback contracts
- provenance events

va relation giua chung khong bi mat.

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

Chi sang Phase C khi:
- state loi luu duoc ben vung
- execution state khong con chi o memory tam
- provenance co cho bam that de phase sau query va verify

## 3.5 Evidence can co

- sqlite schema / migrations
- tests cho persist/load objects
- vi du query lineage toi thieu

## 3.6 Lien he voi ke hoach nang cap/tich hop

Cross-runtime provenance va future integrations se can storage nen on dinh.
Neu khong co storage boundary tot, rat kho tich hop:
- MCP runtime signals
- NemoClaw/OpenShell events
- approval backend events

---

# 4. Phase C - Firewall MVP

## 4.1 Objective

Thay `NoopFirewall` bang mot lop rule-based toi thieu nhung co y nghia.

## 4.2 Success criteria

Phase C thanh cong khi he co the:

- gan trust labels
- tinh taint score
- phat hien contradiction co ban giua intent va proposal
- sanitize output
- phat hien DLP findings co ban

## 4.3 KPIs / target metrics

### Coverage KPIs
- Trust labeling coverage cho core input types: **>= 80%**
- Taint scoring path coverage cho risky inputs: **>= 80%**
- Output sanitize coverage cho core tool outputs: **>= 80%**

### Safety KPIs
- Known obvious poisoned-input cases blocked/quarantined: **>= 70% target**
- False-allow rate tren curated risky fixtures: **<= 10% target**
- Secret leakage in sanitized outputs on test fixtures: **0**

### Quality KPIs
- Firewall unit tests pass rate: **100%**
- Contradiction checks active for mutation proposals: **100% of mutation paths**

## 4.4 Release gate

Chi sang Phase D khi:
- mutation paths khong con chay voi firewall "trong"
- co it nhat mot tap poisoned/risky fixtures chung minh firewall co tac dung

## 4.5 Evidence can co

- rule-based labeler
- taint scorer
- sanitize tests
- poisoned context regression fixtures dau tien

## 4.6 Lien he voi ke hoach nang cap/tich hop

Firewall MVP la dieu kien de ve sau tich hop runtime khac ma van giu chung semantics.
Neu muon tich hop:
- MCP runtimes
- local tools
- NemoClaw/OpenShell events

thi du lieu di vao FerrumGate phai duoc quy ve trust/taint model thong nhat.

---

# 5. Phase D - Adapter-backed Rollback

## 5.1 Objective

Bien rollback tu spec thanh hanh vi that thong qua cac adapter dau tien.

## 5.2 Success criteria

Phase D thanh cong khi co it nhat 3 adapters usable:

- filesystem
- sqlite
- maildraft

va moi adapter deu co:
- happy path
- verify path
- recovery path

## 5.3 KPIs / target metrics

### Adapter KPIs
- Usable adapters count: **>= 3**
- Adapter test pass rate: **100%**
- Adapter verify coverage on mutating ops: **100% of supported ops**

### Recovery KPIs
- Rollback/compensation success rate tren test fixtures: **>= 90% target**
- R3 auto-commit violations: **0**
- Mutation paths without recovery contract in supported adapters: **0**

### Safety KPIs
- Maildraft "no-send" violations in v1: **0**
- Filesystem restore failures on controlled fixtures: **<= 10% target**
- SQLite transaction recovery failures on controlled fixtures: **<= 10% target**

## 5.4 Release gate

Chi sang Phase E khi:
- co it nhat mot full recoverable mutation path hoat dong that
- recovery semantics khong con chi nam tren giay

## 5.5 Evidence can co

- adapter implementations
- rollback/compensation tests
- docs ngan mo ta contract tung adapter

## 5.6 Lien he voi ke hoach nang cap/tich hop

Day la phase tao moat thuc te cho FerrumGate.
Các nang cap tuong lai nhu:
- Reversible Execution Planner
- Outcome Contract enforcement
- cross-runtime governance

deu can mot adapter/recovery model that chu khong chi mock.

---

# 6. Phase E - Gateway Orchestration

## 6.1 Objective

Noi full execution path trong `ferrum-gateway`.

## 6.2 Success criteria

Phase E thanh cong khi gateway co the dieu phoi mot flow day du:

proposal -> evaluate -> mint -> prepare -> execute -> verify -> commit/rollback -> emit provenance

cho it nhat mot happy path va mot negative path.

## 6.3 KPIs / target metrics

### Flow KPIs
- End-to-end happy path count: **>= 1 working path**
- End-to-end negative paths covered: **>= 2**
  - deny path
  - quarantine hoac rollback path

### Governance KPIs
- Mutation executions passing through gateway: **100% for supported flows**
- Capability validation before execution: **100% for supported flows**
- Provenance emission coverage for supported flows: **100%**

### Outcome KPIs
- Verified commits for supported happy paths: **100%**
- Unsupported silent bypasses detected: **0 allowed**
- Terminal state completeness for supported executions: **100%**

## 6.4 Release gate

Chi sang Phase F khi:
- gateway khong con chi la HTTP scaffold
- supported flows di qua day du governance chain
- lineage day du cho happy path va negative path

## 6.5 Evidence can co

- integration test end-to-end
- logs/provenance examples
- CLI/server demo path
- docs cap nhat flow thuc te

## 6.6 Lien he voi ke hoach nang cap/tich hop

Phase E la diem toi thieu de sau nay cam integrations layer:
- `ferrum-integrations-mcp`
- `ferrum-integrations-local`
- `ferrum-integrations-nemoclaw`

Neu gateway orchestration chua xong, tich hop voi runtime khac se chi lam tang complexity ma khong tang gia tri.

---

# 7. Phase F - Hardening, Evidence, and Integration Readiness

## 7.1 Objective

Bien repo tu "MVP chay duoc" thanh "nen tang du de nguoi khac trien khai tiep va tich hop tiep".

## 7.2 Success criteria

Phase F thanh cong khi repo co:

- integration tests co y nghia
- poisoned-context tests
- lineage/replay tests
- docs du de handoff
- release checklist
- bang chung ro rang rang invariants cot loi van giu duoc

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
- Number of unresolved "where to start?" blockers in docs: **0**
- Source-of-truth ambiguity on critical invariants: **0**

### Integration readiness KPIs
- Integration boundary docs for runtime/vendor integration: **present**
- Event mapping readiness for external runtime signals: **defined**
- Vendor-neutral positioning retained in docs: **yes**

## 7.4 Release gate

FerrumGate duoc xem la "du nen tang de nang cap va tich hop tiep" khi:
- docs, tests, governance flow va recovery flow khong con mau thuan nhau
- nguoi khac co the vao repo va di tiep ma khong can tai thiet ke lai toan bo

## 7.5 Evidence can co

- final docs pack
- test reports / smoke evidence
- supported flows list
- open gaps list
- clear next-step backlog

## 7.6 Lien he voi ke hoach nang cap/tich hop

Day la phase xac nhan FerrumGate da san sang cho cac nang cap o tai lieu 90:
- outcome contracts
- reversible execution planner
- cross-runtime provenance fabric
- vendor-neutral integrations

---

# 8. KPI cho cac nang cap sau khi MVP hoan thanh

Phan nay noi truc tiep voi **ke hoach nang cap va tich hop**.

---

# 8.1 Upgrade Track U1 - Outcome-aware Governance

## Objective
Them `Outcome Contract` hoac lop tuong duong de do "diem ket thuc hop le" cua workflow.

## Success criteria
- moi supported workflow co outcome expectations ro
- proposal evaluation khong chi nhin tool call, ma con nhin outcome alignment
- drift detection co can cu tot hon

## KPIs
- Supported workflows with explicit outcome contract: **>= 2 use cases**
- Outcome contract coverage on supported happy paths: **100%**
- Drift detection precision on curated fixtures: **>= 70% target**

## Gate
Chi xem U1 thanh cong khi outcome layer thuc su anh huong den decision hoac verification path.

---

# 8.2 Upgrade Track U2 - Reversible Execution Planner

## Objective
Planner tu sinh hoac ho tro sinh:
- verify checks
- compensation steps
- stop points
- recovery plan quality cao hon

## Success criteria
- rollback contract generation bot thu cong
- verify/compensate path giau hon
- adapter recovery semantics dung duoc tot hon

## KPIs
- Supported adapters using planner-generated artifacts: **>= 2**
- Recovery plan completeness on supported adapters: **>= 80% target**
- Manual rollback-contract authoring burden reduction: **qualitative decrease required**

## Gate
Planner phai tao duoc gia tri thuc, khong chi them abstraction.

---

# 8.3 Upgrade Track U3 - Cross-runtime Provenance Fabric

## Objective
Gop event tu nhieu runtime/sandbox/tool sources vao cung execution lineage.

## Success criteria
- lineage khong con chi phan anh event noi bo FerrumGate
- event tu runtime ben ngoai co the map vao execution graph
- query theo execution van coherent

## KPIs
- External runtime event source count integrated: **>= 1 initially**
- Execution lineages containing both internal + external events: **>= 1 supported integration**
- Orphan external events on supported integrations: **0 target**

## Gate
Phai chung minh event ben ngoai khong chi duoc log rieng, ma duoc lien ket vao lineage.

---

# 8.4 Upgrade Track U4 - Runtime Integrations

## Objective
Tich hop voi:
- MCP runtimes
- local runtimes
- NemoClaw/OpenShell path hoac tuong duong

## Success criteria
- integration tach lop, khong khoa core vao vendor
- mapping policy/runtime signals vao FerrumGate lineage
- runtime integration khong pha 4 tru loi

## KPIs
- Runtime integrations available: **>= 1 serious integration**
- Core crate changes required per new runtime integration: **minimized**
- Vendor-specific assumptions leaked into core crates: **0 target**

## Gate
Neu integration lam core phu thuoc vendor/runtime qua manh, coi nhu chua dat.

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

FerrumGate chi duoc coi la "thanh cong" khi dat dong thoi 3 tang:

## 10.1 Technical success
- build duoc
- flow chay duoc
- rollback/provenance hoat dong

## 10.2 Governance success
- intent/capability/provenance/rollback duoc enforce that
- mutation khong chay theo kieu quyen rong va mu lineage

## 10.3 Strategic success
- repo du ro de nguoi khac tiep tuc phat trien
- san sang cho nang cap va tich hop
- khong bi troi thanh wrapper cho mot vendor hoac mot runtime duy nhat

---

# 11. Ket luan

Tai lieu nay ton tai de tra loi cau hoi:

> "Khi nao mot phase thuc su xong, va khi nao FerrumGate thuc su san sang nang cap / tich hop?"

Cau tra loi la:
- khong chi khi code compile
- khong chi khi co demo
- ma khi **governance semantics duoc giu**, **evidence ton tai**, va **repo san sang de nguoi khac di tiep ma khong bi mo ho**.
