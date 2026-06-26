# OWASP Agentic AI Top 10 Mapping — FerrumGate Controls, Gaps, and Coverage

> **Date:** 2026-05-29
> **Owner / Type:** Security / Document
> **Parent Plan:** [`guides/README.md`](../guides/README.md)

---

## 1. Important Caveat

This document uses the **official OWASP Top 10 for LLM Applications v2.0 (2025)** as the current authoritative baseline and maps its ten categories to FerrumGate controls, gaps, and evidence references.

**When the dedicated OWASP Agentic AI Top 10 is published, this mapping must be reviewed and remapped.** Some categories may split, merge, or receive new identifiers. This document is explicitly versioned to the LLM Top 10 v2.0 baseline.

---

## 2. Scope and Positioning

FerrumGate is an **execution-governance gateway for MCP/agentic operations**, not an LLM runtime, model host, or prompt processor. Its role in an agentic AI system is to sit between the agent/LLM client and the tools/adapters that perform side effects. Therefore, this mapping focuses on:

- How FerrumGate's controls **mitigate risks that arise when LLMs drive tool use**.
- Where FerrumGate **does not intervene** and why that is an accepted boundary.
- Gaps and limitations that operators must address through other controls.

This document is **not** a compliance certification or SOC2 mapping.

---

## 3. Executive Summary Table

| OWASP v2.0 Code | Category | FerrumGate Relevance | Coverage | Key Control | Key Gap |
|-----------------|----------|----------------------|----------|-------------|---------|
| **LLM01** | Prompt Injection | Indirect — FerrumGate does not parse prompts | Partial | Policy evaluation on structured intents; capability minting blocks unauthorized actions | No natural-language prompt sanitization or detection |
| **LLM02** | Sensitive Information Disclosure | Medium | Partial | Scoped tokens, no token logging, TLS, OIDC/JWT, Ed25519 agent identity, deny-by-default | No output PII scanning; vault integration not provided by this component |
| **LLM03** | Supply Chain | Low (gateway only) | Partial | `cargo-deny`/`cargo-audit`; dependency scanning; no adapter binary signing yet | No LLM model provenance validation; no adapter attestation |
| **LLM04** | Data and Model Poisoning | Low | Partial | Policy bundles restrict tool/action scope; tamper-evident audit log detects config changes | No training-data or model-weight validation |
| **LLM05** | Improper Output Handling | Medium | Partial | Execution lifecycle (prepare → execute → verify); rollback classification; compensation flow | No LLM-generated code safety analysis; adapter-side integrity not enforced |
| **LLM06** | Excessive Agency | **High** | Substantial | Policy-evaluated capability minting; approval gating; scoped, time-bounded (≤300s), single-use capabilities; deny-by-default; minimum lineage chain | Single-factor approval; no behavioral anomaly detection on agency patterns |
| **LLM07** | System Prompt Leakage | Very Low | None | Not applicable to gateway scope | No visibility into LLM client prompt storage |
| **LLM08** | Vector and Embedding Weaknesses | Very Low | None | Not applicable to gateway scope | No RAG pipeline in FerrumGate |
| **LLM09** | Misinformation | Low | Partial | Policy simulation/dry-run before execution; approval gating for R3 | No fact-checking or hallucination detection |
| **LLM10** | Unbounded Consumption | Medium | Partial | Rate limiting (`tower_governor`); SQLite write queue; connection pool limits; execution timeouts; capability TTL | No per-LLM-token quota; no adaptive consumption throttling |

---

## 4. Detailed Mapping

### LLM01:2025 — Prompt Injection

> **OWASP Definition:** Manipulating LLM behavior through crafted inputs, causing unintended actions or disclosure.

**FerrumGate Position:** FerrumGate does not receive or parse natural-language prompts. It receives structured intents and proposals from the MCP/agent client. Prompt injection at the LLM layer may result in a malicious proposal reaching FerrumGate, but FerrumGate applies a second control layer.

**Controls:**
- **Policy evaluation:** Every proposal is evaluated against active policy bundles before capability minting. A policy can deny actions that match known high-risk patterns (`action_is_mutation`, tool blacklists, path restrictions). See [`docs/api/policy-simulation.md`](../api/policy-simulation.md).
- **Capability bounding:** Even if a prompt-injected proposal passes policy, the resulting capability is scoped to a specific tool/action and time-bounded (max 300s). It cannot grant broad or persistent access.
- **Approval gating:** High-risk actions (R3) require explicit operator approval before execution, creating a human checkpoint that can catch anomalous proposals.
- **Intent compilation:** The intent compilation step validates structure and taint scores, rejecting malformed or suspiciously constructed requests.

