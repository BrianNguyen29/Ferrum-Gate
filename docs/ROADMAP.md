# Kế hoạch hoàn thiện FerrumGate sau conditional pilot

## Claim discipline

- **production-ready = NO** cho đến khi real domain + revalidation + signoff xong.
- **full G2 = NOT COMPLETE** cho đến khi có evidence/signoff đầy đủ.
- **Block A = WAIVED/CONDITIONAL**, chưa được coi là closed.
- Real domain được deferred theo yêu cầu; sẽ bổ sung sau.

Báo cáo này tập trung vào các mục còn lại:

- full G2, trừ real domain.
- target-host MCP/live workload.
- PostgreSQL/HA.
- security model nâng cao.
- product-facing documentation.
- user-facing docs.
- demo flows.
- hosted deployment story.
- admin/operator UX.
- policy authoring UX.
- tenant/security model.
- production SLO/SLA evidence.

## Baseline references

Roadmap này là tài liệu lập kế hoạch bổ sung, không thay thế các nguồn trạng thái hiện tại. Khi triển khai, đối chiếu với các tài liệu nền sau:

- `docs/implementation-path/01-current-state.md` — trạng thái hiện tại và non-claims.
- `docs/implementation-path/67-production-readiness-roadmap.md` — roadmap production-readiness hiện hữu và blockers.
- `docs/implementation-path/122-completion-roadmap-and-hardening-tracker.md` — tracker hoàn thiện/hardening.
- `docs/implementation-path/125-manual-gates-runbook.md` — manual validation gates, không tự động kích hoạt CI.
- `docs/implementation-path/06-guardrails-and-invariants.md` — guardrails và invariants bắt buộc.
- `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` — boundary single-node v1.
- `docs/ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/00-roadmap-charter.md` — roadmap v2 hiện có; cần reconcile trước khi tạo cây `docs/production-readiness-v2/`.
- `docs/PRODUCTION_NOTES.md` — production notes, SQLite/operational baseline.

## Naming crosswalk

To avoid ambiguity, the following axes are used in this and related docs. They are **not interchangeable**.

| Axis | Meaning | Values | Used in |
|------|---------|--------|---------|
| **ROADMAP Phase** | Execution phase on the post-pilot production path | Phase 0–9 | `docs/ROADMAP.md`, `docs/production-readiness-v2/` |
| **Priority label** | Relative urgency of a task *within* a phase | P0, P1, P2, P3 | Tables inside `docs/ROADMAP.md` and `docs/production-readiness-v2/` |
| **Legacy quarter** | Historical quarterly work package from the baseline roadmap-v2 pack | Q1, Q2, Q3, Q4 | `docs/ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/` |

- **Phases 0–9** are the current post-pilot execution sequence. They are not quarters and do not map 1:1 to calendar quarters.
- **P0–P3** are priority labels applied to individual tasks or deliverables. They are not phases.
- **Q1–Q4** are legacy quarterly planning buckets from the historical/baseline roadmap-v2 pack. They remain relevant as historical reference but do not govern the current post-pilot phase sequence.

## 1. Nhận định tổng quan

FerrumGate hiện đã có nền tảng tốt:

- governance core đã mạnh;
- SQLite single-node pilot đã đủ tốt cho conditional RC;
- local validation/evidence khá đầy đủ;
- MCP local smoke đã pass;
- D1–D6 local/API drills đã pass;
- target-host L1–L5 đã có evidence trong phạm vi DuckDNS conditional pilot;
- PostgreSQL local runtime đã có nền tảng đáng kể;
- docs/evidence discipline tốt.

Nhưng để tiến tới trạng thái “production-grade” cần chuyển từ:

> RC-ready / conditional single-node SQLite pilot

sang:

> production-grade governed execution platform

Muốn vậy, trọng tâm không còn là “sửa lặt vặt”, mà là hoàn thiện 6 khối lớn:

1. G2 + live workload evidence
2. PostgreSQL production hardening
3. Target-host MCP governance pipeline
4. Security/tenant/RBAC model
5. Product/operator/policy UX
6. SLO/SLA + production evidence

## 2. Critical path

Critical path hợp lý nhất:

```
PostgreSQL production foundation
        ↓
SLO/SLA + workload evidence
        ↓
Target-host MCP/live workload
        ↓
Tenant/security model
        ↓
HA/multi-node
        ↓
Production-ready evidence pack
```

Lý do: nhiều mục phụ thuộc vào PostgreSQL production:

- tenant isolation cần PG hoặc ít nhất cần store model có tenant dimension;
- HA cần PG;
- SLO/SLA cần production-like backend;
- target workload nên đo trên backend dự kiến dùng thật;
- admin/operator UX nên thiết kế quanh store/posture production, không nên khóa chặt vào SQLite pilot.

## 3. Gap analysis theo từng nhóm

### 3.1 Full G2, trừ real domain

#### Hiện trạng

| Mục | Trạng thái |
|-----|-----------|
| G2.1–G2.8 | signed cho conditional single-node SQLite pilot |
| Full G2 production posture | chưa hoàn tất |
| Block A real domain | deferred theo yêu cầu |
| Workload model | có baseline, nhưng cần refresh bằng target metrics |
| Operator re-signoff | cần sau khi có evidence mới |
| Production SLO/SLA | chưa formalize đủ |

#### Các gap còn lại nếu bỏ qua real domain

**Gap G2-1: Chưa có production SLO/SLA chính thức**

Hiện có RPO/RTO và một số metric trong docs, nhưng chưa đủ thành SLO/SLA production.

**Lưu ý về rate-limit và SLO (đã giải quyết 2026-05-21 — conservative resolution):**
- Default safety profile `2/50` là **intentionally conservative** và không thay đổi.
- SLO default-config gap đã được **đóng với conservative resolution**: default là safety-oriented, SLO certification yêu cầu explicit high-throughput profile `1000/10000`.
- Operator phải tune dựa trên traffic/IP distribution thực tế.
- Xem `docs/operations/rate-limit-tuning-guide.md`.
- **Không phải lỗi code**: 429 cao dưới default/tuned config là expected behavior, không phải defect.

Cần định nghĩa:

| Nhóm SLO | Ví dụ cần có |
|----------|-------------|
| Availability | `/v1/healthz`, `/v1/readyz/deep` uptime target |
| Latency | p50/p95/p99 cho evaluate/mint/authorize/prepare/execute/verify |
| Error rate | tỷ lệ 5xx, tỷ lệ 429/rate-limit |
| Durability | backup age, restore success, RPO/RTO |
| Correctness | 0 capability bypass, 0 provenance gap, 0 scope violation |
| Security | 0 auth bypass, 0 secret leak in output/logs |
| Operational | incident response time, alert acknowledgement time |

**Gap G2-2: Workload model còn assumption-based**

Hiện workload model có signed assumptions, nhưng cần cập nhật bằng số đo thật:

- throughput observed;
- latency p50/p95/p99;
- write queue depth;
- DB pool usage;
- adapter breakdown;
- error rate;
- memory/CPU trend;
- readiness probe stability.

**Gap G2-3: Sustained workload target-host chưa đủ production evidence**

Hiện có bounded local và một số target-host evidence, nhưng để full G2 cần sustained run rõ ràng hơn.

Nên dùng lại:

