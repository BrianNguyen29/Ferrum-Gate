# FerrumGate Deferred and Do-Not-Build Items

> **Status**: Phase 8 documentation artifact — single source of truth for deferred and explicitly out-of-scope items.
> **Owner**: Operator
> **Last updated**: 2026-05-29
> **Parent**: [`docs/plan.md`](../plan.md)

---

## 1. Purpose and Scope

This document is the **single source of truth (SSOT)** for items that are intentionally **deferred**, **out of scope**, or explicitly marked **do not build** in the FerrumGate strategic execution path.

**What this document is:**
- A boundary-setting record that prevents scope creep, stakeholder ambiguity, and accidental commitment to work that is not aligned with FerrumGate's core execution-governance positioning.

**What this document is NOT:**
- It is **NOT** implementation evidence that any deferred item has been built or tested.
- It is **NOT** a promise to build anything listed here at a future date.
- It is **NOT** the canonical non-claims/readiness boundary document; see [`docs/security/non-claims.md`](../security/non-claims.md).
- It is **NOT** a strategic execution checklist; see [`docs/plan.md`](../plan.md).

---

## 2. Non-Claims Preamble

The following canonical non-claims apply to all items in this document and must be preserved in any reference, cross-link, or derivative work:

| Boundary | Status |
|----------|--------|
| `production-ready` | **NO** |
| `Tier 2` | **NOT COMPLETE** |
| `full G2` | **NOT COMPLETE** |
| Real owned domain / public endpoint | **MISSING** |
| `Block A` | **WAIVED / CONDITIONAL** — accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure. |
| `HA-4` unattended automated failover | **NOT COMPLETE** |
| Multi-host production HA | **NOT COMPLETE** |
| Sustained SLO window (7–30 days) | **NOT COMPLETE** |

No item in this deferred list may be cited as evidence of production readiness, Tier 2 completion, or general availability.

---

## 3. Deferred Items Table

