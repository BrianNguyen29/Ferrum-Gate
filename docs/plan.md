Nên cải thiện FerrumGate theo hướng:
Không biến FerrumGate thành bản sao Microsoft AGT hay OpenAI Tunnel.  
FerrumGate nên giữ định vị riêng: execution-governance gateway cho MCP/agentic operations — nơi mọi hành động có side-effect phải đi qua policy, capability, approval, rollback class, provenance và verification.

## Trạng thái hiện tại (Current State Callout)

> **FerrumGate hiện tại là execution-governance gateway, chưa phải production-ready.**
>
> - `production-ready` = **NO**
> - `Tier 2` = **NOT COMPLETE**
> - Real domain / public endpoint chưa có; mọi SLO evidence hiện tại (nếu có) đều là **domainless**.
> - Full G2 (sign-off / general availability criteria) chưa hoàn thành.
> - HA-4 unattended automated failover = **NOT COMPLETE**; hiện tại chỉ hỗ trợ manual/operator-controlled failover.
> - Sustained SLO window (7–30 ngày liên tục) chưa có evidence đầy đủ.
>
> **Kế hoạch này là strategic execution checklist**, không phải bằng chứng rằng các mục đã hoàn thành. Mỗi mục phải có artifact xác thực riêng.
>
> **SSOT Boundary Declaration**
>
> - `docs/plan.md` is the **single source of truth (SSOT)** for strategic execution checklist, task ordering, priorities, and phase gates.
> - It is **NOT** canonical evidence that tasks are complete by itself; each item requires its own artifact.
> - It is **NOT** the canonical non-claims/readiness boundary document; see [`docs/security/non-claims.md`](./security/non-claims.md).
> - It is **NOT** the canonical production evidence/readiness artifact set; see [`docs/production-readiness-v2/`](./production-readiness-v2/) and [`docs/implementation-path/artifacts/`](./implementation-path/artifacts/).
> - [`docs/ROADMAP.md`](./ROADMAP.md) remains historical/legacy/reference roadmap material unless explicitly cited by current plan/evidence docs.

## 1. Định vị nên giữ

FerrumGate nên tự định vị như sau:

FerrumGate is a self-hosted execution governance gateway for MCP/agentic operations. It turns agent/tool actions into a policy-evaluated, capability-bounded, approval-aware, rollback-classified, and provenance-tracked execution lifecycle.

Dịch ngắn:
FerrumGate là lớp kiểm soát thực thi cho agent/MCP: trước khi tool chạy, FerrumGate kiểm tra policy, cấp capability, yêu cầu approval nếu cần, phân loại rollback, ghi provenance và hỗ trợ verify/compensate.

### Mô hình layer so sánh (Comparison-Driven Layer Model)

| Layer | Hệ thống tham chiếu | Vai trò FerrumGate |
|-------|---------------------|--------------------|
| Secure Transport | OpenAI Secure MCP Tunnels, Cloudflare Tunnel, Tailscale | **Integrate** — tích hợp, không clone |
| Execution Governance | **FerrumGate** | **Core differentiation** — build sâu |
| Identity / Compliance Middleware | Microsoft AGT-style capabilities | **Selectively borrow / integrate** — học hỏi chọn lọc, không clone ecosystem |

FerrumGate nên chơi sâu ở layer giữa.

## 2. Nên bổ sung gì trước

### P0 — Bắt buộc nếu muốn FerrumGate production-credible

#### 2.1. Scoped tokens + RBAC thực sự
- **Status:** `TODO/VERIFY`
- **Action:** Build
- **Evidence/Target doc:** `docs/security/scoped-tokens-rbac.md`, implementation, integration tests
- **Mô tả:** FerrumGate đã có Bearer token và security model docs. Cần scoped token/RBAC đầy đủ.
- **Nội dung cần có:**
  - Token có scope rõ ràng: `intent:submit`, `proposal:evaluate`, `capability:mint`, `execution:prepare`, `execution:execute`, `execution:verify`, `approval:resolve`, `policy:read`, `policy:write`, `provenance:read`, `admin:tokens`, `admin:config`
  - Role mapping: `admin`, `operator`, `policy_author`, `auditor`, `agent`, `read_only`
  - Deny-by-default middleware: endpoint không map scope → reject; token thiếu scope / hết hạn / revoked → reject
  - Token lifecycle: create, revoke, rotate, list redacted, `last_used_at`, `expires_at`
- **Vì sao:** Gap thực tế lớn nhất so với AGT; không cần build identity system phức tạp; vẫn là FerrumGate-native.

#### 2.2. Policy simulation / dry-run API
- **Status:** `TODO/VERIFY`
- **Action:** Build
- **Evidence/Target doc:** `docs/api/policy-simulation.md`, `POST /v1/policy/simulate`, CLI `ferrumctl policy simulate`
- **Mô tả:** API nhận intent/actor/action/target và trả về decision mà không mint capability thật, không tạo execution thật, không chạm adapter, không side-effect.
- **Vì sao:** AGT có policy tooling mạnh; FerrumGate cần UX tương đương nhưng gắn với lifecycle riêng; high-leverage, ít scope creep.