- `scripts/run_real_workload_generator.py`
- `scripts/run_g36_workload_wrapper.sh`
- `scripts/stress/run-all.sh`
- `scripts/check_pilot_readiness.py`

Acceptance target nên là:

```
baseline 600s
→ low 600s
→ target 1800s
→ spike 300s
→ cooldown 600s
```

Hoặc nâng lên production-grade:

```
minimum 1h sustained target workload
plus repeated daily runs for 7–30 days depending claim strength
```

**Gap G2-4: MCP target-host smoke chưa chạy theo live gateway**

MCP local smoke đã có, nhưng cần target-host MCP smoke:

- MCP server kết nối live gateway;
- bearer auth đúng;
- lifecycle tools chạy end-to-end;
- output redaction/sanitization đúng;
- provenance được tạo;
- no secrets leaked.

**Gap G2-5: Operator re-signoff**

Sau khi có evidence mới, cần refresh signoff:

- G2.1 workload model refreshed.
- G2.2 auth/TLS/security refreshed.
- G2.3 backup schedule refreshed.
- G2.4 restore drill refreshed.
- G2.5 RPO/RTO refreshed.
- G2.6 production evaluation refreshed.
- G2.7 accepted-risk review refreshed.
- G2.8 compensate/noop risk refreshed.

### 3.2 Target-host MCP/live workload

#### Hiện trạng

- MCP stdio server có 19 tools.
- Local MCP smoke pass.
- D1–D6 local/API pass.
- Target-host bridge L1–L5 có evidence, nhưng chưa phải MCP live workload đầy đủ.

Cần hoàn thiện 3 lớp.

#### Lớp 1 — Target-host MCP smoke

**Mục tiêu:**

```
MCP server → target ferrumd → governance API → actual lifecycle response
```

**Acceptance:**

- `tools/list` trả đủ 19 tools.
- 9 read-only tools gọi được với target gateway.
- mutating tools fail closed nếu thiếu auth.
- mutating lifecycle tools chạy được với auth hợp lệ trong bounded fixture.
- output redaction/sanitization kiểm tra được.
- logs không in secret.

#### Lớp 2 — MCP governed lifecycle

**Mục tiêu:**

```
submit_intent
→ evaluate_intent
→ mint_capability
→ authorize_execution
→ prepare_execution
→ execute_prepared
→ verify
→ query_lineage
```

**Acceptance:**

- capability single-use được chứng minh.
- TTL max 300s vẫn enforced.
- R3 không auto-commit.
- provenance chain đầy đủ.
- compensate path chạy ít nhất 1 case.
- approval path chạy ít nhất 1 case.

#### Lớp 3 — MCP live workload

**Mục tiêu:**

- agent/MCP client gọi nhiều lifecycle flows;
- chạy trên target-host;
- đo latency/error/readiness;
- có evidence artifact.

**Workload nên gồm:**

| Flow | Adapter | Mục tiêu |
|------|---------|----------|
| file write + verify + rollback | fs | validate rollback |
| git branch/commit dry path | git | validate ref capture |
| HTTP mutation bounded | http | validate idempotency/replay |
| SQLite mutation | sqlite | validate DB rollback |
| maildraft create/update/delete | maildraft | validate safe external communication draft |

### 3.3 PostgreSQL/HA

#### 3.3.1 PostgreSQL hiện đã có gì

Theo explorer, PostgreSQL local/runtime đã khá mạnh:

| Mục | Trạng thái |
|-----|-----------|
| 9 repo implementations | có |
| embedded schema migration | có |
| feature-gated postgres build | có |
| runtime DSN switching | có |
| pool config cơ bản | có |
| ferrum-migrate SQLite → PG | có |
| Docker Compose PG local | có |
| config templates | có |
| integration tests local | có |
| backup/restore runbook | có |
| ops cadence docs | có |

**Nhận định:**

PostgreSQL không phải bắt đầu từ số 0. Nền CRUD/migration/config đã có. Gap nằm ở production operations, HA, observability, target-host evidence, schema migration discipline.

#### 3.3.2 PostgreSQL production gaps

| Gap | Mức độ | Ý nghĩa |
|-----|--------|---------|
| Reconnect behavior documented; runtime recovery proof (B.2) deferred | trung bình/cao | PgPool transparent reconnect documented in operator runbook; automated restart-recovery test NOT implemented |
| No circuit breaker | cao | DB failure có thể lan ra gateway |
| No statement timeout | trung bình/cao | slow query có thể giữ connection quá lâu |
| No PG pool metrics | trung bình | không biết pool saturation |
| No PG-specific alert rules | trung bình | khó vận hành |
| No TLS/SSL DSN guidance | trung bình | production PG chưa hardened |
| No schema versioning chuẩn | trung bình/cao | hiện migration còn one-shot |
| No target-host PG drills | cao | chưa có evidence production PG |
| No PG restore drill evidence | cao | backup docs có, evidence chưa |
| No CI for postgres feature | trung bình | drift risk |
| No PgBouncer/connection pooling story | trung bình | scaling khó |
| No HA/failover | critical | chưa production HA |
| No replication configs | cao | không có standby/read replica |
| No failover runbook | cao | không có promote/reroute procedure |
| No split-brain prevention | cao | HA claim không thể có |

#### 3.3.3 PostgreSQL hardening plan

##### Phase PG-1 — Production PG baseline

**Mục tiêu:**

- chạy ferrumd với `postgres://...` trên target/staging production-like.
- `/v1/readyz/deep` pass.
- migration từ SQLite snapshot sang PG staging pass.
- full tests hoặc targeted integration trên PG pass.

**Deliverables:**

- PG target env doc.
- PG deployment runbook.
- PG migration evidence.
- PG readyz evidence.

**Acceptance:**

```
FERRUMD_STORE_DSN=postgres://...
/v1/readyz/deep = 200
ferrum-migrate completes
row counts match
content hash validation pass
```

##### Phase PG-2 — Connection hardening

**Implement:**

- statement_timeout.
- idle_in_transaction_session_timeout.
- pool acquire timeout metrics.
- reconnect/retry policy.
- DB health circuit breaker.
- graceful degraded readiness.

**Acceptance:**

- restart PG during test → ferrumd recovers or fails closed cleanly.
- slow query returns timeout.
- `/v1/readyz/deep` reflects DB unhealthy.
- metrics expose PG pool health.

##### Phase PG-3 — Backup/restore evidence

**Implement/execute:**

- scheduled pg_dump or WAL backup.
- retention pruning.
- restore drill to clean DB.
- row counts/hash checks.
- evidence artifact.

**Acceptance:**

```
pg_dump exit 0
pg_restore exit 0
restored row count matches
readiness after restore pass
RPO/RTO measured
```

##### Phase PG-4 — Schema migration discipline

**Implement:**

- migration version table.
- incremental migration files.
- idempotent migration runner.
- rollback/forward strategy.
- schema drift check.

**Acceptance:**

- migration can run twice safely.
- version is recorded.
- test migration from N to N+1.
- CI checks schema drift.

##### Phase PG-5 — HA design, then implementation

Start with ADR before code.

**Design options:**

| Option | Pros | Cons |
|--------|------|------|
| Managed PostgreSQL HA | easiest operationally | cloud/vendor dependency |
| Patroni | mature HA | operational complexity |
| repmgr | simpler than Patroni | still operational burden |
| manual failover | easiest for v1 | not true HA |
| read replicas only | easier scaling | write HA unresolved |