| Item | Status | Reason Deferred | Reconsideration Condition | Source Refs |
|------|--------|-----------------|---------------------------|-------------|
| **Multi-tenant SaaS** | `DEFERRED` | Single-tenant hardening is incomplete; multi-tenant introduces data-isolation risk, complex auth surface, and org-hierarchy scope creep that diverts from core execution governance. | Single-tenant is production-hardened (Tier 2 signed off, real domain, sustained SLO), and an operator explicitly requests multi-tenant with documented isolation requirements. | [`docs/plan.md` §3 Anti-pattern table](../plan.md) |
| **Web dashboard** | `DEFERRED` | TUI + CLI evidence UX is sufficient for operator workflows today. A web dashboard would introduce session management, CSRF, RBAC UI, frontend maintenance, and audit UI scope that distracts from execution-governance primitives. | Operator evidence UX (CLI/TUI) is complete and signed off, and an enterprise customer requires a web dashboard with dedicated UX resources. | [`docs/plan.md` §3 Anti-pattern table](../plan.md) |
| **HA-4 unattended automated failover** | `NOT COMPLETE` | Risk of split-brain, false-positive promotion, insufficient fencing, and data loss if automation is incorrect. Manual/operator-controlled failover is the current safe mode. | Multi-host production HA evidence exists, fencing is operator-validated, and an automated promotion runbook has been rehearsed successfully with zero data-loss incidents. | [`docs/plan.md` §3 Anti-pattern table](../plan.md), [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../production-readiness-v2/00-scope-and-nonclaims.md) |
| **WASM sandbox** | `DEFERRED` | No current adapter or tool execution model requires sandboxed WASM. Adds toolchain, security surface, and runtime complexity without a validated use case. | A validated use case emerges where an adapter must run untrusted code in a bounded sandbox, and the operator accepts the runtime overhead. | [`docs/plan.md` §3 Anti-pattern table (implied)](../plan.md) |
| **Full compliance automation / SOC2-GRC-SIEM automation** | `DEFERRED` | FerrumGate is not production-ready and has no enterprise certification path yet. Building a compliance automation platform before core governance is complete is premature. | Production-ready status achieved (Tier 2, real domain, sustained SLO), and an operator or customer requests formal compliance mapping with evidence. | [`docs/plan.md` §3 Anti-pattern table](../plan.md), [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md) §Non-Claims |
| **Agent marketplace** | `DEFERRED` | An agent marketplace would re-position FerrumGate as an AGT-like ecosystem, not an execution-governance gateway. No validated demand exists. | Core execution governance is complete and a partner ecosystem explicitly requests a marketplace layer. | [`docs/plan.md` §3 Anti-pattern table](../plan.md) |
| **DID / trust-score mesh** | `DEFERRED` | W3C DID, Verifiable Credentials, and trust scoring are complex, standards-immature, and orthogonal to FerrumGate's capability-model approach. | A standards-mature, lightweight DID mechanism is available that integrates cleanly with Ed25519 agent identity without scope creep. | [`docs/security/agent-identity-ed25519.md`](../security/agent-identity-ed25519.md) §7 |
| **mTLS native implementation** | `DEFERRED` | Native mTLS in `ferrumd` / `ferrum-mcp-server` would require TLS termination libraries, certificate reloading, revocation checking, and failure-mode handling. Current single-node topology has no cross-host network hops justifying the complexity. | Multi-node, cross-host deployment topology exists (gateway and MCP server on separate hosts, or store on a separate network segment). | [`docs/security/mtls-service-mesh.md`](../security/mtls-service-mesh.md) §3 ADR, [`docs/plan.md` §4 Feature matrix](../plan.md) |
| **SSE streaming / session / resumability for MCP** | `DEFERRED` | Phase 6.1 skeleton covers synchronous JSON-RPC over HTTP. SSE streaming, session management (`Mcp-Session-Id`), resumability, and strict SEP-2243 headers are deferred to Phase 6.2+. | A remote MCP client or deployment topology explicitly requires SSE streaming or session resumability, and the Streamable HTTP MCP spec is finalized/stable. | [`docs/mcp/streamable-http-mcp.md`](../mcp/streamable-http-mcp.md) §Deferred items |
| **External anchoring / WORM sink** | `DEFERRED` | Tamper-evident audit hash chain and signed checkpoints are implemented (Phase 5.1–5.2). External anchoring (blockchain, timestamp authority) and WORM sink integration add operational complexity and cost without current threat-model justification. | A compliance or threat-model requirement mandates third-party tamper evidence beyond signed checkpoints, or an operator is willing to operate the anchoring infrastructure. | [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md) §Future Work |
| **Cloud / Pulumi provider modules** | `DEFERRED` | Local Terraform artifact generator exists; cloud-specific modules (AWS, GCP, Azure) require per-provider maintenance, testing, and documentation overhead. | Operator requests a specific cloud provider module and is willing to co-maintain or sponsor it. | [`docs/security/non-claims.md`](../security/non-claims.md) §2 |
| **Cedar / OPA policy engine formalization** | `DEFERRED` | FerrumGate uses a native static PDP and policy bundle model. Formalizing Cedar or OPA would require a full policy-language migration, testing, and tooling that is not justified by current operator needs. Legacy/backlog source: early roadmap-v2 exploration considered OPA but did not proceed. | A customer or operator explicitly requires Cedar/OPA policy language support and provides migration/testing resources. | [`docs/plan.md`](../plan.md) (legacy/backlog consideration only) |
| **Cryptographic capabilities (macaroon / signed capability chain)** | `DEFERRED` | Current capabilities are DB-backed, scoped, time-bounded, and single-use. Macaroons or signed capability chains would add cryptographic complexity (caveat verification, attenuation) without a validated performance or trust-boundary need. Legacy/backlog source: capability hardening backlog item from 2026-Q1. | A validated use case requires delegated, attenuated, or offline-verifiable capabilities (e.g., cross-domain capability sharing). | [`docs/plan.md`](../plan.md), internal capability-hardening backlog |

---

## 4. Explicit Do-Not-Build Items

The following items are **explicitly out of scope** and must **not** be built, planned, or represented as future roadmap commitments in any FerrumGate document, presentation, or marketing material.

### 4.1 Tunnel Service

**Status**: `DO NOT BUILD`

**Rationale:**
- OpenAI Secure MCP Tunnels, Cloudflare Tunnel, and Tailscale already solve secure transport with operator-grade reliability, certificate management, and global edge presence.
- Building a native tunnel service would split engineering focus away from execution governance, introduce a security-sensitive surface (reverse tunnels are hardening-critical), and duplicate industry-standard solutions.
- FerrumGate's correct posture is **works behind secure tunnels**, not **is a tunnel**.