#### 2.3. Sustained SLO evidence window
- **Status:** `TODO/VERIFY` — cần bắt đầu chạy
- **Action:** Operate / Document
- **Evidence/Target doc:** `docs/implementation-path/artifacts/YYYY-MM-DD-sustained-slo-window-evidence.md`
- **Mô tả:** Chạy liên tục 7 ngày tối thiểu, tốt hơn 30 ngày. Thu thập: readiness uptime, deep readiness uptime, p95/p99 latency, error rate, throughput, PostgreSQL pool health, backup freshness, replication lag (nếu có), restart/reconnect events, operator intervention log.
- **Non-claims:** Nếu chưa có real domain thì ghi rõ `domainless evidence only`; `Tier 2 = NOT COMPLETE`; `production-ready = NO`.
- **Vì sao:** FerrumGate đã có nhiều bounded rehearsal nhưng thiếu sustained window là gap quan trọng nhất cho production credibility.

### P1 — Nâng cao trust và tích hợp

#### 3.1. Secure MCP Tunnel integration guide
- **Status:** `DOCS COMPLETE`
- **Action:** Integrate / Document
- **Evidence/Target doc:** `docs/guides/secure-mcp-tunnel-integration.md`
- **Mô tả:** Không tự build tunnel. Viết guide tích hợp OpenAI Secure MCP Tunnels / Cloudflare / Tailscale vào FerrumGate. Ghi rõ: tunnel bảo vệ connectivity, tunnel không thay thế policy, FerrumGate bảo vệ execution, không mở inbound MCP public nếu không có auth/tunnel, không log bearer token.
- **Vì sao:** OpenAI Tunnels bổ trợ trực tiếp; docs-only, ít rủi ro.

#### 3.2. OIDC/JWT federation
- **Status:** `TODO/VERIFY`
- **Action:** Integrate
- **Evidence/Target doc:** `docs/security/oidc-jwt-federation.md`, config mẫu, integration tests
- **Mô tả:** Không tự làm identity provider. Hỗ trợ Google, GitHub, Azure AD/Entra, Keycloak. OIDC issuer → JWT → FerrumGate auth middleware → map claims thành `actor_id + roles + scopes`.
- **Vì sao:** Học từ AGT mà không clone; enterprise users cần SSO; FerrumGate vẫn giữ core là execution governance.

#### 3.3. Tamper-evident audit / provenance hardening
- **Status:** `TODO/VERIFY`
- **Action:** Build minimal
- **Evidence/Target doc:** `docs/architecture/tamper-evident-audit-design.md`, `ferrumctl audit verify`
- **Mô tả:** FerrumGate đã có provenance/evidence discipline tốt nhưng chưa đủ cryptographic audit. Thiết kế trước, sau đó implement tối thiểu: canonical event serialization, per-event hash, previous_hash, chain root, verification command.
- **Later:** Merkle root per time window, signed checkpoint, export verification bundle, optional WORM sink, optional external anchoring.
- **Không cần ngay:** full SOC2 automation, SIEM platform, compliance SaaS.
- **Vì sao:** Điểm AGT mạnh; FerrumGate có provenance rồi nên thêm cryptographic verifiability là bước tự nhiên; giữ đúng định vị execution evidence.

#### 3.4. Formal STRIDE threat model
- **Status:** `TODO/VERIFY`
- **Action:** Document
- **Evidence/Target doc:** `docs/security/threat-model-stride.md`
- **Mô tả:** Trust boundaries: Human/Operator → FerrumGate; Agent/MCP Client → FerrumGate MCP Server; MCP Server → Gateway; Gateway → PDP; Gateway → Adapter; Gateway → Store; Gateway → Provenance Ledger; Operator → Approval Resolve. Mỗi boundary map STRIDE (Spoofing, Tampering, Repudiation, Information disclosure, Denial of service, Elevation of privilege). Ghi rõ: existing controls, gaps, deferred controls, non-claims.
- **Vì sao:** AGT có threat model rất mạnh; FerrumGate cần tài liệu tương tự nhưng focused vào execution plane; docs-only nhưng nâng trust đáng kể.

### P2 — Nâng cao interoperability và agent identity

#### 4.1. Agent identity nhẹ bằng Ed25519
- **Status:** `TODO/VERIFY`
- **Action:** Build minimal
- **Evidence/Target doc:** `docs/security/agent-identity-ed25519.md`, implementation, tests
- **Mô tả:** Không cần DID/trust mesh ngay. Thiết kế tối thiểu: `agent_id`, `public_key`, `key_fingerprint`, `allowed_scopes`, `created_at`, `revoked_at`. Request envelope có `agent_id`, `timestamp`, `nonce`, `body_hash`, `signature`. FerrumGate verify: signature hợp lệ, timestamp không quá lệch, nonce chưa dùng, agent chưa revoked, requested scope hợp lệ.
- **Vì sao:** Lấy phần tốt từ AGT (cryptographic agent identity) mà không ôm DID/trust scoring phức tạp; rất hợp với capability model của FerrumGate.