**Gaps / Limitations:**
- FerrumGate has **no natural-language prompt parsing or sanitization**. It cannot detect prompt injection at the LLM input layer.
- Policy rules are written by operators; they may not cover novel prompt-injection payloads.
- There is **no automated anomaly detection** on proposal patterns that could indicate prompt manipulation (see ADR 010 for a proposed behavioral anomaly detection layer).

**Evidence / References:**
- [`docs/api/policy-simulation.md`](../api/policy-simulation.md) — dry-run policy evaluation
- [`docs/security/scoped-tokens-rbac.md`](./scoped-tokens-rbac.md) — scope enforcement
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B2 (Agent/MCP Client → MCP Server) tampering controls

---

### LLM02:2025 — Sensitive Information Disclosure

> **OWASP Definition:** Exposure of sensitive data, model details, or system information through LLM outputs or interactions.

**FerrumGate Position:** FerrumGate handles authentication tokens, audit logs, and execution metadata. It is a potential source of sensitive information if misconfigured, but it also provides controls to limit disclosure.

**Controls:**
- **No token logging:** Bearer token values, `Authorization` headers, and `Mcp-Session-Id` headers are never logged. See [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md).
- **TLS in transit:** All external connections use TLS. PostgreSQL connections support TLS DSN. SQLite is local-file only.
- **Scoped tokens with least privilege:** Tokens expose only the scopes required for a role; list operations return metadata only (redacted secrets). See [`docs/security/scoped-tokens-rbac.md`](./scoped-tokens-rbac.md).
- **OIDC/JWT federation:** Externalizes identity to enterprise IdPs; FerrumGate does not store user passwords or long-lived credentials. See [`docs/security/oidc-jwt-federation.md`](./oidc-jwt-federation.md).
- **Ed25519 agent identity:** Cryptographic agent identity means no shared secrets at the client; only public keys are stored. See [`docs/security/agent-identity-ed25519.md`](./agent-identity-ed25519.md).
- **Audit log sanitization:** Authentication failures emit sanitized `AuthFailed` entries (actor_id=`unknown`, no token/header logged).
- **Deny-by-default:** Unknown routes and unmapped scopes are rejected, reducing accidental information leakage through misconfigured endpoints.

**Gaps / Limitations:**
- FerrumGate does **not scan LLM outputs or adapter responses for PII**.
- **Secrets management (vault integration)** is not provided by this component; connection strings and adapter credentials are in config files.
- There is **no Data Loss Prevention (DLP)** layer for tool outputs.
- mTLS service-to-service is not provided by this component, so inter-service traffic encryption relies on network boundaries.

**Evidence / References:**
- [`docs/security/scoped-tokens-rbac.md`](./scoped-tokens-rbac.md)
- [`docs/security/oidc-jwt-federation.md`](./oidc-jwt-federation.md)
- [`docs/security/agent-identity-ed25519.md`](./agent-identity-ed25519.md)
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B1, B5 information disclosure controls

---

### LLM03:2025 — Supply Chain

> **OWASP Definition:** Risks from vulnerable or compromised components, training data, or model providers in the LLM supply chain.

**FerrumGate Position:** FerrumGate is a software artifact with its own dependency supply chain (Rust crates). It does not train or host LLMs, but it interacts with adapters and MCP servers that may.

**Controls:**
- **Dependency scanning:** `cargo-deny` and `cargo-audit` are run in CI. `RUSTSEC-2023-0071` is tracked and ignored only for uncompiled optional dependencies. See `Makefile` `audit` target.
- **Minimal feature selection:** JWT dependency uses only required crypto features; no unnecessary OpenSSL linkage.
- **Reproducible builds:** `Cargo.lock` is committed; `rust-toolchain.toml` pins the compiler.

**Gaps / Limitations:**
- FerrumGate does **not validate LLM model provenance**, training data integrity, or model provider trust.
- There is **no adapter binary signing or attestation**. Adapters are trusted by configuration, not cryptographic verification.
- FerrumGate does not scan adapter containers or dependencies for vulnerabilities.
- SBOM generation and distribution are not provided.

