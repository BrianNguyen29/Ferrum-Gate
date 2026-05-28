# 11 — Blockers and Unblock Plan

> **Status**: Planning artifact. Tracks the 7 active blockers/open items that gate further production-path progress.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-28
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Executive Summary

One item remains open (`BLK-A-DOM`) between the domainless intermediate tiers and Tier 2 (production-ready / domain-backed). The other six blockers have been unblocked or completed as of 2026-05-21. This doc orders them, assigns owners, states prerequisites, and lists the exact next action for each. It does not claim production-ready status or full G2 closure. See [`00a-domainless-readiness-tier.md`](./00a-domainless-readiness-tier.md) for the canonical tiered readiness model.

**Conditional signoff**: BrianNguyen authorized conditional re-signoff for single-node SQLite pilot scope on 2026-05-21. Full G2 closure remains NOT COMPLETE.

The seven blockers:

| # | Blocker ID | Item | Owner | Status |
|---|------------|------|-------|--------|
| 1 | **BLK-SLO-RAT** | SLO operator ratification | Operator | ✅ RATIFIED FOR VALIDATION BASELINE — 2026-05-20 |
| 2 | **BLK-SLO-TGT** | SLO target-host workload validation | Engineering | ✅ UNBLOCKED — canonical SLO Run #3 passed (max-valid config) 2026-05-21; Runs #1/#2 failed and documented as failure evidence; full certification NOT claimed for default/tuned configs |
| 3 | **BLK-SEC-PH4** | Phase 4 scoped token / RBAC model | Engineering + Operator | ✅ IMPLEMENTED — operator decisions approved; SEC-1–SEC-5 + TTL enforcement complete |
| 4 | **BLK-UX-4** | UX-4 token rotate / revoke CLI | Engineering | ✅ IMPLEMENTED — `ferrumctl admin tokens` CLI complete |
| 5 | **BLK-MCP-TGT** | Phase 3 MCP target-host smoke | Engineering | ✅ UNBLOCKED — target-mode MCP smoke passed 15/15 on 2026-05-21 |
| 6 | **BLK-DEP-5** | DEP-5 Helm / K8s packaging | Engineering | ✅ LIVE KIND PASS — `helm lint` + `helm template` passed 2026-05-21; live kind cluster install succeeded 2026-05-21; NOT production K8s/HA |
| 7 | **BLK-A-DOM** | Real owned domain / Block A full closure | Operator | ☐ WAIVED/CONDITIONAL — real domain still required for Tier 2 (production-ready); Tier 1 (domainless production-candidate) does not require real domain |

---

## Blocker taxonomy

| Axis | Engineering-owned | Operator-required |
|------|-------------------|-------------------|
| **Blocked on operator input** | — | BLK-A-DOM |
| **Unblocked / completed** | BLK-SLO-TGT, BLK-MCP-TGT, BLK-SEC-PH4, BLK-UX-4, BLK-DEP-5 | BLK-SLO-RAT |
| **Remains open** | — | BLK-A-DOM |

---

## Ordered todo list

### 🔧 Now / local-safe (engineering can start immediately)

| # | Todo | Blocker ID | Owner | Prerequisites | Acceptance Criteria | Evidence Required |
|---|------|------------|-------|---------------|---------------------|-------------------|
| N.1 | Scaffold Helm chart directory and `Chart.yaml` | BLK-DEP-5 | Engineering | None | ✅ `helm lint` passes on scaffold; no live cluster required | `docs/implementation-path/artifacts/2026-05-20-dep5-helm-scaffold-evidence.md` |
| N.2 | Define K8s manifest set (Deployment, Service, Secret, optional Ingress/HPA) in `helm/ferrumgate/templates/` | BLK-DEP-5 | Engineering | N.1 | ✅ Templates render with `helm template`; no syntax errors | Same artifact §N.2 |
| N.3 | Add local `kind` or `minikube` smoke test script (dry-run; no live cluster required) | BLK-DEP-5 | Engineering | N.2 | `helm template` + `kubeconform` or equivalent passes | Same artifact §N.3 |
| N.4 | Document DEP-5 non-claims and prerequisites in `08-hosted-deployment-plan.md` | BLK-DEP-5 | Engineering | N.1 | Doc updated with Helm scope, P1 priority, and Block A conditional disclaimer | `08-hosted-deployment-plan.md` diff |