#### 4.2. Streamable HTTP MCP support
- **Status:** `TODO/VERIFY`
- **Action:** Build
- **Evidence/Target doc:** `docs/mcp/streamable-http-mcp.md`, implementation, compatibility tests
- **Mô tả:** Hiện FerrumGate MCP chủ yếu stdio. Thêm Streamable HTTP, SSE response support nếu cần, health/ready endpoints cho MCP server, auth middleware rõ ràng, reverse proxy/tunnel compatibility.
- **Vì sao:** OpenAI tunnel và nhiều MCP deployment remote cần HTTP transport; nhưng không nên làm trước RBAC/policy simulation.

#### 4.3. Operator evidence UX
- **Status:** `TODO/VERIFY` — TUI base đã có nhưng evidence commands chưa
- **Action:** Build
- **Evidence/Target doc:** `docs/operator/evidence-ux.md`, CLI/TUI implementation
- **Mô tả:** Thêm evidence UX vào `ferrumctl`: `evidence snapshot`, `evidence slo-window start/status/finalize`, `audit verify`, `readiness report`. TUI hiển thị: current tier, non-claims, SLO window state, pending approvals, last audit verify, readiness blockers.
- **Vì sao:** Khác biệt riêng của FerrumGate; không sa vào web dashboard lớn; tăng operator trust.

## 3. Không nên build gì (Anti-pattern / "Do not build")

| Anti-pattern | Lý do không làm | Thay vào đó |
|--------------|-----------------|-------------|
| **Tunnel service clone** (FerrumGate Tunnel, Cloud Connector, Reverse Tunnel Service) | OpenAI / Cloudflare / Tailscale làm tốt hơn; build tunnel phân tán khỏi core governance; tunnel là security-sensitive, dễ tạo surface rủi ro mới | FerrumGate works behind secure tunnels |
| **Full IdP / AGT clone** (multi-framework governance SDK lớn, agent marketplace, full trust mesh, DID ecosystem, policy SDK cho mọi framework, compliance suite lớn, GRC/SOC2 automation platform) | AGT đã đi hướng đó; clone AGT sẽ làm mất định vị; FerrumGate nhỏ hơn nhưng có lợi thế execution lifecycle cụ thể | Chọn lọc học hỏi identity/compliance middleware; giữ execution governance làm core |
| **Multi-tenant SaaS sớm** (tenant billing, workspace SaaS, org hierarchy phức tạp, PostgreSQL RLS everywhere, tenant admin UI) | Chưa đủ hardening single-tenant; multi-tenant kéo theo rủi ro data isolation, auth phức tạp | Single-tenant production hardening trước; `tenant_id` reserved field/design; không claim multi-tenant |
| **Web dashboard lớn ngay** | TUI + CLI hiện đủ cho operator; web dashboard dễ kéo theo auth phức tạp, session management, CSRF, RBAC UI, frontend maintenance, audit UI, approvals mutation UI | Ưu tiên CLI/TUI evidence UX trước |
| **HA-4 unattended failover sớm** | Rủi ro split-brain, false positive promotion, fencing không đủ chắc, data loss nếu automation sai | Manual/operator-controlled failover first; unattended failover deferred; `HA-4 = NOT COMPLETE` |

## 4. Feature matrix

| Capability | Priority | Action | Status | Evidence / Target doc |
|------------|----------|--------|--------|----------------------|
| Scoped token / RBAC | P0 | Build | `TODO/VERIFY` | `docs/security/scoped-tokens-rbac.md` + implementation + tests |
| Policy simulation / dry-run | P0/P1 | Build | `TODO/VERIFY` | `docs/api/policy-simulation.md` + API + CLI |
| Sustained SLO evidence window | P0 | Operate / Document | `TODO/VERIFY` | `docs/implementation-path/artifacts/YYYY-MM-DD-sustained-slo-window-evidence.md` |
| Secure MCP tunnel integration guide | P1 | Integrate / Document | `DOCS COMPLETE` | `docs/guides/secure-mcp-tunnel-integration.md` |
| OIDC / JWT federation | P1 | Integrate | `PHASE 4.4 COMPLETE` | `docs/security/oidc-jwt-federation.md` + offline JWT validation + live JWKS fetch/cache + config loading + tests |
| STRIDE threat model | P1 | Document | `TODO/VERIFY` | `docs/security/threat-model-stride.md` |
| Tamper-evident audit | P1/P2 | Build minimal | `TODO/VERIFY` | `docs/architecture/tamper-evident-audit-design.md` + `ferrumctl audit verify` |
| Agent Ed25519 identity | P2 | Build minimal | `TODO/VERIFY` | `docs/security/agent-identity-ed25519.md` + implementation |
| Streamable HTTP MCP | P2 | Build | `TODO/VERIFY` | `docs/mcp/streamable-http-mcp.md` + implementation |
| mTLS service-to-service design | P2 | Document | `DESIGN COMPLETE` | `docs/security/mtls-service-mesh.md` |
| mTLS service-to-service native impl | P2 | Build | `DEFERRED` | Deferred until multi-node cross-host topology |
| OWASP Agentic AI Top 10 mapping | P2 | Document | `TODO/VERIFY` | `docs/security/owasp-agentic-ai-mapping.md` |
| Operator evidence UX | P2 | Build | `TODO/VERIFY` | `docs/operator/evidence-ux.md` + CLI/TUI |
| Web dashboard | Later | Defer | `DEFERRED` | Không có target doc cho đến khi single-tenant ổn định |
| Tunnel service | Never / Avoid | Do not build | `AVOID` | Không build |
| AGT-like SDK ecosystem | Avoid | Do not build | `AVOID` | Không build |
| Multi-tenant SaaS | Later | Defer | `DEFERRED` | `tenant_id` reserved field only |