**Recommendation:**

- Step 1: managed PostgreSQL or manual failover runbook.
- Step 2: read replica support for reads.
- Step 3: automated failover only after tenant/security model is stable.

**Acceptance for HA claim:**

- primary failure drill.
- standby promotion.
- ferrumd reconnect/reroute.
- no split-brain.
- RPO/RTO measured.
- read/write behavior documented.
- `/v1/readyz/deep` reports role/replication health.

### 3.4 Security model nâng cao

#### Hiện trạng

- Auth model: Disabled hoặc Bearer.
- Bearer enough for pilot.
- No multi-tenancy.
- No RBAC.
- No scoped API tokens.
- No OIDC/JWT/SSO.
- No tenant isolation.
- No token lifecycle API.
- No actor-level authorization beyond bearer possession.

#### Security gaps

| Gap | Mức độ |
|-----|--------|
| Single bearer token global power | cao |
| No per-actor identity | cao |
| No roles/RBAC | cao |
| No tenant/org/workspace model | cao |
| No scoped tokens | cao |
| ~~No admin audit log separate from provenance~~ ✅ Done 2026-05-21 (SEC-6) | trung bình/cao |
| Capability revocation durability concerns | trung bình |
| No token rotation API | trung bình |
| No OIDC/JWT/SSO | trung bình |
| No mTLS option | thấp/trung bình |
| TLS delegated to proxy | chấp nhận được, cần docs tốt |

#### Security model target

Nên thiết kế 4 lớp:

```
Tenant
  └── Workspace/Project
        └── Actor
              ├── Human Operator
              ├── Auditor
              ├── Agent
              └── Service Account
```

**RBAC tối thiểu:**

| Role | Quyền |
|------|-------|
| admin | manage config, policy, tokens, backups, users |
| operator | approve/reject, run restore/drill, view health |
| policy_author | create/update/simulate policy bundles |
| auditor | read-only lineage/executions/provenance |
| agent | submit intent/use MCP within scope |
| read_only | health, lineage, execution status |

**Token model:**

```
token_id
tenant_id
actor_id
role
scopes[]
expires_at
created_at
last_used_at
revoked_at
```

**Scopes nên có:**

- `intent:submit`
- `proposal:evaluate`
- `capability:mint`
- `execution:authorize`
- `execution:prepare`
- `execution:execute`
- `execution:verify`
- `execution:compensate`
- `approval:resolve`
- `policy:read`
- `policy:write`
- `provenance:read`
- `admin:tokens`
- `admin:config`
- `backup:run`

**Acceptance security:**

- token read-only không gọi được mutating endpoint.
- agent token không approve được.
- auditor không execute được.
- tenant A không đọc được tenant B.
- revoked token 401.
- expired token 401.
- audit log ghi actor/action/result.

### 3.5 Tenant model

Hiện chưa có tenant model.

Nếu muốn production/enterprise, tenant isolation là bắt buộc.

#### Option 1 — Single-tenant production

Dễ nhất. Phù hợp trước.

One deployment = one tenant

**Pros:**

- ít thay đổi code.
- phù hợp self-hosted.
- security đơn giản.
- production nhanh hơn.

**Cons:**

- không phải SaaS multi-tenant.
- mỗi customer/deployment riêng.

#### Option 2 — Row-level tenant_id

Every table has tenant_id.
Every query filters tenant_id.

**Pros:**

- phù hợp SaaS.
- scale hơn.

**Cons:**

- thay đổi store lớn.
- cần test cross-tenant cho mọi endpoint.
- rất dễ leak nếu quên filter.

#### Option 3 — PostgreSQL Row-Level Security

tenant_id + PG RLS policies.

**Pros:**

- DB-level guard.
- defense-in-depth.

**Cons:**

- phức tạp.
- cần set session tenant context chính xác.
- migrations/RLS tests nghiêm ngặt.

#### Recommendation

Thứ tự nên là:

- **Phase T1:** Single-tenant production hardening
- **Phase T2:** Tenant model ADR
- **Phase T3:** tenant_id in schema + store filters
- **Phase T4:** PostgreSQL RLS as defense-in-depth
- **Phase T5:** multi-tenant production claim

Không nên nhảy thẳng vào multi-tenant trước khi PG production ổn.

### 3.6 Admin/operator UX

#### Hiện trạng

- chủ yếu CLI/docs/runbooks.
- chưa có web admin dashboard.
- chưa có token/user/policy/operator UI.
- ferrumctl có một số chức năng nhưng chưa đủ operator-plane.

#### Operator UX cần có

MVP admin/operator UX nên gồm:

**Screen/API/CLI 1 — System status**

Hiển thị:

- health/ready/deep ready;
- store backend;
- DB health;
- backup age;
- queue depth;
- latest error;
- version/build info.

**Screen/API/CLI 2 — Execution viewer**

Hiển thị:

- intents;
- proposals;
- executions;
- current state;
- rollback contract;
- terminal status;
- provenance chain;
- actor.

Filters:

- state;
- actor;
- time;
- risk tier;
- rollback class;
- policy decision.

**Screen/API/CLI 3 — Approval queue**

Chức năng:

- list pending approvals;
- inspect intent/proposal;
- approve/reject;
- require reason;
- show risk and compensation plan.

**Screen/API/CLI 4 — Policy bundles**

Chức năng:

- list/create/update/delete;
- set active;
- validate;
- simulate;
- diff;
- rollback to previous version.

**Screen/API/CLI 5 — Backup/restore**

Chức năng:

- show latest backup;
- run backup;
- verify backup;
- list backups;
- restore drill mode;
- never allow destructive restore without explicit confirmation.

**Screen/API/CLI 6 — Tokens/actors**

Chức năng:

- list actors/tokens;
- create scoped token;
- revoke token;
- rotate token;
- view last_used_at.

#### Recommended build order

1. Extend ferrumctl first
2. Add admin APIs where needed
3. Add simple web UI/TUI later

**Why:**

- faster;
- easier to test;
- less UX scope creep;
- works for operators immediately.

### 3.7 Policy authoring UX

#### Hiện trạng

- policy bundles có YAML/JSON model.
- CRUD API có.
- validation function có, nhưng chưa đủ UX.
- thiếu simulate/dry-run.
- thiếu template library.
- thiếu policy diff/versioning.

#### Policy UX gaps

| Gap | Priority |
|-----|----------|
| ferrumctl policy validate | P0 |
| ferrumctl policy simulate | P0 |
| policy examples/templates | P0/P1 |
| policy authoring guide | P0 |
| policy diff | P1 |
| policy version history | P1 |
| policy rollback/revert | P1 |
| web editor/rule builder | P2 |

#### Policy authoring flow target

```
Create policy from template
→ validate locally
→ simulate against sample intents
→ apply as inactive bundle
→ run dry-run evaluation
→ set active
→ monitor decisions
→ rollback if needed
```

**Acceptance:**

- policy author can create a 3-rule bundle without reading Rust code.
- invalid policy fails with useful error.
- simulation returns decision and matched rule.
- active policy switch is auditable.
- previous policy can be restored.

### 3.8 Product-facing and user-facing docs