**Evidence / References:**
- `Cargo.lock`, `clippy.toml`, `Makefile` (`make audit`)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B7 adapter controls

---

### LLM04:2025 — Data and Model Poisoning

> **OWASP Definition:** Corruption of training data, fine-tuning data, or model weights to produce harmful or biased outputs.

**FerrumGate Position:** FerrumGate does not train models. Its relevance is limited to preventing poisoned data from propagating through tool execution (e.g., a poisoned model generating a malicious file that an adapter writes).

**Controls:**
- **Policy bundles:** Can restrict which tools, paths, and actions are permitted, reducing the blast radius of a poisoned model's output.
- **Tamper-evident audit log:** SHA-256 hash chain with `previous_hash` linkage detects unauthorized changes to policy bundles, audit entries, or provenance records. See [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md).
- **Rollback classification:** R1–R3 classification ensures high-risk actions require additional scrutiny, which can catch anomalous tool use triggered by poisoned outputs.

**Gaps / Limitations:**
- FerrumGate has **no ability to validate training data, model weights, or fine-tuning datasets**.
- It does not detect whether an LLM output is the result of model poisoning versus legitimate behavior.
- Content-hash validation exists for policy bundles but not for adapter inputs or outputs.
- The audit log provides tamper-evident detection, but no WORM storage or external anchoring for long-term archival. See ADR 009 for a proposed WORM export bundle.

**Evidence / References:**
- [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md)
- [`docs/api/policy-simulation.md`](../api/policy-simulation.md)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B5, B6 tampering controls

---

### LLM05:2025 — Improper Output Handling

> **OWASP Definition:** Insufficient validation, sanitization, or handling of LLM outputs before they are passed to downstream systems.

**FerrumGate Position:** FerrumGate's execution lifecycle is designed to verify and control the impact of tool actions before they are committed. This indirectly addresses improper handling of LLM-generated outputs by adding governance gates.

**Controls:**
- **Execution lifecycle:** `prepare` → `execute` → `verify` chain ensures side effects are prepared, executed, and then verified before commitment. See [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md).
- **Rollback classification:** Every proposal is classified R0 (no rollback needed) through R3 (irreversible / requires approval). R3 never auto-commits. See [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md).
- **Compensation / rollback:** `POST /v1/executions/{id}/compensate` can trigger rollback for recoverable actions.
- **Policy simulation:** Dry-run evaluation lets operators preview what a proposal would do without side effects. See [`docs/api/policy-simulation.md`](../api/policy-simulation.md).
- **Capability validation:** Adapters receive a capability lease scoped to a specific action; the gateway validates the lease before adapter invocation.

**Gaps / Limitations:**
- FerrumGate does **not parse or validate the semantic safety of LLM-generated content** (e.g., generated code, SQL queries, configuration files).
- **Adapter-side integrity** is not enforced by the gateway; an adapter may misinterpret or improperly handle the data it receives.
- There is no sandboxed execution environment for adapter outputs.
- Content validation (e.g., schema validation of LLM output) is adapter-dependent, not gateway-enforced.