### 🛑 Operator-required (cannot proceed without operator action or decision)

| # | Todo | Blocker ID | Owner | Prerequisites | Acceptance Criteria | Evidence Required |
|---|------|------------|-------|---------------|---------------------|-------------------|
| O.1 | Operator reviews and ratifies SLO/SLA draft (`01-slo-sla.md`) | BLK-SLO-RAT | Operator | Draft doc exists | Operator replies with ratification or change requests; signed acknowledgment artifact created | `artifacts/YYYY-MM-DD-slo-ratification-signoff.md` |
| O.2 | Operator provides valid target bearer token for staging workload runs | BLK-SLO-TGT, BLK-MCP-TGT | Operator | Target host deployed and reachable | Engineering can run `check_pilot_readiness.py` and get 200 on `/v1/approvals?limit=1` | `artifacts/YYYY-MM-DD-target-token-handoff-evidence.md` (sanitized, no secret values) |
| O.3 | Operator answers tenant / OIDC / scoped-token decision packet (see §Operator Decision Packet below) | BLK-SEC-PH4 | Operator | `04-security-tenant-model-adr.md` reviewed | Operator selects tenant model option (T1 single-tenant), approves RBAC scope set, and approves or defers OIDC | `artifacts/YYYY-MM-DD-security-model-operator-decisions.md` |
| O.4 | Operator procures real owned domain and configures DNS A record → `34.158.51.8` | BLK-A-DOM | Operator | Domain purchased | `dig +short <domain>` returns `34.158.51.8`; HTTPS 200 from target host | `artifacts/YYYY-MM-DD-block-a-domain-evidence.md` |
| O.5 | Operator re-signs G2.1–G2.8 after real domain + new evidence | BLK-A-DOM | Operator | O.4 complete; SLO target evidence + MCP target evidence available | `54-operator-signoff-packet.md` updated with new evidence references | `artifacts/YYYY-MM-DD-g2-resignoff-evidence.md` |

### 🔓 Post-unblock (engineering work that becomes unblocked after operator actions above)