Hiện docs nhiều nhưng thiên về implementation/evidence/runbooks. Cần product-facing docs riêng.

#### Docs cần viết

**1. Landing / “What is FerrumGate?”**

Nội dung:

- FerrumGate là gì.
- Vấn đề nó giải quyết.
- Khi nào nên dùng.
- Khi nào không nên dùng.
- Architecture 1-page.
- Current supported deployment modes.

**2. Quickstart 10 phút**

Flow:

```
install/build
→ start ferrumd local
→ health check
→ submit intent
→ evaluate
→ mint capability
→ prepare
→ execute
→ verify
→ query lineage
```

Nên có cả:

- curl version.
- ferrumctl version.
- MCP version.

**3. Concepts guide**

Giải thích:

- Intent.
- Proposal.
- Policy decision.
- Capability.
- Approval.
- Rollback class.
- Provenance.
- Lineage.
- Adapter.
- R0/R1/R2/R3.

**4. API guide**

- link OpenAPI.
- endpoint lifecycle.
- auth.
- errors.
- examples.

**5. MCP integration guide**

- how to run MCP server.
- sample MCP client config.
- tools list.
- read-only tools.
- lifecycle tools.
- approval tools.
- auth setup.
- security warnings.

**6. Policy authoring guide**

- policy schema.
- examples.
- templates.
- common patterns:
  - allow read-only;
  - require approval for R3;
  - deny out-of-scope fs;
  - quarantine high taint;
  - allow draft-only email.

**7. Adapter guide**

For each adapter:

- supported operations.
- rollback behavior.
- limitations.
- examples.
- risk class mapping.

**8. Operator guide**

- config.
- deployment.
- backup/restore.
- token rotation.
- incident response.
- monitoring.
- SLO/SLA.

**9. Hosted deployment guide**

- systemd deployment.
- Docker Compose.
- Kubernetes/Helm later.
- reverse proxy/TLS.
- PostgreSQL deployment.
- backup/restore.

### 3.9 Demo flows

Cần demo cụ thể, copy-paste được.

**Demo 1 — Governed file write**

```
submit intent: write file
→ evaluate
→ mint capability
→ authorize
→ prepare snapshot
→ execute file write
→ verify
→ query lineage
→ compensate rollback
```

**Acceptance:**

- runs locally;
- no secrets;
- creates temp file;
- cleanup guaranteed.

**Demo 2 — Governed git commit**

```
create temp repo
→ file change
→ submit git commit intent
→ evaluate
→ prepare captures HEAD
→ execute commit
→ verify
→ rollback reset
```

**Demo 3 — Governed SQLite mutation**

```
create temp sqlite DB
→ submit SQL mutation
→ prepare savepoint/schema
→ execute
→ verify
→ compensate restore
```

**Demo 4 — Approval-required R3**

```
submit R3 intent
→ evaluate returns RequireApproval
→ list approvals
→ approve
→ continue execution
→ verify no auto-commit
```

**Demo 5 — MCP agent flow**

```
start ferrumd
→ start ferrum-mcp-server
→ tools/list
→ submit/evaluate/mint/authorize/prepare/execute/verify through MCP
→ query lineage
```

**Demo 6 — Policy simulation**

```
write policy
→ validate
→ simulate against intent
→ see Allow/Deny/RequireApproval
→ set active
```

### 3.10 Hosted deployment story

Hiện có configs/scripts nhưng chưa có coherent hosted deployment story.

#### Deployment modes nên hỗ trợ

**Mode A — Local demo**

```
SQLite in temp/dev mode
auth disabled loopback only
```

Purpose:

- quickstart;
- demos;
- development.

**Mode B — Single-node self-hosted**

```
ferrumd + SQLite persistent
systemd
nginx/caddy TLS
backup timer
```

Purpose:

- conditional pilot;
- small internal deployments.

**Mode C — PostgreSQL self-hosted**

```
ferrumd + PostgreSQL
systemd/docker compose
backup/restore
metrics
```

Purpose:

- production foundation.

**Mode D — Kubernetes**

```
ferrumd Deployment
PostgreSQL external/managed
Secret
ConfigMap
Service
Ingress
PVC if SQLite mode
Prometheus ServiceMonitor
```

Purpose:

- hosted production-like.

#### Deliverables

| Deliverable | Priority |
|-------------|----------|
| docker-compose.demo.yml | P0 |
| docker-compose.postgres-demo.yml | P0 |
| systemd service example | P0 |
| env var reference | P0 |
| deployment guide | P0 |
| Helm chart | P1/P2 |
| Terraform/Pulumi | P2 |
| managed PostgreSQL guide | P1 |
| backup/restore hosted guide | P0 |
| DEP-4 target-host systemd runbook | P0 — target-host runtime evidence captured; not production-ready |
| DEP-6 hosted backup preflight checklist | P0 — hosted SQLite temp-copy restore drill captured; not production-ready |

## 4. Phase-based implementation plan

### Phase 0 — Planning artifacts and definitions

**Goal:** khóa scope, không để roadmap mơ hồ.

**Tasks**

- Viết `production-scope.md`:
  - production-ready nghĩa là gì;
  - conditional pilot khác gì production;
  - real domain deferred nhưng vẫn prerequisite.
- Viết `slo-sla-draft.md`.
- Viết `postgres-production-gap-adr.md`.
- Viết `tenant-security-model-adr.md`.
- Viết `mcp-target-host-validation-plan.md`.
- Viết `product-docs-information-architecture.md`.

**Acceptance**

- Có checklist rõ cho từng phase.
- Mọi claim đều có evidence requirement.
- Không dùng từ production-ready cho đến Phase cuối.

### Phase 1 — PostgreSQL production foundation

**Goal:** biến PostgreSQL từ “local feature-gated implementation” thành backend production-like có evidence.

**Engineering tasks**

1. **Harden PG connection:**
   - statement timeout.
   - idle transaction timeout.
   - reconnect/retry.
   - pool acquire failure handling.
   - circuit breaker/fail-closed readiness.

2. **Add PG metrics:**
   - pool size.
   - active connections.
   - idle connections.
   - acquire wait.
   - acquire failures.
   - DB health.

3. **Add PG alert rules:**
   - DB down.
   - pool saturation.
   - slow acquire.
   - backup stale.
   - replication lag later.

4. **Add schema versioning:**
   - migration table.
   - incremental migrations.
   - idempotent migration runner.

5. **Run PG target/staging drill:**
   - migrate SQLite snapshot to PG.
   - run full lifecycle.
   - backup.
   - restore.
   - verify counts/hash.
   - PG deployment runbook.
   - PG backup/restore runbook refresh.
   - PG operations cadence.
   - PG target evidence artifact template.

**Acceptance gates**

- PG-1 ferrumd starts with postgres DSN.
- PG-2 `/v1/readyz/deep` reports PG health.
- PG-3 migration succeeds with hash/count validation.
- PG-4 backup/restore drill passes.
- PG-5 PG metrics visible in `/v1/metrics`.
- PG-6 PG target evidence artifact created.

### Phase 2 — SLO/SLA and workload evidence

**Goal:** formalize “production acceptable” as measurable targets.

**Tasks**

1. **Define SLOs:**
   - availability;
   - latency;
   - throughput;
   - durability;
   - correctness;
   - security;
   - operator response.

