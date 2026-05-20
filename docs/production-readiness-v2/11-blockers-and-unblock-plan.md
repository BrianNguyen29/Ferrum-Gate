# 11 — Blockers and Unblock Plan

> **Status**: Planning artifact. Tracks the 7 active blockers/open items that gate further production-path progress.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Executive Summary

Seven items remain open between the current conditional RC posture and the next credible production-candidate milestones. This doc orders them, assigns owners, states prerequisites, and lists the exact next action for each. It does not claim any item is complete, and it does not claim production-ready status.

The seven blockers:

| # | Blocker ID | Item | Owner | Status |
|---|------------|------|-------|--------|
| 1 | **BLK-SLO-RAT** | SLO operator ratification | Operator | ☐ NOT STARTED — operator signoff pending |
| 2 | **BLK-SLO-TGT** | SLO target-host workload validation | Engineering | 🚫 BLOCKED — valid target bearer token required |
| 3 | **BLK-SEC-PH4** | Phase 4 scoped token / RBAC model | Engineering + Operator | 🚫 BLOCKED — pending operator tenant / OIDC / scoped-token decisions |
| 4 | **BLK-UX-4** | UX-4 token rotate / revoke CLI | Engineering | 🚫 BLOCKED — requires Phase 4 scoped token endpoints |
| 5 | **BLK-MCP-TGT** | Phase 3 MCP target-host smoke | Engineering | 🚫 BLOCKED — target bearer token / access required |
| 6 | **BLK-DEP-5** | DEP-5 Helm / K8s packaging | Engineering | ☐ NOT STARTED — no dependency on operator; can start locally |
| 7 | **BLK-A-DOM** | Real owned domain / Block A full closure | Operator | ☐ WAIVED/CONDITIONAL — real domain still required for production-ready or full G2 closure |

---

## Blocker taxonomy

| Axis | Engineering-owned | Operator-required |
|------|-------------------|-------------------|
| **Blocked on operator input** | BLK-SEC-PH4, BLK-UX-4 | BLK-SLO-RAT, BLK-A-DOM |
| **Blocked on target access / token** | BLK-SLO-TGT, BLK-MCP-TGT | — |
| **Unblocked / can start now** | BLK-DEP-5 | — |

---

## Ordered todo list

### 🔧 Now / local-safe (engineering can start immediately)

| # | Todo | Blocker ID | Owner | Prerequisites | Acceptance Criteria | Evidence Required |
|---|------|------------|-------|---------------|---------------------|-------------------|
| N.1 | Scaffold Helm chart directory and `Chart.yaml` | BLK-DEP-5 | Engineering | None | `helm lint` passes on scaffold; no live cluster required | `docs/implementation-path/artifacts/2026-05-20-dep5-helm-scaffold-evidence.md` |
| N.2 | Define K8s manifest set (Deployment, Service, ConfigMap, Secret, Ingress) in `helm/ferrumgate/templates/` | BLK-DEP-5 | Engineering | N.1 | Templates render with `helm template`; no syntax errors | Same artifact §N.2 |
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
| P.1 | Run SLO target-host workload per `slo-validation-runbook.md` | BLK-SLO-TGT | Engineering | O.1 (ratification) + O.2 (token) | All five phases complete; p99 latencies and error rates recorded; artifact created | `artifacts/YYYY-MM-DD-slo-target-evidence.md` |
| P.2 | Run MCP target-host smoke (Layer 1–3) per `03-target-mcp-live-workload-plan.md` | BLK-MCP-TGT | Engineering | O.2 (token) | MCP-1 through MCP-7 pass; artifact created with no secrets | `artifacts/YYYY-MM-DD-mcp-target-smoke-evidence.md` |
| P.3 | Implement scoped token store schema and RBAC middleware | BLK-SEC-PH4 | Engineering | O.3 (decisions) | SEC-1 through SEC-6 pass in automated tests | `artifacts/YYYY-MM-DD-scoped-token-implementation-evidence.md` |
| P.4 | Implement `ferrumctl admin tokens` CLI (list/create/revoke/rotate) | BLK-UX-4 | Engineering | P.3 complete | UX-4 acceptance criteria met; demo recording or test output captured | `artifacts/YYYY-MM-DD-ux4-token-cli-evidence.md` |
| P.5 | Implement `POST /v1/admin/tokens` and `DELETE /v1/admin/tokens/{id}` APIs | BLK-UX-4 | Engineering | P.3 complete | APIs return correct 201/204/401/403; test coverage added | Same artifact §P.5 |
| P.6 | Run DEP-5 Helm install against a live cluster (optional; can remain local-only until operator cluster available) | BLK-DEP-5 | Engineering | N.1–N.3 + optional live cluster | `helm install` produces ready pod; `kubectl get pods` shows Running | `artifacts/YYYY-MM-DD-dep5-helm-install-evidence.md` |
| P.7 | Re-run L1–L5 target bridge + G2 re-signoff with real domain | BLK-A-DOM | Engineering + Operator | O.4 + O.5 | All target bridge checks pass; operator signs updated signoff packet | `artifacts/YYYY-MM-DD-block-a-closure-evidence.md` |