| # | Todo | Blocker ID | Owner | Unblock Condition | Acceptance Criteria | Evidence Required |
|---|------|------------|-------|-------------------|---------------------|-------------------|
| P.1 | Run SLO target-host workload per `slo-validation-runbook.md` | BLK-SLO-TGT | Engineering | O.1 (ratification) + O.2 (token) | Abbreviated target workload executed 2026-05-21; full SLO certification deferred | `artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §3 |
| P.2 | Run MCP target-host smoke (Layer 1–3) per `03-target-mcp-live-workload-plan.md` | BLK-MCP-TGT | Engineering | O.2 (token) | MCP-1 through MCP-7 pass; artifact created with no secrets | `artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 |
| P.3 | Implement scoped token store schema and RBAC middleware | BLK-SEC-PH4 | Engineering | O.3 (decisions) | ✅ COMPLETE — SEC-1 through SEC-6 pass in automated tests | `artifacts/2026-05-20-scoped-token-implementation-evidence.md` + `artifacts/2026-05-21-sec6-audit-log-implementation-evidence.md` |
| P.4 | Implement `ferrumctl admin tokens` CLI (list/create/revoke/rotate) | BLK-UX-4 | Engineering | P.3 complete | ✅ COMPLETE — UX-4 acceptance criteria met; test output captured | `artifacts/2026-05-20-scoped-token-implementation-evidence.md` §5 |
| P.5 | Implement `POST /v1/admin/tokens` and `DELETE /v1/admin/tokens/{id}` APIs | BLK-UX-4 | Engineering | P.3 complete | ✅ COMPLETE — APIs return correct 201/204/401/403; test coverage added | Same artifact §P.5 |
| P.6 | Run DEP-5 Helm install against a live cluster (optional; can remain local-only until operator cluster available) | BLK-DEP-5 | Engineering | N.1–N.3 + optional live cluster | `helm lint` and `helm template` passed 2026-05-21; live cluster install blocked by kind timeout | `artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §5 |
| P.7 | Re-run L1–L5 target bridge + G2 re-signoff with real domain | BLK-A-DOM | Engineering + Operator | O.4 + O.5 | All target bridge checks pass; operator signs updated signoff packet | `artifacts/YYYY-MM-DD-block-a-closure-evidence.md` |

### 📦 Deferred (out of current scope; do not start)

| # | Todo | Blocker ID | Owner | Rationale |
|---|------|------------|-------|-----------|
| D.1 | HA/multi-node (Phase 9) | — | Engineering + Operator | Manual multi-host failover/failback drills, GCP fencing mechanism, detection-only watchdog, and host B redundancy evidence captured 2026-05-27. HA-4 unattended automated failover and external endpoint cutover remain NOT COMPLETE. See `09-ha-roadmap.md`, `artifacts/2026-05-27-ha-phase9-multihost-drill-evidence.md`, and `artifacts/2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md` |
| D.2 | Web admin dashboard / TUI | — | Engineering | CLI-first; P2 deferred per `06-admin-operator-ux-plan.md` |
| D.3 | Terraform / Pulumi module | — | Engineering | After Helm/K8s model stabilizes per `08-hosted-deployment-plan.md` |
| D.4 | Multi-tenant implementation (T3–T5) | — | Engineering | After single-tenant production hardening and PG baseline per `04-security-tenant-model-adr.md` |
| D.5 | Automated failover | — | Engineering + Operator | After manual failover drill and read replica support per `09-ha-roadmap.md` |
| D.6 | Enterprise / SOC2-style evidence pack | — | Operator | After all core production path items complete |

---

## Operator decision packet

The following questions must be answered by the operator to unblock BLK-SEC-PH4 and BLK-SLO-RAT. Answers should be recorded in a signed artifact.

### Tenant / OIDC / scoped token decisions (unblocks BLK-SEC-PH4 → BLK-UX-4)

| # | Question | Options | Default recommendation |
|---|----------|---------|------------------------|
| Q1 | Which tenant model for first production posture? | Option 1 — Single-tenant production (one deployment = one tenant) / Option 2 — Row-level `tenant_id` / Option 3 — PostgreSQL RLS | **Option 1** — minimal code change, fits self-hosted, defers SaaS complexity |
| Q2 | Is OIDC/JWT/SSO required for the first production posture, or can it be deferred? | Required now / Deferred to later phase | **Deferred** — bearer + scoped tokens first; OIDC later |
| Q3 | Which RBAC roles should be enabled in the first implementation? | Full set (admin, operator, policy_author, auditor, agent, read_only) / Subset | **Full set** — the scope set in `04-security-tenant-model-adr.md` is already minimal viable |
| Q4 | Should token revocation be immediate (in-memory deny list) or durable (store-backed revocation table)? | Immediate / Durable | **Durable** — store-backed `revoked_at` column; survives restart |
| Q5 | What is the maximum token TTL acceptable for service-account tokens? | 24h / 7d / 30d / 90d | **90d** with mandatory rotation reminder |
| Q6 | Do you approve the scoped token model and scope list in `04-security-tenant-model-adr.md` §Scopes? | Approve / Request changes | Approve recommended |

### SLO ratification decisions (unblocks BLK-SLO-RAT → BLK-SLO-TGT)

| # | Question | Options | Default recommendation |
|---|----------|---------|------------------------|
| Q7 | Do you ratify the pilot-tier SLO targets in `01-slo-sla.md` as the validation baseline? | Ratify / Request changes / Defer | Ratify as baseline; upgrade targets after first evidence run |
| Q8 | What observation window should be used for the first validation run? | 1 day / 7-day rolling / 30-day rolling | **7-day rolling** for pilot; 30-day for production-candidate |
| Q9 | Who will operate the target host during the SLO validation run? | Engineering runs with operator observer / Operator runs with engineering support | Engineering runs; operator reviews artifact |

### Domain decisions (unblocks BLK-A-DOM)

| # | Question | Options | Default recommendation |
|---|----------|---------|------------------------|
| Q10 | Do you commit to procuring a real owned domain for production-ready closure? | Yes / No / Deferred indefinitely | Yes — but timeline is operator-owned |
| Q11 | If yes, what is the target timeline for DNS A record configuration? | < 30 days / < 90 days / > 90 days | Operator to fill |

---

## Per-blocker detail

### BLK-SLO-RAT — SLO operator ratification

- **Blocker ID**: `BLK-SLO-RAT`
- **Owner**: Operator
- **Status**: ✅ RATIFIED FOR VALIDATION BASELINE — abbreviated target-host run completed; full certification review deferred
- **Prerequisites**: `01-slo-sla.md` draft exists; `slo-validation-runbook.md` exists.
- **Blocked on**: Nothing for baseline ratification; full certification still needs operator-approved canonical-duration run and review.
- **Acceptance criteria**:
  - Operator has read `01-slo-sla.md` and `slo-validation-runbook.md`.
  - Operator replies with ratification or a numbered list of requested changes.
  - Ratification artifact is signed and stored in `docs/implementation-path/artifacts/`.
- **Evidence required**: `artifacts/2026-05-20-slo-ratification-signoff.md` plus post-run target evidence.
- **Exact next action**: Operator reviews abbreviated target evidence; engineering reruns canonical-duration SLO validation only if requested.
- **Downstream impact**: Baseline ratification has unblocked abbreviated target workload evidence; full certification remains deferred.

### BLK-SLO-TGT — SLO target-host workload validation

- **Blocker ID**: `BLK-SLO-TGT`
- **Owner**: Engineering
- **Status**: ✅ UNBLOCKED — canonical SLO Run #3 passed (max-valid config) 2026-05-21; Runs #1/#2 documented as failure evidence
- **Prerequisites**: BLK-SLO-RAT ratified; valid target bearer token available.
- **Blocked on**: Nothing — token installed and canonical workloads executed.
- **Acceptance criteria**:
  - Pilot readiness check passes before and after run.
  - All five workload phases execute (baseline → low → target → spike → cooldown).
  - p99 latencies recorded for evaluate, mint, execute pipeline.
  - 5xx rate < 1%; 429 rate < 5%.
  - Evidence artifact created and marked PASS/FAIL per run.
- **Evidence achieved (canonical)**:
  - **Run #1 (default config)**: FAIL — 429 rate 46.8% (1114/2382), target p99 403.424 ms.
  - **Run #2 (tuned config, 20/500)**: FAIL — 429 rate 73.4% (1795/2444), target p99 382.939 ms.
  - **Run #3 (max-valid config, 1000/10000)**: PASS — 0 errors, 0 429s, target p99 394.054 ms, readyz all 200.
  - Evidence artifact: `artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §3.