2. **Map existing scripts to SLO checks:**
   - `scripts/stress/run-all.sh`;
   - `scripts/run_real_workload_generator.py`;
   - `scripts/run_g36_workload_wrapper.sh`;
   - `scripts/check_pilot_readiness.py`.

3. **Create SLO validation runbook:**
   - prechecks;
   - workload phases;
   - expected outputs;
   - pass/fail criteria;
   - evidence artifact format.

4. **Run target/staging workload:**
   - baseline;
   - target;
   - spike;
   - cooldown.

5. **Refresh workload model:**
   - assumed vs observed;
   - capacity ceiling;
   - recommended safe limits.

**Acceptance gates**

- SLO-1 SLO/SLA doc exists.
- SLO-2 runbook maps scripts to pass/fail.
- SLO-3 target workload run completed.
- SLO-4 p95/p99 latency under threshold.
- SLO-5 readiness success meets target.
- SLO-6 error rate under threshold.
- SLO-7 evidence artifact reviewed.

### Phase 3 — Target-host MCP/live workload

**Goal:** prove MCP is not just local smoke, but target-host/live governed agent path.

**Tasks**

1. **Adapt MCP lifecycle smoke for target gateway:**
   - configurable gateway URL;
   - bearer token from env;
   - no token printing;
   - TLS target.

2. **Add MCP target evidence mode:**
   - `tools/list`;
   - read-only calls;
   - mutating auth failure test;
   - bounded lifecycle with temp fixtures;
   - lineage query.

3. **Run target-host MCP smoke.**

4. **Add MCP live workload:**
   - repeated small workflows;
   - mix adapters;
   - verify provenance;
   - verify redaction.

5. **Add MCP evidence artifact:**
   - no secrets;
   - pass/fail;
   - request counts;
   - error categories;
   - lineage sample IDs sanitized.

**Acceptance gates**

- MCP-1 target `tools/list` returns 19 tools.
- MCP-2 read-only tools pass against target.
- MCP-3 mutating tools fail closed without auth.
- MCP-4 lifecycle flow passes with auth.
- MCP-5 provenance chain exists.
- MCP-6 redaction/sanitization verified.
- MCP-7 target evidence artifact created.

### Phase 4 — Security and tenant model

**Goal:** move from bearer-only single-operator model to scoped identity model.

**Tasks**

1. **Write security model ADR:**
   - actors;
   - roles;
   - scopes;
   - token lifecycle;
   - tenant model;
   - audit model.

2. **Implement scoped tokens:**
   - token metadata in store;
   - scopes;
   - expiry;
   - revocation;
   - last_used_at.

3. **Add RBAC middleware:**
   - endpoint → required scope mapping.
   - role → scopes mapping.
   - deny by default.

4. **Add actor identity to provenance/audit:**
   - actor_id.
   - tenant_id later.
   - role/scopes maybe not all in event payload.

5. **Add audit log:** ✅ Done 2026-05-21 — minimal append-only audit log with best-effort store append.
   - auth success/failure;
   - admin actions;
   - policy changes;
   - approval resolution;
   - token rotation.

6. **Tenant model:**
   - start with single-tenant production.
   - design tenant_id for future.
   - later add tenant filters in store.
   - eventually PG RLS.

**Acceptance gates**

- SEC-1 read-only token cannot mutate.
- SEC-2 agent token cannot approve.
- SEC-3 auditor token cannot execute.
- SEC-4 revoked token fails.
- SEC-5 expired token fails.
- SEC-6 audit log records admin/policy/approval/token actions.
- SEC-7 tenant ADR approved before implementation.

### Phase 5 — Policy authoring UX

**Goal:** make policy usable without reading Rust/code internals.

**Tasks**

1. **Add CLI:**
   - `ferrumctl policy validate`
   - `ferrumctl policy simulate`
   - `ferrumctl policy apply`
   - `ferrumctl policy diff`
   - `ferrumctl policy rollback`

2. **Add simulation API:**
   - `POST /v1/policy-bundles/simulate`
   - `POST /v1/policy/simulate`

3. **Add policy templates:**
   - read-only safe baseline;
   - require approval for R3;
   - deny external HTTP except allowlist;
   - draft-only email;
   - tenant scoped policy later.

4. **Add policy authoring guide.**

5. **Add policy version history:**
   - store previous versions;
   - active bundle switch audit;
   - rollback previous active bundle.

**Acceptance gates**

- POL-1 invalid policy returns useful error.
- POL-2 simulate returns decision without side effect.
- POL-3 template produces valid policy.
- POL-4 policy switch is auditable.
- POL-5 rollback to previous policy works.

### Phase 6 — Admin/operator UX

**Goal:** operator can run system without spelunking docs/source.

Start with CLI/TUI before web UI.

**Recommended first:**

- `ferrumctl admin status`
- `ferrumctl admin executions`
- `ferrumctl admin approvals`
- `ferrumctl admin tokens`
- `ferrumctl admin backup`
- `ferrumctl admin config`

**Then web dashboard**

MVP screens:

1. System status.
2. Execution viewer.
3. Approval queue.
4. Policy manager.
5. Backup/restore.
6. Tokens/actors.
7. Metrics snapshot.

**Acceptance gates**

- UX-1 operator can view current health/status.
- UX-2 operator can approve/reject without curl.
- UX-3 operator can inspect execution lineage.
- UX-4 operator can rotate/revoke token.
- UX-5 operator can validate/apply policy.
- UX-6 operator can run/verify backup.

### Phase 7 — Product-facing docs and demo flows

**Goal:** make FerrumGate usable by someone outside the project.

**Docs deliverables**

| Doc | Priority |
|-----|----------|
| What is FerrumGate? | P0 |
| FerrumGate in 10 Minutes | P0 |
| Concepts guide | P0 |
| API lifecycle guide | P0 |
| MCP client integration guide | P0 |
| Policy authoring guide | P0 |
| Adapter guide | P1 |
| Operator quickstart | P0 |
| Hosted deployment guide | P0 |
| Troubleshooting guide | P1 |
| Security model guide | P1 |
| SLO/SLA guide | P1 |

**Demo deliverables**

| Demo | Priority |
|------|----------|
| governed file write | P0 |
| governed SQLite mutation | P0 |
| R3 approval flow | P0 |
| MCP lifecycle flow | P0 |
| governed git commit | P1 |
| HTTP mutation/replay | P1 |
| policy simulation | P1 |
| backup/restore demo | P1 |

**Acceptance gates**

- DOC-1 new user can complete quickstart in <30 min.
- DOC-2 every demo runs without secrets.
- DOC-3 docs state production-ready limitations correctly.
- DOC-4 MCP client config example exists.
- DOC-5 policy guide has at least 5 templates/examples.

### Phase 8 — Hosted deployment story

**Goal:** package FerrumGate into reproducible deployment modes.

**Deliverables**

**P0:**

- docker-compose.demo.yml
- docker-compose.postgres.yml or postgres-demo.yml
- configs/examples/ferrumd.service
- configs/examples/ferrumd.env.example
- deployment/local-demo.md
- deployment/single-node-systemd.md
- deployment/postgres-self-hosted.md

**P1:**

- helm chart
- k8s manifests
- prometheus/grafana dashboard integration
- backup cron/timer docs
- managed PostgreSQL guide