### 📦 Deferred (out of current scope; do not start)

| # | Todo | Blocker ID | Owner | Rationale |
|---|------|------------|-------|-----------|
| D.1 | HA/multi-node (Phase 9) | — | Engineering + Operator | Requires PG production foundation + security model stable; see `09-ha-roadmap.md` |
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
- **Status**: ☐ NOT STARTED
- **Prerequisites**: `01-slo-sla.md` draft exists; `slo-validation-runbook.md` exists.
- **Blocked on**: Operator review time and signoff.
- **Acceptance criteria**:
  - Operator has read `01-slo-sla.md` and `slo-validation-runbook.md`.
  - Operator replies with ratification or a numbered list of requested changes.
  - Ratification artifact is signed and stored in `docs/implementation-path/artifacts/`.
- **Evidence required**: `artifacts/YYYY-MM-DD-slo-ratification-signoff.md`
- **Exact next action**: Operator reads `01-slo-sla.md` and replies with ratification or change requests.
- **Downstream impact**: Unblocks BLK-SLO-TGT (engineering can then execute target workload with ratified targets).

### BLK-SLO-TGT — SLO target-host workload validation

- **Blocker ID**: `BLK-SLO-TGT`
- **Owner**: Engineering
- **Status**: 🚫 BLOCKED
- **Prerequisites**: BLK-SLO-RAT ratified; valid target bearer token available.
- **Blocked on**: Operator signoff (BLK-SLO-RAT) + shared target bearer token handoff (O.2, same dependency as BLK-MCP-TGT).
- **Acceptance criteria**:
  - Pilot readiness check passes before and after run.
  - All five workload phases execute (baseline → low → target → spike → cooldown).
  - p99 latencies recorded for evaluate, mint, execute pipeline.
  - 5xx rate < 1%; 429 rate < 5%.
  - Evidence artifact created and marked PENDING SIGNOFF.
- **Evidence required**: `artifacts/YYYY-MM-DD-slo-target-evidence.md`
- **Exact next action**: Engineering awaits operator ratification (BLK-SLO-RAT) and token handoff (O.2).
- **Downstream impact**: Enables workload model refresh and G2 re-signoff.

### BLK-SEC-PH4 — Phase 4 scoped token / RBAC model

- **Blocker ID**: `BLK-SEC-PH4`
- **Owner**: Engineering + Operator
- **Status**: 🚫 BLOCKED
- **Prerequisites**: `04-security-tenant-model-adr.md` reviewed by operator.
- **Blocked on**: Operator answers Q1–Q6 in §Operator Decision Packet.
- **Acceptance criteria**:
  - Operator decision artifact signed.
  - Scoped token store schema implemented.
  - RBAC middleware denies by default.
  - SEC-1 through SEC-6 automated tests pass.
- **Evidence required**: `artifacts/YYYY-MM-DD-security-model-operator-decisions.md` + `artifacts/YYYY-MM-DD-scoped-token-implementation-evidence.md`
- **Exact next action**: Operator answers decision packet Q1–Q6.
- **Downstream impact**: Unblocks BLK-UX-4 (token CLI).