- **Evidence required for full certification**: `artifacts/YYYY-MM-DD-slo-target-full-evidence.md` (default config pass without max-valid override, operator signoff)
- **Exact next action**: Operator review of canonical evidence; full SLO certification for default config deferred until operator requests.
- **Downstream impact**: Enables workload model refresh and conditional G2 re-signoff.

### BLK-SEC-PH4 — Phase 4 scoped token / RBAC model

- **Blocker ID**: `BLK-SEC-PH4`
- **Owner**: Engineering + Operator
- **Status**: ✅ COMPLETE / SIGNED — prep, implementation, SEC-6 evidence, and operator evidence review complete
- **Prerequisites**: `04-security-tenant-model-adr.md` reviewed by operator. **Prep artifacts created 2026-05-20:**
  - `12-endpoint-to-scope-mapping.md` — endpoint-to-scope mapping
  - `13-token-api-contract.md` — token API contract
  - `14-ferrumctl-admin-tokens-cli-spec.md` — ferrumctl CLI surface spec
  - `15-revocation-durability-tradeoff.md` — revocation durability tradeoff note
  - `16-operator-shortcut-decision-packet.md` — condensed operator decision packet
- **Blocked on**: No operator decision blocker remains; implementation completed under `2026-05-20-security-model-operator-decisions.md`.
- **Acceptance criteria**:
  - ✅ Operator decision artifact signed.
  - ✅ Scoped token store schema implemented (SQLite migration 007; PostgreSQL 001_initial.sql updated).
  - ✅ RBAC middleware denies by default (`auth_middleware` in `server.rs`).
  - ✅ SEC-1 through SEC-5 automated tests pass.
  - ✅ SEC-6 (audit log) implemented 2026-05-21 — minimal append-only audit log with best-effort store append.