**Evidence / References:**
- [`docs/api/policy-simulation.md`](../api/policy-simulation.md)
- [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B7 adapter controls

---

### LLM06:2025 — Excessive Agency

> **OWASP Definition:** Granting LLM-based systems too much autonomy, enabling destructive or unauthorized actions.

**FerrumGate Position:** **This is FerrumGate's core design focus.** FerrumGate exists precisely to limit and govern the agency of agents and LLM-driven systems.

**Controls:**
- **Policy-evaluated capability minting:** No capability is minted without passing active policy bundle evaluation and the static PDP engine. Deny-by-default is the baseline.
- **Approval gating:** R3 actions (irreversible, high-risk) require explicit operator approval via `approval:resolve`. The action cannot proceed until approved.
- **Scoped, time-bounded capabilities:** Capabilities have `ttl_max=300s` and are **single-use only**. They cannot be replayed or persisted beyond their window.
- **Minimum lineage chain:** The system enforces a provenance chain (`PolicyEvaluated → CapabilityMinted → ActionProposalSubmitted → SideEffectPrepared → ToolCallPrepared → ToolCallExecuted → SideEffectVerified → Terminal`) before any side effect is committed.
- **Rollback-by-default design:** Every action is classified with a rollback class; the system is designed to compensate or roll back where possible.
- **Deny-by-default scope enforcement:** Every API route requires an explicit scope; unknown paths are rejected.
- **Agent identity (Ed25519):** Cryptographic identity ensures that requests originate from a registered, non-revoked agent with explicitly allowed scopes. See [`docs/security/agent-identity-ed25519.md`](./agent-identity-ed25519.md).

**Gaps / Limitations:**
- **Single-factor approval:** Operator approval currently relies on scoped token auth only; there is no second-factor confirmation (e.g., MFA, out-of-band notification with cryptographic acknowledge). See [`docs/security/threat-model-stride.md`](./threat-model-stride.md) B8 and ADR 008 for a proposed timeout/MFA design.
- **No behavioral anomaly detection:** FerrumGate does not learn or detect unusual agency patterns (e.g., an agent suddenly requesting many R3 actions outside its historical baseline). See ADR 010 for a proposed profiling layer.
- **No automated escalation:** If an agent exceeds a rate or risk threshold, there is no automated lockdown or revocation trigger.
- Approval resolve is synchronous; there is no timeout-based auto-deny for stale approvals. See ADR 008 for a proposed timeout design.

**Evidence / References:**
- [`docs/security/scoped-tokens-rbac.md`](./scoped-tokens-rbac.md)
- [`docs/security/agent-identity-ed25519.md`](./agent-identity-ed25519.md)
- [`docs/api/policy-simulation.md`](../api/policy-simulation.md)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B2, B4, B7, B8
- [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md)

---

### LLM07:2025 — System Prompt Leakage

> **OWASP Definition:** Exposure of system prompts, instructions, or configuration through LLM manipulation or misconfiguration.

**FerrumGate Position:** FerrumGate does not store, serve, or process system prompts. This category is **largely out of scope** for the gateway.

**Controls:**
- **No prompt handling:** FerrumGate's MCP server and gateway do not expose system prompt fields or endpoints.
- **No logging of LLM traffic:** FerrumGate logs execution provenance and audit events, not LLM conversation history or prompt content.

**Gaps / Limitations:**
- FerrumGate has **zero visibility into or control over system prompt storage** at the LLM client (e.g., ChatGPT, Claude Desktop, custom agent hosts).
- If a system prompt is leaked and used to craft malicious proposals, FerrumGate's policy layer may catch the resulting proposal, but it cannot prevent the leak itself.

**Evidence / References:**
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B2

---

### LLM08:2025 — Vector and Embedding Weaknesses

> **OWASP Definition:** Vulnerabilities in vector databases and embedding models, including unauthorized access, data poisoning, or injection.

**FerrumGate Position:** FerrumGate does not use vector databases or embedding models in its core architecture. This category is **not applicable** to the gateway.

**Controls:**
- **Not applicable:** No vector store integration.

**Gaps / Limitations:**
- If FerrumGate adds RAG-based policy reasoning or semantic intent matching, this category will become relevant and must be revisited.
- As of this document, there is **no RAG pipeline, no vector DB, and no embedding model** in FerrumGate.

**Evidence / References:**
- FerrumGate has no RAG/vector DB features in its current scope.

---

### LLM09:2025 — Misinformation

> **OWASP Definition:** LLM-generated false, misleading, or harmful content that is acted upon by users or systems.

**FerrumGate Position:** FerrumGate cannot detect misinformation in LLM outputs, but its execution governance can reduce the harm of acting on misinformed outputs by adding friction and verification.

**Controls:**
- **Policy simulation / dry-run:** Operators can preview what a proposal would do before it is executed, creating an opportunity to spot misinformed actions. See [`docs/api/policy-simulation.md`](../api/policy-simulation.md).
- **Approval gating for R3:** High-stakes actions triggered by potentially misinformed outputs require human approval.
- **Provenance tracking:** Every execution is logged with actor, proposal, decision, and outcome, enabling post-hoc investigation if misinformation leads to a bad action.
- **Rollback and compensation:** Where possible, actions can be rolled back or compensated after discovery.

**Gaps / Limitations:**
- FerrumGate does **not fact-check, hallucination-detect, or semantic-verify LLM outputs**.
- It does not compare LLM outputs against trusted knowledge bases.
- Misinformation that slips through policy and approval gates can still cause harm.

**Evidence / References:**
- [`docs/api/policy-simulation.md`](../api/policy-simulation.md)
- [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md)
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md)