## 5. Ứng dụng thực tế nên nhắm tới

### 5.1. Governed MCP server for private engineering tools
Use case: Agent wants to edit repo / run migration / call internal API → FerrumGate evaluates risk → requires approval if R3 → mints short-lived capability → executes through adapter → verifies side effect → logs provenance. Đây là wedge tốt nhất.

### 5.2. Safe CI/CD agent gate
FerrumGate làm gate cho agent trong CI: agent đề xuất thay đổi → classify rollback → policy check branch/path/scope → high-risk deploy requires approval → provenance lưu lại. Không cần thay CI system. Chỉ làm governance gate.

### 5.3. Operator-controlled database/tool execution
FerrumGate kiểm soát: SQL mutation, backup/restore, migration, admin action, external API mutation. Rất hợp với R0–R3 model.

### 5.4. Private MCP behind secure tunnel
Không mở MCP public. Topology: ChatGPT / MCP client → Secure MCP Tunnel → FerrumGate MCP → FerrumGate Gateway → governed execution. Đây là ứng dụng trực tiếp từ OpenAI Secure MCP Tunnels.

### 5.5. Evidence-first production readiness assistant
FerrumGate không chỉ chạy tool, mà còn trả lời: Hành động này có được phép không? Ai approve? Rollback class gì? Capability nào được dùng? Side-effect đã verify chưa? Evidence artifact ở đâu? Có đủ điều kiện production claim không? Đây là điểm khác biệt lớn so với AGT/Tunnels.

## 6. Thứ tự thực hiện tốt nhất (Top 10 + Hành động tiếp theo ngay)

### Top 10 execution order
Nếu chỉ chọn 10 việc, đề xuất:
1. Scoped token / RBAC implementation
2. `ferrumctl admin token lifecycle`
3. Policy simulation API
4. Policy simulation CLI
5. Secure MCP tunnel integration guide
6. STRIDE threat model
7. Start 7-day sustained SLO evidence window
8. Tamper-evident audit design
9. OIDC/JWT design + minimal implementation
10. Agent identity Ed25519 design

Thứ tự này giúp FerrumGate:
- An toàn hơn
- Dễ chứng minh hơn
- Enterprise-credible hơn
- Vẫn giữ định vị execution governance
- Không bị kéo thành tunnel/AGT clone

### Hành động tiếp theo ngay (Immediate Next Actions)
1. **Phase 0 — Alignment & Doc Hygiene:** Hoàn thành checklist Phase 0 trước khi bắt đầu implement P0.
2. **Scoped tokens + RBAC:** Bắt đầu implement deny-by-default middleware và scope mapping.
3. **Policy simulation API:** Thiết kế endpoint `POST /v1/policy/simulate` và CLI tương ứng.
4. **SLO evidence window:** Triển khai monitoring stack và bắt đầu thu thập metrics liên tục.

## 7. Checklist đầy đủ theo Phase

### Phase 0: Alignment / Non-claims / Doc hygiene
**Mục tiêu:** Đảm bảo mọi tuyên bố đều trung thực, không overclaim, và tài liệu nền tảng sẵn sàng trước khi build.