- **Evidence required**: `artifacts/2026-05-20-security-model-operator-decisions.md` + `artifacts/2026-05-20-scoped-token-implementation-evidence.md` + `artifacts/2026-05-21-sec6-audit-log-implementation-evidence.md` + `artifacts/2026-05-27-phase4-security-operator-signoff.md`
- **Exact next action**: None for Phase 4 evidence review. Remaining future security work (tenant model beyond T1, OIDC/SSO, compliance-grade audit logging) is post-v1/Tier 2+ scope.
- **Downstream impact**: Unblocks BLK-UX-4 (token CLI).

### BLK-UX-4 — UX-4 token rotate / revoke CLI

- **Blocker ID**: `BLK-UX-4`
- **Owner**: Engineering
- **Status**: ✅ IMPLEMENTED — `ferrumctl admin tokens` CLI complete
- **Prerequisites**: BLK-SEC-PH4 implementation complete.
- **Blocked on**: None.
- **Acceptance criteria**:
  - ✅ `ferrumctl admin tokens list/create/revoke/rotate` wired to admin APIs.
  - ✅ CLI parse tests pass.
  - 📝 Demo recording deferred to operator validation session.
- **Evidence required**: `bins/ferrumctl/src/main.rs` `AdminTokensCommand` + client methods + `artifacts/2026-05-20-scoped-token-implementation-evidence.md` §5
- **Exact next action**: Operator validation of `ferrumctl admin tokens` commands against staging.
- **Downstream impact**: Enables operator token lifecycle management without curl.

### BLK-MCP-TGT — Phase 3 MCP target-host smoke

- **Blocker ID**: `BLK-MCP-TGT`
- **Owner**: Engineering
- **Status**: ✅ UNBLOCKED — target-mode MCP smoke passed 15/15 on 2026-05-21
- **Prerequisites**: Valid target bearer token; target gateway reachable.
- **Blocked on**: Nothing — token installed and smoke executed.
- **Acceptance criteria**:
  - MCP-1: `tools/list` returns 19 tools against target.
  - MCP-2: 9 read-only tools pass.
  - MCP-3: Mutating tools fail closed without auth.
  - MCP-4: Lifecycle flow passes with auth.
  - MCP-5: Provenance chain exists.
  - MCP-6: Redaction/sanitization verified.
  - MCP-7: Evidence artifact created with no secrets.