### BLK-UX-4 — UX-4 token rotate / revoke CLI

- **Blocker ID**: `BLK-UX-4`
- **Owner**: Engineering
- **Status**: 🚫 BLOCKED
- **Prerequisites**: BLK-SEC-PH4 implementation complete.
- **Blocked on**: Phase 4 scoped token endpoints exist.
- **Acceptance criteria**:
  - `ferrumctl admin tokens list/create/revoke/rotate` wired to new admin APIs.
  - CLI tests pass.
  - Demo recording or test output captured.
- **Evidence required**: `artifacts/YYYY-MM-DD-ux4-token-cli-evidence.md`
- **Exact next action**: Engineering implements BLK-SEC-PH4 first; then implements CLI.
- **Downstream impact**: Enables operator token lifecycle management without curl.

### BLK-MCP-TGT — Phase 3 MCP target-host smoke

- **Blocker ID**: `BLK-MCP-TGT`
- **Owner**: Engineering
- **Status**: 🚫 BLOCKED
- **Prerequisites**: Valid target bearer token; target gateway reachable.
- **Blocked on**: Operator provides valid target bearer token (O.2).
- **Acceptance criteria**:
  - MCP-1: `tools/list` returns 19 tools against target.
  - MCP-2: 9 read-only tools pass.
  - MCP-3: Mutating tools fail closed without auth.
  - MCP-4: Lifecycle flow passes with auth.
  - MCP-5: Provenance chain exists.
  - MCP-6: Redaction/sanitization verified.
  - MCP-7: Evidence artifact created with no secrets.
- **Evidence required**: `artifacts/YYYY-MM-DD-mcp-target-smoke-evidence.md`
- **Exact next action**: Engineering awaits token handoff (O.2), then runs Layer 1–3 per `03-target-mcp-live-workload-plan.md`.
- **Downstream impact**: Proves agent path on target; feeds G2 re-signoff.

### BLK-DEP-5 — DEP-5 Helm / K8s packaging

- **Blocker ID**: `BLK-DEP-5`
- **Owner**: Engineering
- **Status**: ☐ NOT STARTED
- **Prerequisites**: None for local scaffold; live cluster optional.
- **Blocked on**: Nothing. This is the only blocker that can start immediately.
- **Acceptance criteria**:
  - `helm lint` passes.
  - `helm template` renders valid K8s manifests.
  - Local dry-run validation passes (`kubeconform` or equivalent).
  - Optional: live cluster install produces ready pod.
- **Evidence required**: `artifacts/YYYY-MM-DD-dep5-helm-scaffold-evidence.md` (+ optional live install artifact)
- **Exact next action**: Engineering creates `helm/ferrumgate/` scaffold and runs local lint/template checks.
- **Downstream impact**: Enables K8s deployment mode documentation and eventual live cluster testing.

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
  - G2 re-signoff completed.
- **Evidence required**: `artifacts/YYYY-MM-DD-block-a-domain-evidence.md` + `artifacts/YYYY-MM-DD-block-a-closure-evidence.md`
- **Exact next action**: Operator procures domain, configures DNS, then notifies engineering to re-run target bridge.
- **Downstream impact**: Required for any production-ready or full G2 claim.

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

---

## Non-claims

- **NOT production-ready**: This plan does not make FerrumGate production-ready.
- **NOT full G2**: Full G2 requires BLK-A-DOM closure + BLK-SLO-TGT evidence + operator re-signoff.
- **NOT target-host validated**: No target-host workload or MCP smoke has been executed yet for the items marked BLOCKED.
- **NOT implemented**: Phase 4 scoped token model and UX-4 CLI are designs only until BLK-SEC-PH4 is unblocked.
- **NOT multi-tenant**: Single-tenant production (T1) is the recommended first posture.
- **NOT a committed timeline**: Dates in artifact names are templates; actual execution dates are TBD and operator-dependent.
- **DuckDNS conditional only**: Block A remains WAIVED/CONDITIONAL. Real owned domain is still required for any production-ready claim.

---

*End of file — Blockers and Unblock Plan (planning artifact only).*