- [x] **0.1** Xác nhận lại toàn bộ `docs/` không có claim `production-ready`, `Tier 2 complete`, `GA`, `enterprise-ready`. (Owner: Operator / Type: Operate)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase0.1-overclaim-audit-evidence.md`](./implementation-path/artifacts/2026-05-28-phase0.1-overclaim-audit-evidence.md)
- [x] **0.2** Thêm / cập nhật `docs/security/non-claims.md` ghi rõ những gì FerrumGate **không** làm được hiện tại. (Owner: Operator / Type: Document)
  - Evidence: [`docs/security/non-claims.md`](./security/non-claims.md)
- [x] **0.3** Kiểm tra toàn bộ README / marketing docs không có real domain giả định. (Owner: Operator / Type: Operate)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase0.3-domain-assumption-audit-evidence.md`](./implementation-path/artifacts/2026-05-28-phase0.3-domain-assumption-audit-evidence.md)
- [x] **0.4** Đánh dấu `HA-4 = NOT COMPLETE` trong mọi tài liệu liên quan đến high availability. (Owner: Operator / Type: Operate)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase0.4-ha4-audit-evidence.md`](./implementation-path/artifacts/2026-05-28-phase0.4-ha4-audit-evidence.md)
- [x] **0.5** Đánh dấu `Sustained SLO window = NOT COMPLETE` nếu chưa có evidence đủ 7 ngày. (Owner: Operator / Type: Operate)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase0.5-slo-sustained-window-audit-evidence.md`](./implementation-path/artifacts/2026-05-28-phase0.5-slo-sustained-window-audit-evidence.md)
- [x] **0.6** Tạo / cập nhật `docs/plan.md` (file này) làm single source of truth cho strategic execution checklist. (Owner: Operator / Type: Document)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase0.6-plan-ssot-update-evidence.md`](./implementation-path/artifacts/2026-05-28-phase0.6-plan-ssot-update-evidence.md)

**Success criteria:** Không còn claim sai ở bất kỳ đâu trong repo.  
**Evidence artifact:** `docs/security/non-claims.md`, `docs/plan.md`, scan log.

### Phase 1: Execution governance hardening
**Mục tiêu:** Xây dựng các primitive kiểm soát thực thi cốt lõi.

- [x] **1.1** Implement scoped token model với danh sách scope cụ thể. (Owner: Dev / Type: Build)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase1.1-scoped-token-model-evidence.md`](./implementation-path/artifacts/2026-05-28-phase1.1-scoped-token-model-evidence.md)
- [x] **1.2** Implement deny-by-default middleware trên toàn bộ API surface. (Owner: Dev / Type: Build)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase1.2-deny-by-default-evidence.md`](./implementation-path/artifacts/2026-05-28-phase1.2-deny-by-default-evidence.md)
- [x] **1.3** Implement token lifecycle: create, revoke, rotate, list redacted, `last_used_at`, `expires_at`. (Owner: Dev / Type: Build)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase1.3-token-lifecycle-evidence.md`](./implementation-path/artifacts/2026-05-28-phase1.3-token-lifecycle-evidence.md)
- [x] **1.4** Thêm `ferrumctl admin token lifecycle` commands. (Owner: Dev / Type: Build)
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase1.4-ferrumctl-token-lifecycle-evidence.md`](./implementation-path/artifacts/2026-05-28-phase1.4-ferrumctl-token-lifecycle-evidence.md)
- [x] **1.5** Viết `docs/security/scoped-tokens-rbac.md` mô tả model, role mapping, và flow. (Owner: Dev / Type: Document)
  - Evidence: [`docs/security/scoped-tokens-rbac.md`](./security/scoped-tokens-rbac.md), [`docs/implementation-path/artifacts/2026-05-28-phase1.5-scoped-tokens-rbac-doc-evidence.md`](./implementation-path/artifacts/2026-05-28-phase1.5-scoped-tokens-rbac-doc-evidence.md)
- [x] **1.6** Viết integration tests cho RBAC deny-by-default. (Owner: Dev / Type: Build)
- [x] **1.7** Thiết kế và implement Policy simulation API (`POST /v1/policy/simulate`). (Owner: Dev / Type: Build)
- [x] **1.8** Thêm `ferrumctl policy simulate --file intent.json`. (Owner: Dev / Type: Build)
- [x] **1.9** Viết `docs/api/policy-simulation.md` mô tả input/output và constraints. (Owner: Dev / Type: Document)
- [ ] **1.10** Bắt đầu chạy 7-day sustained SLO evidence window và thiết lập monitoring. (Owner: Operator / Type: Operate) — **STARTED 2026-05-28; NOT COMPLETE** until 7–30 days of evidence are collected.
  - Evidence: [`docs/implementation-path/artifacts/2026-05-28-phase1.10-slo-window-start-evidence.md`](./implementation-path/artifacts/2026-05-28-phase1.10-slo-window-start-evidence.md)
  - Daily log template: [`docs/implementation-path/artifacts/TEMPLATE-slo-daily-evidence-log.md`](./implementation-path/artifacts/TEMPLATE-slo-daily-evidence-log.md)

**Success criteria:** Deny-by-default hoạt động trên 100% API; policy simulation trả về decision đúng mà không side-effect; SLO window đang chạy. **Phase 1.10 sustained evidence collection is in progress and must not be marked complete until the observation period finishes.**  
**Evidence artifact:** Integration test reports, `docs/security/scoped-tokens-rbac.md`, `docs/api/policy-simulation.md`, monitoring dashboard screenshot / log.

### Phase 2: Production credibility / evidence
**Mục tiêu:** Cung cấp bằng chứng và artifact để chứng minh độ tin cậy.

- [ ] **2.1** Hoàn thành và đóng gói 7-day (hoặc 30-day) sustained SLO evidence artifact. (Owner: Operator / Type: Operate)
- [ ] **2.2** Viết `docs/implementation-path/artifacts/YYYY-MM-DD-sustained-slo-window-evidence.md` đầy đủ các mục: start/end, environment, workload, criteria, metrics, incidents, exclusions, conclusion. (Owner: Operator / Type: Document)
  - Use [`docs/implementation-path/artifacts/TEMPLATE-slo-daily-evidence-log.md`](./implementation-path/artifacts/TEMPLATE-slo-daily-evidence-log.md) for per-day entries during the window.
- [x] **2.3** Thiết kế tamper-evident audit: canonical serialization, per-event hash, previous_hash, chain root. (Owner: Dev / Type: Document) — Design implemented as hash chain with SHA-256 canonical serialization; Merkle root deferred.
- [x] **2.4** Implement `ferrumctl audit verify`. (Owner: Dev / Type: Build) — Remote verify via `GET /v1/admin/audit/verify` implemented; local DB direct-verify deferred.
- [x] **2.5** Viết `docs/architecture/tamper-evident-audit-design.md`. (Owner: Dev / Type: Document) — Doc written; covers design, scope, legacy handling, non-claims.
- [x] **2.6** Viết `docs/security/threat-model-stride.md` với trust boundaries và STRIDE mapping đầy đủ. (Owner: Security / Type: Document) — Doc written; 8 trust boundaries mapped with controls/gaps/deferred items.
- [ ] **2.7** Nếu có real domain, chạy validation trên domain thật và cập nhật status; nếu không, ghi rõ `domainless`. (Owner: Operator / Type: Operate)

**Success criteria:** Có artifact SLO hoàn chỉnh; threat model được review; audit verify chạy được trên local/test data.  
**Evidence artifact:** `docs/implementation-path/artifacts/...-sustained-slo-window-evidence.md`, `docs/security/threat-model-stride.md`, `docs/architecture/tamper-evident-audit-design.md`, `ferrumctl audit verify` demo.

### Phase 3: Secure transport integration docs
**Mục tiêu:** Tích hợp với lớp transport bên ngoài mà không build lại.

- [x] **3.1** Viết `docs/guides/secure-mcp-tunnel-integration.md` với topology và nguyên tắc rõ ràng. (Owner: Dev / Type: Document)
- [x] **3.2** Tạo deployment example cho OpenAI Secure MCP Tunnel + FerrumGate. (Owner: Dev / Type: Document)
- [x] **3.3** Tạo deployment example cho Cloudflare Tunnel + FerrumGate. (Owner: Dev / Type: Document)
- [x] **3.4** Tạo deployment example cho Tailscale + FerrumGate. (Owner: Dev / Type: Document)
- [x] **3.5** Kiểm tra và ghi rõ: không log bearer token qua tunnel; không mở inbound public. (Owner: Security / Type: Document)

**Success criteria:** Người dùng có thể follow guide để deploy FerrumGate behind tunnel mà không cần hỏi thêm.  
**Evidence artifact:** `docs/guides/secure-mcp-tunnel-integration.md`, `docs/security/secure-mcp-tunnel-review.md`, example configs, validation log.

### Phase 4: Identity federation and agent identity
**Mục tiêu:** Tích hợp identity provider bên ngoài và cung cấp agent identity nhẹ.

- [x] **4.1** Thiết kế OIDC/JWT federation flow và config schema (`auth.oidc` config). (Owner: Dev / Type: Document)
  - Evidence: [`docs/security/oidc-jwt-federation.md`](./security/oidc-jwt-federation.md)
- [x] **4.2** Sync `AuthMode` enums (single source in `ferrum-proto`, re-export via `ferrum-gateway`), add `Oidc` variant with `Display`/`FromStr`, attach auth middleware that fails closed (401) until Phase 4.3, document JWT dependency strategy. (Owner: Dev / Type: Build)
- [x] **4.3** Implement JWT validation middleware (offline/static JWKS) và role mapping từ JWT claims sang FerrumGate roles/scopes. (Owner: Dev / Type: Build)
  - Evidence: `crates/ferrum-gateway/src/server.rs` OIDC middleware + tests, `crates/ferrum-gateway/src/state.rs` `OidcConfig`/`KeyMaterial`
- [x] **4.4** Implement live JWKS fetch/cache (`reqwest`) và OIDC config loading from TOML/env. (Owner: Dev / Type: Build)
  - Evidence: `crates/ferrum-gateway/src/state.rs` `OidcJwksCache`, `crates/ferrum-gateway/src/server.rs` JWKS fallback, `bins/ferrumd/src/main.rs` OIDC config parsing, tests
- [x] **4.5** Thiết kế agent identity Ed25519: schema, request envelope, verification flow. (Owner: Dev / Type: Document)
  - Evidence: [`docs/security/agent-identity-ed25519.md`](./security/agent-identity-ed25519.md)
- [x] **4.6** Implement agent registry và signature verification. (Owner: Dev / Type: Build)
  - Evidence: `crates/ferrum-store/src/sqlite/agents.rs`, `crates/ferrum-store/src/postgres/agents.rs`, `crates/ferrum-gateway/src/server.rs` agent auth middleware + tests, `crates/ferrum-proto/src/agent.rs`
- [x] **4.7** Implement `ferrumctl admin agents register/list/revoke`, gateway admin endpoints `POST/GET/DELETE /v1/admin/agents`, `admin:agents` scope mapping, and audit entries for register/revoke. (Owner: Dev / Type: Build) — **COMPLETE**
  - Evidence: `crates/ferrum-proto/src/agent.rs` (RegisterAgentRequest/Response/AgentListResponse), `crates/ferrum-gateway/src/server.rs` (handlers + scope mapping + tests), `bins/ferrumctl/src/main.rs` + `client.rs` (CLI + client methods)

> **OIDC hardening notes (post-4.4):** Future `iat` rejection added to JWT validation; missing `iat` is tolerated. OIDC authentication failures emit sanitized `AuthFailed` audit entries by default (actor_id=`unknown`, no token/header logged). JWKS cache age exposed as `ferrumgate_oidc_jwks_cache_age_seconds` in `/v1/metrics`. All changes are tested.

**Success criteria:** Operator có thể đăng nhập qua OIDC; agent có thể ký request và FerrumGate verify thành công.  
**Evidence artifact:** `docs/security/oidc-jwt-federation.md`, `docs/security/agent-identity-ed25519.md`, integration tests.

### Phase 5: Audit / compliance hardening
**Mục tiêu:** Làm cứng audit trail và chuẩn bị các mapping compliance.

- [x] **5.1** Implement Merkle root per time window cho audit log. (Owner: Dev / Type: Build)
  - Evidence: Domain-separated SHA-256 Merkle tree (`0x00` leaf / `0x01` internal), hourly UTC-aligned windows, odd-count duplication, deterministic `id ASC` ordering, excludes legacy entries without `content_hash`. `audit_merkle_roots` table with idempotent cache. SQLite v11 + Postgres v5 migrations. Gateway endpoints `GET /v1/admin/audit/merkle-verify` and `GET /v1/admin/audit/merkle-roots` (scope `admin:audit`). CLI commands `ferrumctl admin audit merkle-verify` and `merkle-roots`. Tests: Merkle algorithm (1/2/3 leaves), store compute/cache/list, gateway endpoint auth/pagination, CLI parse.
- [x] **5.2** Implement signed checkpoint. (Owner: Dev / Type: Build)
  - Evidence: Ed25519-signed checkpoint over Merkle root per hourly window; canonical SHA-256 payload hash; `audit_checkpoints` table; gateway endpoints `POST /v1/admin/audit/checkpoints`, `GET /v1/admin/audit/checkpoints/{window_start}/verify`, `GET /v1/admin/audit/checkpoints`; CLI commands `ferrumctl admin audit checkpoint-sign`, `checkpoint-verify`, `checkpoint-list`. SQLite v12 + Postgres v6 migrations. Tests: create+verify, tampered-root rejection, list+pagination, CLI parse. See [`docs/implementation-path/artifacts/2026-05-29-phase5.2-signed-checkpoints-evidence.md`](./implementation-path/artifacts/2026-05-29-phase5.2-signed-checkpoints-evidence.md)
- [x] **5.3** Implement audit export bundle (`ferrumctl audit export`). (Owner: Dev / Type: Build)
  - Evidence: `GET /v1/admin/audit-logs/export` (requires `admin:audit` scope); supports `ndjson` (default), `json`, and `csv`; bounded pagination with 10,000-row max; `since`/`until` date filters added to store layer and list endpoint; `ferrumctl admin audit export` with filters and output path/stdout; tests for store date filters, gateway export formats/auth, and CLI args.
- [x] **5.4** Viết `docs/security/owasp-agentic-ai-mapping.md` map OWASP Agentic AI Top 10 vào controls của FerrumGate. (Owner: Security / Type: Document)
  - Evidence: [`docs/security/owasp-agentic-ai-mapping.md`](./security/owasp-agentic-ai-mapping.md), [`docs/implementation-path/artifacts/2026-05-29-phase5.4-owasp-agentic-ai-mapping-evidence.md`](./implementation-path/artifacts/2026-05-29-phase5.4-owasp-agentic-ai-mapping-evidence.md)
  - Note: Dedicated OWASP Agentic AI Top 10 is not finalized; mapping uses official OWASP LLM Top 10 v2.0 (2025) as interim baseline. Remap required when Agentic list publishes.
- [x] **5.5** Kiểm tra không có hardcoded secrets trong codebase qua scan. (Owner: Security / Type: Operate)
  - Evidence: [`docs/security/secret-scan-report.md`](./security/secret-scan-report.md), [`scripts/run_secret_scan.sh`](../../scripts/run_secret_scan.sh)
  - Result: PASS (0 findings) on 2026-05-29

**Success criteria:** Audit bundle có thể export và verify ngoài hệ thống; OWASP mapping đầy đủ.  
**Evidence artifact:** `docs/security/owasp-agentic-ai-mapping.md`, audit export demo, secret-scan report.

### Phase 6: MCP interoperability
**Mục tiêu:** Hỗ trợ HTTP transport và schema compatibility.

- [x] **6.1** Implement Streamable HTTP MCP transport skeleton (`POST /mcp`, `GET /mcp` 405, CLI args, `tokio::task::spawn_blocking` around blocking client). (Owner: Dev / Type: Build)
- [ ] **6.2** Thêm SSE response support nếu cần. (Owner: Dev / Type: Build)
- [x] **6.3** Health/ready endpoints cho MCP server included in 6.1 skeleton (`GET /health`, `GET /ready`). (Owner: Dev / Type: Build)
- [x] **6.4** `docs/mcp/streamable-http-mcp.md` included in 6.1 skeleton documentation. (Owner: Dev / Type: Document)
- [x] **6.5** Viết compatibility tests cho MCP tool schema/version. (Owner: Dev / Type: Build)
- [x] **6.6** Viết private MCP deployment guide. (Owner: Dev / Type: Document)
  - Evidence: [`docs/mcp/private-deploy.md`](./mcp/private-deploy.md)
- [x] **6.7a** Thiết kế mTLS service-to-service (design doc). (Owner: Security / Type: Document)
  - Evidence: [`docs/security/mtls-service-mesh.md`](./security/mtls-service-mesh.md)
  - Non-claims: `production-ready = NO`; `Tier 2 = NOT COMPLETE`; native mTLS **not implemented**.
- [ ] **6.7b** Triển khai native mTLS trong ferrumd / ferrum-mcp-server. (Owner: Dev / Type: Build) — **DEFERRED** until multi-node cross-host topology exists.

**Success criteria:** MCP client remote có thể kết nối qua HTTP; compatibility test pass; private deploy guide dùng được.  
**Evidence artifact:** `docs/mcp/streamable-http-mcp.md`, [`docs/mcp/private-deploy.md`](./mcp/private-deploy.md), compatibility test reports.

### Phase 7: Operator evidence UX
**Mục tiêu:** Nâng cao trải nghiệm operator qua CLI/TUI.

- [ ] **7.1** Implement `ferrumctl evidence snapshot`. (Owner: Dev / Type: Build)
- [ ] **7.2** Implement `ferrumctl evidence slo-window start/status/finalize`. (Owner: Dev / Type: Build)
- [ ] **7.3** Implement `ferrumctl readiness report`. (Owner: Dev / Type: Build)
- [ ] **7.4** Cập nhật TUI hiển thị: current tier, non-claims, SLO window state, pending approvals, last audit verify, readiness blockers. (Owner: Dev / Type: Build)
- [ ] **7.5** Viết `docs/operator/evidence-ux.md`. (Owner: Dev / Type: Document)

**Success criteria:** Operator chạy 1 lệnh là biết trạng thái sẵn sàng của hệ thống.  
**Evidence artifact:** `docs/operator/evidence-ux.md`, CLI demo, TUI screenshots.

### Phase 8: Deferred / Explicit non-goals
**Mục tiêu:** Ghi rõ những gì bị hoãn và tại sao.

- [ ] **8.1** Tạo / cập nhật `docs/roadmap/deferred.md` ghi rõ: multi-tenant SaaS, web dashboard, unattended HA-4, WASM sandbox, full compliance automation, agent marketplace, DID/trust-score mesh. (Owner: Operator / Type: Document)
- [ ] **8.2** Đảm bảo mỗi deferred item có lý do rõ ràng và điều kiện để reconsider. (Owner: Operator / Type: Document)
- [ ] **8.3** Đánh dấu `Tunnel service = Do not build` trong tất cả tài liệu roadmap. (Owner: Operator / Type: Document)
- [ ] **8.4** Đánh dấu `AGT-like SDK ecosystem = Do not build` trong tất cả tài liệu roadmap. (Owner: Operator / Type: Document)

**Success criteria:** Không có ambiguity về những gì đang bị hoãn; mọi stakeholder đều hiểu tại sao.  
**Evidence artifact:** `docs/roadmap/deferred.md`, review log.

## 8. Kết luận

Nên cải thiện FerrumGate theo nguyên tắc:

- **Build:** execution governance primitives (scoped tokens, policy simulation, tamper-evident audit, agent identity, operator evidence UX)
- **Integrate:** identity provider (OIDC/JWT) và secure transport (tunnel)
- **Document:** threat model, compliance mapping, non-claims, integration guides
- **Defer:** SaaS, tunnel service, web dashboard, broad SDK ecosystem, unattended HA-4

Ba việc đáng làm nhất ngay:
1. Scoped tokens + RBAC
2. Policy simulation / dry-run
3. Secure MCP Tunnel integration guide + STRIDE threat model

Và việc vận hành cần bắt đầu song song:
7-day sustained SLO evidence window

FerrumGate có định vị riêng tốt nhất nếu trở thành:
**The execution-governance layer for MCP agents — not the tunnel, not the agent framework, not the identity provider.**