- **Evidence achieved**:
  - `run_mcp_lifecycle_smoke.sh --gateway-url https://ferrumgate.duckdns.org` passed 15/15.
  - 19 tools validated; target gateway reachable; lifecycle submit/evaluate/mint/list returned results.
  - Sanitized log at `/tmp/opencode/ferrumgate-target-mcp-smoke-20260521.log`.
  - Evidence artifact: `artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4.
- **Exact next action**: Operator review of target MCP evidence.
- **Downstream impact**: Proves agent path on target; feeds G2 re-signoff.

### BLK-DEP-5 — DEP-5 Helm / K8s packaging

- **Blocker ID**: `BLK-DEP-5`
- **Owner**: Engineering
- **Status**: ✅ LIVE KIND PASS — `helm lint` + `helm template` passed 2026-05-21; live kind cluster install succeeded 2026-05-21; NOT production K8s/HA
- **Prerequisites**: None for local scaffold; live cluster optional.
- **Blocked on**: Nothing — live kind install completed successfully.
- **Acceptance criteria**:
  - ✅ `helm lint` passes locally.
  - ✅ `helm template` renders valid K8s manifests locally.
  - ✅ Live cluster install produces ready pod — **PASS on kind**.
  - Production K8s / HA — NOT CLAIMED.
- **Evidence achieved**:
  - kind v0.23.0 cluster `ferrumgate-helm-live` created successfully.
  - Local Docker image `ferrumgate/ferrumd:0.1.0` loaded.
  - Helm v3.15.4 release `ferrumgate` installed in namespace `ferrumgate`.
  - Pod `ferrumgate-5cf6c87fb5-nr5hj 1/1 Running 0 restarts`.
  - Port-forward health returned `{"status":"ok"}`; readiness returned `{"status":"ready"}`.
  - Evidence artifact: `artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §4.
- **Evidence required for production K8s claim**: Operator-provided cluster; Ingress/TLS; multi-node/rolling update evidence.
- **Exact next action**: Operator review of kind-only evidence; production K8s validation deferred until operator cluster available.
- **Downstream impact**: Enables K8s deployment mode documentation and operator cluster testing.

### BLK-A-DOM — Real owned domain / Block A full closure

- **Blocker ID**: `BLK-A-DOM`
- **Owner**: Operator
- **Status**: ☐ WAIVED/CONDITIONAL
- **Prerequisites**: Operator procures domain.
- **Blocked on**: Operator action only.
- **Acceptance criteria**:
  - DNS A record points to `34.158.51.8`.
  - `dig +short <domain>` resolves correctly.
  - HTTPS 200 from target host.
  - L1–L5 re-run with real domain.
  - G2 re-signoff completed (full closure, not conditional).
- **Evidence required**: `artifacts/YYYY-MM-DD-block-a-domain-evidence.md` + `artifacts/YYYY-MM-DD-block-a-closure-evidence.md`
- **Exact next action**: Operator procures domain, configures DNS, then notifies engineering to re-run target bridge. See `docs/implementation-path/artifacts/2026-05-21-blk-a-dom-operator-action-brief.md` for step-by-step requirements, evidence format, consequences, and timeline decision point.
- **Downstream impact**: Required for Tier 2 (production-ready / domain-backed). Gates Tier 1 → Tier 2 progression. Does not gate Tier 0 → Tier 1.
- **Conditional signoff note**: BrianNguyen authorized conditional re-signoff for single-node SQLite pilot on 2026-05-21. This does **not** close BLK-A-DOM or complete full G2.

---

## Evidence artifact naming convention

For each blocker resolution, use the following artifact paths:

| Blocker ID | Artifact path template |
|------------|------------------------|
| BLK-SLO-RAT | `docs/implementation-path/artifacts/YYYY-MM-DD-slo-ratification-signoff.md` |
| BLK-SLO-TGT | `docs/implementation-path/artifacts/YYYY-MM-DD-slo-target-evidence.md` |
| BLK-SEC-PH4 | `docs/implementation-path/artifacts/YYYY-MM-DD-security-model-operator-decisions.md` + `YYYY-MM-DD-scoped-token-implementation-evidence.md` |
| BLK-UX-4 | `docs/implementation-path/artifacts/YYYY-MM-DD-ux4-token-cli-evidence.md` |
| BLK-MCP-TGT | `docs/implementation-path/artifacts/YYYY-MM-DD-mcp-target-smoke-evidence.md` |
| BLK-DEP-5 | `docs/implementation-path/artifacts/YYYY-MM-DD-dep5-helm-scaffold-evidence.md` (and optional `YYYY-MM-DD-dep5-helm-install-evidence.md`) |
| BLK-A-DOM | `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-domain-evidence.md` + `YYYY-MM-DD-block-a-closure-evidence.md` |