**P2:**

- Terraform/Pulumi module
- cloud marketplace style deployment
- zero-downtime upgrade guide

**Acceptance gates**

- DEP-1 docker compose demo starts ferrumd.
- DEP-2 healthz passes after compose up.
- DEP-3 Postgres deployment mode documented and tested.
- DEP-4 systemd unit works with env file. Target-host systemd runtime evidence captured at `docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md`. This is not a production-ready claim.
- DEP-5 Helm install produces ready pod. Live kind cluster install succeeded 2026-05-21; evidence: `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §4. NOT production K8s/HA.
- DEP-6 backup/restore procedure works in hosted mode. Hosted single-node SQLite temp-copy restore drill captured at `docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md`. This is not a production-ready claim.

### Phase 9 — HA/multi-node

**Goal:** move beyond single-node production.

Do not start implementation before:

- PostgreSQL production foundation stable.
- security/tenant model decided.
- SLO metrics available.
- backup/restore evidence exists.

**HA staged plan**

**HA-1 — HA ADR**

Define:

- managed PG vs self-hosted;
- failover strategy;
- replica strategy;
- split-brain prevention;
- leader/writer model;
- read routing;
- migration handling;
- RPO/RTO target.

**HA-2 — Manual failover**

Acceptance:

- primary down detected.
- standby promoted manually.
- ferrumd reconnects or restarts.
- RPO/RTO measured.

**HA-3 — Read replicas**

Acceptance:

- read-only endpoints can use replica.
- writes go to primary.
- readiness shows replica lag.
- stale reads documented.

**HA-4 — Automated failover**

Acceptance:

- failover occurs automatically.
- no split-brain.
- writes resume.
- data consistency verified.
- incident log generated.

**Recommendation**

Do not promise HA too early. A safer claim path is:

```
production-grade single-node PostgreSQL
→ manual failover support
→ read replica support
→ automated HA
```

## 5. Recommended sequencing

### Track A — Production foundation

- A1 PostgreSQL production baseline
- A2 PG connection hardening
- A3 PG metrics/alerts
- A4 PG backup/restore evidence
- A5 PG schema migration discipline

### Track B — Evidence/G2

- B1 SLO/SLA draft
- B2 SLO validation runbook
- B3 sustained target workload
- B4 workload model refresh
- B5 G2 re-signoff, excluding domain for now

### Track C — MCP/live agent path

- C1 target-host MCP smoke
- C2 MCP governed lifecycle evidence
- C3 MCP live workload
- C4 MCP docs/demo

### Track D — Security/product maturity

- D1 scoped tokens/RBAC ADR
- D2 scoped token implementation
- D3 audit log
- D4 tenant model ADR
- D5 tenant implementation later

### Track E — UX/docs/deployment

- E1 quickstart/docs IA
- E2 demo flows
- E3 policy authoring CLI/simulation
- E4 operator CLI
- E5 Docker/systemd deployment story
- E6 Helm/K8s later

## 6. Suggested milestones

### Milestone 0.5 — “Domainless Production-Candidate” ✅ COMPLETE

**Objective:**

Reach Tier 1 (domainless production-candidate) by completing B+C+HA-B engineering evidence without requiring a real owned domain.

**Status:**

- **COMPLETE / ACKNOWLEDGED** on 2026-05-26.
- Operator (BrianNguyen) explicitly authorized Tier 1 acknowledgment.
- End-state artifact: [`docs/implementation-path/artifacts/2026-05-26-domainless-tier1-complete-end-state.md`](./implementation-path/artifacts/2026-05-26-domainless-tier1-complete-end-state.md)
- Completion tracker: [`docs/production-readiness-v2/12-domainless-completion-status.md`](./production-readiness-v2/12-domainless-completion-status.md)

**Scope:**

- **B**: Domainless readiness semantics defined (`domainless production-candidate` is Tier 1; legacy `production-ready` remains Tier 2 only).
- **C**: PostgreSQL local hardening maximized with migration/restore/backup/resume/timer and sustained workload evidence.
- **HA-B**: Local Docker primary/standby streaming replication and manual failover simulation passes with RPO/RTO measured (NOT production HA).

**Exit criteria (all satisfied):**

- B+C+HA-B evidence artifacts exist and are reviewable. ✅
- Operator acknowledges Tier 1 scope and non-claims. ✅
- `production-ready = NO` remains explicit. ✅
- `full G2 = NOT COMPLETE` remains explicit. ✅
- `Block A = WAIVED/CONDITIONAL` remains explicit. ✅
- `PostgreSQL production = NO` remains explicit. ✅
- `HA/multi-node = NO` remains explicit. ✅

**Non-claims (preserved):**

- Tier 1 is **not** production-ready.
- Tier 1 does **not** close Block A.
- Tier 1 does **not** complete full G2.
- Tier 1 does **not** deploy PostgreSQL to production.
- Tier 1 does **not** implement production HA/multi-node; HA-B is local Docker simulation only.
- Tier 1 does **not** require or claim a real owned domain.

See [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](./production-readiness-v2/00a-domainless-readiness-tier.md) for the canonical tiered readiness model.

### Milestone 0.75 — “Tier 1.5 Domainless Production Infrastructure” ✅ COMPLETE

**Objective:**

Reach Tier 1.5 (domainless production infrastructure complete) by completing PostgreSQL production deployment, HA multi-node topology, and automated failover evidence in an operator environment — without requiring a real owned domain. Tier 1.5 is the optional, final intermediate tier; no further subtier may be introduced without a written ADR and explicit operator acknowledgment.

**Status:**

- **COMPLETE / ACKNOWLEDGED** as of 2026-05-27.
- Batch 1 PostgreSQL production deployment evidence complete.
- Batch 2 same-VM HA multi-node topology evidence complete.
- Batch 3 same-VM automated failover evidence complete.
- Operator (BrianNguyen) explicitly authorized Tier 1.5 acknowledgment.
- End-state artifact: [`docs/implementation-path/artifacts/2026-05-27-tier-1-5-complete-end-state.md`](./implementation-path/artifacts/2026-05-27-tier-1-5-complete-end-state.md)
- Canonical definition: [`docs/production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md`](./production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md)
- Completion tracker: [`docs/production-readiness-v2/13-tier-1.5-completion-status.md`](./production-readiness-v2/13-tier-1.5-completion-status.md)

**Scope:**

- **PostgreSQL production deployment**: Target/staging PostgreSQL provisioned, ferrumd starts with production DSN, `/v1/readyz/deep` reports PG health, TLS/SSL DSN validated, PgBouncer operational, backup/restore drill passes, alert rules deployed and validated.
- **HA multi-node topology**: At least two-node PostgreSQL primary/standby streaming replication deployed, read/write routing validated, replication lag measured, fencing/split-brain prevention documented.
- **Automated failover**: Failover without manual `pg_promote`, ferrumd reconnect without manual restart, RTO/RPO measured, no split-brain, at least three drills with evidence.

**Exit criteria:**

- PostgreSQL production deployment evidence complete. ✅
- HA multi-node topology evidence complete. ✅ (same VM primary/standby topology; no multi-host production HA claim)
- Automated failover evidence complete. ✅ (same-VM watchdog + PgBouncer reconnect; no multi-host production HA claim)
- Operator acknowledges Tier 1.5 scope and non-claims. ✅
- `production-ready = NO` remains explicit. ✅ (preserved by framework)
- `full G2 = NOT COMPLETE` remains explicit. ✅ (preserved by framework)
- `Block A = WAIVED/CONDITIONAL` remains explicit. ✅ (preserved by framework)

**Non-claims (preserved):**

- Tier 1.5 is **not** production-ready.
- Tier 1.5 does **not** close Block A.
- Tier 1.5 does **not** complete full G2.
- Tier 1.5 does **not** validate a sustained SLO observation window.
- Tier 1.5 does **not** require or claim a real owned domain.
- Tier 1.5 does **not** replace operator final signoff for Tier 2.

### Milestone 1 — “Production Foundation without Domain”

**Objective:**

PostgreSQL + SLO definitions + workload plan ready.

**Tasks:**

- PG baseline target/staging deployment.
- PG backup/restore drill.
- SLO/SLA draft.
- SLO validation runbook.
- docs updated to state domain deferred.

**Exit criteria:**

- Postgres target readyz pass.
- backup/restore pass.
- SLO doc approved.
- no production-ready claim.

### Milestone 2 — “Live Workload Evidence”

Sustained workload and MCP target-host evidence.

- full G3.6 workload target run.
- stress run mapped to SLO.
- target MCP lifecycle smoke.
- update workload model.
- evidence artifacts.

**Exit criteria:**

- target workload pass.
- MCP target pass.
- SLO metrics pass or gaps documented.
- operator reviews evidence.

### Milestone 3 — “Security Control Plane”

Move beyond single bearer token.

- scoped token model.
- RBAC endpoint mapping.
- token persistence/revocation.
- audit log.
- CLI for token/admin basics.

**Exit criteria:**

- read-only token cannot mutate.
- agent cannot approve.
- revoked token fails.
- audit log records admin actions.

### Milestone 4 — “Policy and Operator UX”

Operators and policy authors can use system without raw curl/source reading.

- `ferrumctl policy validate`.
- `ferrumctl policy simulate`.
- policy templates.
- approval queue CLI/TUI.
- execution viewer.
- backup/status CLI.
- policy authoring guide.

**Exit criteria:**

- operator can approve/reject via CLI.
- policy author can validate/simulate/apply policy.
- execution lineage visible from CLI.

### Milestone 5 — “Productization”

External user can understand, deploy, demo, and evaluate FerrumGate.

- landing doc.
- quickstart.
- concepts guide.
- MCP guide.
- adapter guide.
- demo scripts.
- Docker Compose demo.
- systemd/postgres deployment guide.

**Exit criteria:**

- new user can complete quickstart in <30 min.
- demo flows reproducible.
- docs do not overclaim readiness.

### Milestone 6 — “HA and Production Evidence”

Prepare final production-ready claim path.

- HA ADR.
- manual failover runbook.
- read replica plan.
- failover drill.
- 7–30 day SLO evidence window.
- final evidence pack.

**Exit criteria:**

- RPO/RTO measured.
- failover drill pass.
- SLO met over evidence window.
- operator signs final production posture.
- real domain added by user before final claim.

## 7. Must-have vs nice-to-have

### Must-have for credible production path

| Item | Why |
|------|-----|
| PostgreSQL production deployment | foundation for scale/HA/tenant |
| PG backup/restore evidence | durability requirement |
| SLO/SLA definitions | production claim needs measurable targets |
| sustained target workload | prove stability |
| target-host MCP lifecycle | prove agent path |
| scoped auth/RBAC | bearer-only is too weak |
| audit log | operator/security accountability |
| policy simulation | safe policy authoring |
| product quickstart/docs | adoption and correct use |
| deployment story | reproducibility |
| real domain eventually | final production-ready prerequisite |

### Nice-to-have / later

| Item | Why later |
|------|-----------|
| web admin dashboard | CLI can cover MVP |
| visual policy builder | templates + simulate first |
| Helm chart | Docker/systemd first |
| Terraform/Pulumi | after deployment model stabilizes |
| automated HA | manual failover first |
| enterprise evidence bundle | after core production path |

## 8. Implementation priority recommendation

Nếu bạn muốn tiến nhanh và không lãng phí công:

### Nên làm trước

1. SLO/SLA doc + SLO validation runbook
2. PostgreSQL target/staging deployment + backup/restore evidence
3. Target-host MCP smoke
4. Sustained workload evidence
5. Scoped token/RBAC design
6. Product quickstart + demo flows

### Không nên làm trước

1. Web dashboard lớn
2. Helm/Terraform phức tạp
3. Multi-tenant implementation ngay
4. Automated HA ngay
5. Enterprise/SOC2-style evidence pack

**Lý do:** những mục “không nên làm trước” có rủi ro scope bloat cao và phụ thuộc vào PG/security/SLO foundation.

## 9. Rủi ro chính

### Risk 1 — PostgreSQL production chưa đủ hardening

Nếu chuyển sang PG mà không có pool metrics/reconnect/backup/restore/timeout, sẽ dễ có production incident.

**Mitigation:**

- harden PG trước workload lớn.
- add PG metrics.
- run restore drill.

### Risk 2 — SLO/SLA định nghĩa quá sớm hoặc quá tham vọng

Nếu đặt SLO 99.99% khi chưa có HA, claim sẽ không thực tế.

**Mitigation:**

- SLO theo phase:
  - pilot SLO;
  - single-node PG SLO;
  - HA SLO.

### Risk 3 — MCP D-1 scope creep

Không nên để MCP layer tự implement lại policy/rollback.

**Mitigation:**

- MCP chỉ gọi gateway governance endpoints.
- policy/capability/rollback vẫn nằm ở gateway/core.

### Risk 4 — Tenant isolation bug

Nếu multi-tenant mà thiếu filter/RLS ở một endpoint là catastrophic.

**Mitigation:**

- tenant model ADR.
- tenant isolation tests cho mọi endpoint.
- PG RLS as defense-in-depth.

### Risk 5 — Product docs overclaim

Docs marketing dễ lỡ claim production-ready.

**Mitigation:**

- docs phải có “Deployment status matrix”.
- mỗi mode ghi rõ supported/unsupported.

## 10. Đề xuất structure roadmap mới

Nên tạo roadmap mới dạng:

```
docs/production-readiness-v2/
  00-scope-and-nonclaims.md
  01-slo-sla.md
  02-postgres-production-plan.md
  03-target-mcp-live-workload-plan.md
  04-security-tenant-model-adr.md
  05-policy-authoring-ux-plan.md
  06-admin-operator-ux-plan.md
  07-product-docs-plan.md
  08-hosted-deployment-plan.md
  09-ha-roadmap.md
  10-evidence-checklist.md