---

### LLM10:2025 — Unbounded Consumption

> **OWASP Definition:** LLM applications allowing excessive or uncontrolled resource usage, leading to denial of service or cost overruns.

**FerrumGate Position:** FerrumGate provides rate limiting, connection management, and execution timeouts that bound resource consumption at the gateway layer. It does not bound LLM token usage directly.

**Controls:**
- **Rate limiting:** `tower_governor` middleware applies rate limits to HTTP API requests. See [`docs/security/threat-model-stride.md`](./threat-model-stride.md) B1.
- **SQLite write queue:** Eliminates lock thrash and bounds write concurrency for the SQLite backend.
- **Connection pool limits:** PostgreSQL connection pool has max/min bounds and acquire timeouts.
- **Execution timeouts:** Adapter and tool calls have execution timeouts to prevent runaway operations.
- **Capability TTL:** Capabilities expire after a maximum of 300 seconds, preventing long-lived resource reservations.
- **WAL mode and busy_timeout:** SQLite PRAGMAs (`busy_timeout=5000ms`) prevent indefinite blocking.

**Gaps / Limitations:**
- FerrumGate has **no per-LLM-token quota or cost-tracking**. It cannot throttle based on LLM API usage or token count.
- There is **no adaptive rate limiting** that responds to LLM provider rate-limit headers.
- No resource quotas per actor, agent, or tenant (multi-tenancy is outside this document's scope).
- MCP-specific quotas (e.g., max tool calls per session) are not provided.

**Evidence / References:**
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — B1, B5 Denial of Service controls
- FerrumGate execution governance hardening (deny-by-default, scoped tokens, capability TTL) is implemented.
- [`docs/security/scoped-tokens-rbac.md`](./scoped-tokens-rbac.md) — token expiry

---

## 5. Cross-Reference Matrix

| FerrumGate Control | LLM01 | LLM02 | LLM03 | LLM04 | LLM05 | LLM06 | LLM07 | LLM08 | LLM09 | LLM10 |
|--------------------|:-----:|:-----:|:-----:|:-----:|:-----:|:-----:|:-----:|:-----:|:-----:|:-----:|
| Policy evaluation / simulation | ● | | | ● | ● | ● | | | ● | |
| Scoped tokens / RBAC | | ● | | | | ● | | | | |
| Ed25519 agent identity | | ● | | | | ● | | | | |
| OIDC/JWT federation | | ● | | | | | | | | |
| Capability minting (TTL, single-use) | ● | | | | | ● | | | | ● |
| Approval gating (R3) | ● | | | | ● | ● | | | ● | |
| Execution lifecycle (prepare/execute/verify) | | | | | ● | ● | | | | |
| Tamper-evident audit log | | | | ● | | ● | | | ● | |
| Rate limiting / connection pools | | | | | | | | | | ● |
| Deny-by-default | ● | ● | | | | ● | | | | |
| No token logging | | ● | | | | | | | | |
| TLS in transit | | ● | | | | | | | | |

*(● = primary relevance; ○ = secondary relevance, omitted for clarity)*

---

## 6. Additional Notes

- This is **not** a SOC 2, ISO 27001, or formal compliance mapping.
- This is **not** a penetration test or formal security audit.
- FerrumGate does **not** detect, sanitize, or validate LLM prompts, outputs, or model behavior directly.
- The OWASP Agentic AI Top 10 is **not finalized**; this mapping uses the OWASP LLM Top 10 v2.0 (2025) as a baseline and **must be remapped** when the Agentic list publishes.
- Coverage ratings (Partial, Substantial, None) are relative to FerrumGate's scope as an execution-governance gateway, not a full-stack AI security platform.

---

## 7. Remap Trigger

This document must be updated when:

1. The **dedicated OWASP Agentic AI Top 10 is published** — remap all categories.
2. FerrumGate implements a **new control** that changes coverage for any category (e.g., behavioral anomaly detection, prompt sanitization).
3. A **new gap is identified** through threat modeling, incident review, or external audit.
4. Any **cross-referenced document** is materially updated.

---

*End of OWASP mapping document.*