**What FerrumGate does instead:**
- Documents integration guides for OpenAI Secure MCP Tunnels, Cloudflare Tunnel, and Tailscale. See [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md).
- Assumes tunnel-layer identity (WireGuard machine identity, Cloudflare Access, mTLS at edge) where applicable.
- Never opens inbound MCP public ports without auth/tunnel in front.

**Reconsideration condition:** None. This is a permanent architectural boundary. If tunnel requirements change, the integration guides are updated, not a native service built.

### 4.2 AGT-Like SDK Ecosystem

**Status**: `DO NOT BUILD`

**Rationale:**
- Microsoft Agent Governance Toolkit (AGT) is an ecosystem play: multi-framework governance SDKs, agent marketplace, full trust mesh, DID ecosystem, policy SDKs for every framework, and a compliance suite.
- FerrumGate is intentionally smaller and more focused: execution governance for MCP/agentic operations. Building an AGT clone would erase FerrumGate's differentiation and consume resources without competitive advantage.
- FerrumGate's correct posture is **selectively borrow / integrate** from AGT-style identity/compliance middleware, while keeping execution governance as the sole core.

**What FerrumGate does instead:**
- Provides lightweight Ed25519 agent identity. See [`docs/security/agent-identity-ed25519.md`](../security/agent-identity-ed25519.md).
- Supports OIDC/JWT federation for human operators. See [`docs/security/oidc-jwt-federation.md`](../security/oidc-jwt-federation.md).
- Offers scoped tokens and RBAC for service integrations. See [`docs/security/scoped-tokens-rbac.md`](../security/scoped-tokens-rbac.md).
- Documents OWASP Agentic AI controls and gaps without claiming AGT-style breadth. See [`docs/security/owasp-agentic-ai-mapping.md`](../security/owasp-agentic-ai-mapping.md).

**Reconsideration condition:** None. This is a permanent strategic boundary. Individual primitives (e.g., richer agent identity) may evolve, but an AGT-style SDK ecosystem is explicitly out of scope.

---

## 5. Revisit Triggers and Review Cadence

### 5.1 Revisit Triggers for Deferred Items

Any deferred item may be reconsidered **only** when **all** of the following are true:
1. A clear, validated use case or customer request exists.
2. The item's reconsideration condition (Section 3) is satisfied.
3. Core execution governance (P0 items in [`docs/plan.md`](../plan.md)) is complete and signed off.
4. The operator explicitly approves the scope expansion in writing.

### 5.2 Review Cadence

This document must be reviewed:
- **Quarterly** as part of the strategic execution checklist review.
- **Immediately** when any deferred item is promoted to "build" or "design" status.
- **Immediately** when any new deferred item is identified (e.g., from threat modeling, incident review, or customer feedback).
- **Immediately** when any canonical non-claim boundary changes (e.g., Tier 2 signoff, real domain acquisition, HA-4 completion).

### 5.3 Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-05-29 | Phase 8 deferred doc created | Boundary-setting exercise to prevent scope creep before Phase 7 implementation. |

---

## 6. Related Documents

| Document | Purpose |
|----------|---------|
| [`docs/plan.md`](../plan.md) | Strategic execution checklist and phase tracking. |
| [`docs/security/non-claims.md`](../security/non-claims.md) | Canonical non-claims and readiness boundaries. |
| [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../production-readiness-v2/00-scope-and-nonclaims.md) | Scope and non-claims for the production-readiness path. |
| [`docs/security/mtls-service-mesh.md`](../security/mtls-service-mesh.md) | mTLS design doc with native implementation deferral rationale. |
| [`docs/mcp/streamable-http-mcp.md`](../mcp/streamable-http-mcp.md) | Streamable HTTP MCP transport with deferred SSE/session items. |
| [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md) | Audit design with deferred external anchoring / WORM. |
| [`docs/security/agent-identity-ed25519.md`](../security/agent-identity-ed25519.md) | Agent identity design with DID/trust-mesh non-claim. |
| [`docs/security/owasp-agentic-ai-mapping.md`](../security/owasp-agentic-ai-mapping.md) | OWASP mapping with compliance automation deferred. |
| [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) | Tunnel integration guide (FerrumGate works behind tunnels). |
| [`docs/ROADMAP.md`](../ROADMAP.md) | Historical/legacy roadmap reference. |

---

*End of deferred and do-not-build document.*