```

Mỗi file nên có:

- Goal
- Current state
- Gaps
- Implementation tasks
- Acceptance criteria
- Evidence required
- Owner
- Status
- Non-claims

## 11. Kế hoạch triển khai thực tế theo thứ tự ngắn gọn

### Batch 1 — Docs/planning only

- Create production readiness v2 docs.
- Define SLO/SLA.
- Define PG production checklist.
- Define MCP target-host checklist.
- Define security/tenant ADR.
- Define product docs IA.

**Output:**

clear roadmap, no code risk

### Batch 2 — PostgreSQL hardening

- Add PG timeouts/reconnect/metrics.
- Add PG backup/restore target drill.
- Add schema versioning.
- Add PG CI/manual gate.

**Output:**

PostgreSQL moves from local-capable to production-candidate

### Batch 3 — Workload/MCP evidence

- Adapt MCP smoke for target.
- Run sustained workload.
- Collect evidence.
- Refresh workload model.
- Operator signoff update.

**Output:**

G2 excluding domain becomes evidence-backed

### Batch 4 — Security/RBAC

- Scoped tokens.
- RBAC.
- token persistence/revocation.
- audit log.
- CLI admin basics.

**Output:**

Bearer-only global power removed

### Batch 5 — UX/docs/demo

- quickstart.
- demo scripts.
- policy validate/simulate.
- operator CLI.
- deployment docs.

**Output:**

FerrumGate becomes usable by non-author users

### Batch 6 — HA/SLO evidence

- HA ADR.
- manual failover.
- read replica plan.
- 7–30 day SLO evidence.
- final production evidence pack.

**Output:**

ready to combine with real domain for production-ready review

## 12. Final recommendation

Nếu bỏ qua real domain tạm thời, hướng triển khai tối ưu là:

1. Formalize SLO/SLA and evidence gates.
2. Harden PostgreSQL production path.
3. Prove target-host MCP/live workload.
4. Add scoped security/RBAC and audit.
5. Build policy/operator UX through ferrumctl first.
6. Write product-facing docs and demo flows.
7. Add hosted deployment packaging.
8. Only then design/implement HA.
9. Khi bạn bổ sung real domain, re-run L1–L5 + G2 re-signoff + final evidence pack.

Nhận định thẳng:

FerrumGate không còn thiếu “core idea” hay “core architecture”. Những gì còn thiếu để tiến lên production-grade chủ yếu là production operations, PostgreSQL hardening, target-host workload evidence, security/tenant control plane, operator/product UX, và SLO/SLA evidence. Nếu làm đúng thứ tự, dự án có đường đi rõ ràng từ conditional RC sang production-candidate. Real domain có thể để sau, nhưng production-ready claim vẫn phải chờ domain + revalidation + signoff.

## 13. Related docs update plan

The following docs should be created or updated to align with this roadmap. **This roadmap does not edit them directly;** it lists them as follow-up work.

### Immediate reconciliation tasks

Before creating new documents, reconcile this roadmap with existing planning packs to avoid duplicate sources of truth:

| Task | Reason | Priority |
|------|--------|----------|
| ✅ **Decided**: `docs/production-readiness-v2/` supplements (does not supersede) `docs/ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/`. The legacy pack remains historical/baseline reference; production-readiness-v2 is the active post-pilot execution/evidence planning layer. | Prevent forked roadmap hierarchies | P0 |
| ✅ **Done**: Backlinks confirmed in `README.md`, `AGENTS.md`, `01-current-state.md`, and `67-production-readiness-roadmap.md`; `00-start-here.md` link fixed to clickable markdown. | Make the new roadmap discoverable | P0 |
| ✅ **Done**: All v2 planning docs (00–10, runbook) link back to `docs/ROADMAP.md` and `00-scope-and-nonclaims.md`. | Preserve traceability | P0 |
| ✅ **Done**: Naming crosswalk added in §"Naming crosswalk" above; distinguishes ROADMAP Phase 0–9, priority labels P0–P3, and legacy Q1–Q4 quarters | Avoid phase-number ambiguity | P1 |
| Keep `production-ready = NO`, `full G2 = NOT COMPLETE`, and `Block A = WAIVED/CONDITIONAL` in all updated docs until domain/revalidation/signoff are complete | Prevent readiness overclaim | P0 |

### New docs to create

| File | Purpose | Priority |
|------|---------|----------|
| `docs/production-readiness-v2/00-scope-and-nonclaims.md` | Lock scope and explicit non-claims | P0 |
| `docs/production-readiness-v2/01-slo-sla.md` | SLO/SLA definitions and runbooks | P0 |
| `docs/production-readiness-v2/02-postgres-production-plan.md` | PG hardening plan and acceptance gates | P0 |
| `docs/production-readiness-v2/03-target-mcp-live-workload-plan.md` | MCP target-host validation plan | P0 |
| `docs/production-readiness-v2/04-security-tenant-model-adr.md` | Security and tenant model ADR | P0 |
| `docs/production-readiness-v2/05-policy-authoring-ux-plan.md` | Policy authoring UX plan | P1 |
| `docs/production-readiness-v2/06-admin-operator-ux-plan.md` | Admin/operator UX plan | P1 |
| `docs/production-readiness-v2/07-product-docs-plan.md` | Product-facing docs information architecture | P1 |
| `docs/production-readiness-v2/08-hosted-deployment-plan.md` | Hosted deployment packaging plan | P1 |
| `docs/production-readiness-v2/09-ha-roadmap.md` | HA roadmap and ADR | P2 |
| `docs/production-readiness-v2/10-evidence-checklist.md` | Evidence checklist per phase | P1 |
| `docs/production-readiness-v2/11-blockers-and-unblock-plan.md` | Active blockers, unblock plan, and operator decision packet | P0 |
| `docs/guides/quickstart.md` | 10-minute quickstart | P0 |
| `docs/guides/concepts.md` | Concepts guide | P0 |
| `docs/guides/mcp-integration.md` | MCP integration guide | P0 |
| `docs/guides/policy-authoring.md` | Policy authoring guide | P0 |
| `docs/guides/operator.md` | Operator guide | P0 |
| `docs/guides/hosted-deployment.md` | Hosted deployment guide | P0 |
| `docs/guides/adapter-reference.md` | Adapter reference | P1 |
| `docs/guides/slo-sla.md` | SLO/SLA guide | P1 |
| `docs/guides/security-model.md` | Security model guide | P1 |
| `docs/guides/troubleshooting.md` | Troubleshooting guide | P1 |

### Existing docs to update

| File | Update reason |
|------|---------------|
| `docs/PRODUCTION_NOTES.md` | Add deferred domain note; refresh SLO/HA posture |
| `docs/implementation-path/67-production-readiness-roadmap.md` | Cross-reference new v2 docs; mark superseded sections |
| `docs/implementation-path/122-completion-roadmap-and-hardening-tracker.md` | Update tracker entries for Phase 0–9 |
| `README.md` | Ensure no production-ready overclaim; link quickstart |
| `CONTRIBUTING.md` | Reference new planning docs |

### Deliverable artifacts to create

| Artifact | Owner | Priority |
|----------|-------|----------|
| SLO validation runbook | Track B | P0 |
| PG target/staging evidence artifact | Track A | P0 |
| MCP target evidence artifact | Track C | P0 |
| Workload model refresh | Track B | P0 |
| Security model ADR | Track D | P0 |
| Tenant model ADR | Track D | P1 |
| Policy templates library | Track E | P1 |
| Demo scripts (copy-paste runnable) | Track E | P0 |
| Docker Compose demo | Track E | P0 |
| systemd unit example | Track E | P0 |
| Helm chart (basic) | Track E | P2 |
| HA ADR | Track A | P2 |
| Final evidence pack | All tracks | P1 |