---

## Cross-references

- [`00a-domainless-readiness-tier.md`](./00a-domainless-readiness-tier.md) — Canonical tiered readiness model
- [`01-slo-sla.md`](./01-slo-sla.md) — SLO/SLA draft and targets
- [`slo-validation-runbook.md`](./slo-validation-runbook.md) — Repeatable validation procedure
- [`03-target-mcp-live-workload-plan.md`](./03-target-mcp-live-workload-plan.md) — MCP target-host plan
- [`04-security-tenant-model-adr.md`](./04-security-tenant-model-adr.md) — Security and tenant model ADR
- [`06-admin-operator-ux-plan.md`](./06-admin-operator-ux-plan.md) — Admin/operator UX plan
- [`08-hosted-deployment-plan.md`](./08-hosted-deployment-plan.md) — Hosted deployment plan
- [`10-evidence-checklist.md`](./10-evidence-checklist.md) — Phase-by-phase evidence checklist
- [`docs/ROADMAP.md`](../../ROADMAP.md) — Post-pilot phased completion roadmap
- [`docs/implementation-path/122-completion-roadmap-and-hardening-tracker.md`](../../implementation-path/122-completion-roadmap-and-hardening-tracker.md) — Prior blocker tracker
- [`docs/implementation-path/67-production-readiness-roadmap.md`](../../implementation-path/67-production-readiness-roadmap.md) — Authoritative blocker status and evidence gates
- [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](../../implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md) — Canonical SLO + Helm live + conditional signoff
- [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](../../implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md) — Phase 0 NO→YES completion map and template readiness signoff
- [`docs/implementation-path/artifacts/2026-05-23-rc-ready-conditional-end-state.md`](../../implementation-path/artifacts/2026-05-23-rc-ready-conditional-end-state.md) — RC-ready conditional end state; current terminal achievement (RC-ready/conditional; max achievable without real domain)
- [`docs/implementation-path/artifacts/TEMPLATE-final-production-readiness-signoff.md`](../../implementation-path/artifacts/TEMPLATE-final-production-readiness-signoff.md) — Final production readiness signoff template
- [`docs/implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md`](../../implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md) — Full G2 re-signoff template
- [`docs/implementation-path/artifacts/TEMPLATE-pg-production-deployment-signoff.md`](../../implementation-path/artifacts/TEMPLATE-pg-production-deployment-signoff.md) — PostgreSQL production deployment signoff template
- [`docs/implementation-path/artifacts/TEMPLATE-ha-multinode-evidence-pack.md`](../../implementation-path/artifacts/TEMPLATE-ha-multinode-evidence-pack.md) — HA/multi-node evidence pack template

---

## Non-claims

- **NOT production-ready**: This plan does not make FerrumGate production-ready.
- **NOT full G2**: Full G2 requires BLK-A-DOM closure + operator-approved default-config SLO pass + operator final signoff. Conditional re-signoff on 2026-05-21 is pilot-scope only.
- **NOT full target-host certification for all configs**: Canonical SLO Run #3 passed under max-valid rate-limit config only. Runs #1/#2 failed under default/tuned configs. Full certification requires default-config pass without max-valid override.
- **NOT all future security scope complete**: Phase 4 scoped token model, UX-4 CLI, and SEC-6 audit log are implemented. Tenant model (T1–T5) and OIDC remain deferred.
- **NOT multi-tenant**: Single-tenant production (T1) is the recommended first posture.
- **NOT a committed timeline**: Dates in artifact names are templates; actual execution dates are TBD and operator-dependent.
- **DuckDNS conditional only**: Block A remains WAIVED/CONDITIONAL. Real owned domain is still required for Tier 2 (production-ready). Tier 1 (domainless production-candidate) does not require real domain.
- **NOT production K8s/HA**: Helm live install verified on local kind cluster only. Tier 1 includes HA-B (local Helm validation), not HA implementation.

---

*End of file — Blockers and Unblock Plan (planning artifact only).*
