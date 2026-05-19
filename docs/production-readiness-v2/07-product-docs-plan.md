# 07 — Product Docs Plan

> **Status**: Planning artifact. Docs scaffolds exist; content is not validated.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Make FerrumGate understandable and usable by someone outside the project team. Create product-facing docs that explain what it is, when to use it, and how to get started.

## Current state

- Implementation/evidence/runbook docs are extensive.
- No product-facing landing doc.
- No quickstart validated end-to-end. API/curl flow validated locally through lineage endpoint; ferrumctl and MCP remain scaffold.
- Guide scaffolds created in `docs/guides/`; quickstart API/curl path has local validation evidence.

## Gaps

| Gap | Why |
|-----|-----|
| No "What is FerrumGate?" landing | External users cannot understand the product |
| No validated quickstart | Cannot prove a new user can get running in <30 min |
| No concepts guide | Users do not understand intent/capability/provenance |
| No MCP client integration guide | MCP users cannot configure the server |
| No policy authoring guide | Policy authors must read code |
| No adapter guide | Users do not know per-adapter limitations |
| No operator guide | Operators must read source |
| No hosted deployment guide | No reproducible deployment story |

## Implementation tasks

1. **Landing / "What is FerrumGate?"**
   - [ ] Write 1-page explanation.
   - [ ] Include: problem solved, when to use, when NOT to use, architecture diagram.

2. **Quickstart 10 minutes**
   - [x] Validate curl/API version — full API/curl flow validated locally (`healthz` through `lineage`, including `authorize`, `prepare`, `execute`, `verify`, `evaluate-outcome`).
   - [ ] Validate ferrumctl version end-to-end.
   - [ ] Validate MCP version end-to-end.
   - [x] Time the flow — API/curl flow elapsed 0.384 s locally; supports <30 min target for validated API/curl path. ferrumctl and MCP timing remain open.

3. **Concepts guide**
   - [ ] Explain: Intent, Proposal, Policy decision, Capability, Approval, Rollback class, Provenance, Lineage, Adapter, R0/R1/R2/R3.

4. **API guide**
   - [ ] Link OpenAPI spec.
   - [ ] Document endpoint lifecycle, auth, errors, examples.

5. **MCP integration guide**
   - [ ] How to run MCP server.
   - [ ] Sample client config.
   - [ ] Tools list, auth setup, security warnings.

6. **Policy authoring guide**
   - [ ] Schema, examples, templates, common patterns.
   - [ ] At least 5 templates/examples.

7. **Adapter guide**
   - [ ] Per-adapter: operations, rollback, limitations, examples, risk class.

8. **Operator guide**
   - [ ] Config, deployment, backup/restore, token rotation, incident response, monitoring, SLO/SLA.

9. **Hosted deployment guide**
   - [ ] systemd, Docker Compose, Kubernetes/Helm later, reverse proxy/TLS, PostgreSQL, backup/restore.

## Acceptance criteria

- [-] DOC-1 (PARTIAL / NOT CLOSED): API/curl flow completes in <30 min — validated locally (0.384 s elapsed). Full quickstart end-to-end (including ferrumctl + MCP) NOT validated. Fresh-user test NOT performed. Acceptance criterion remains OPEN.
- [-] DOC-2 (PARTIAL / NOT CLOSED): Validated API/curl demo runs without secrets — `auth_mode=disabled`, no bearer token required for API/curl flow. ferrumctl and MCP paths NOT validated. Acceptance criterion remains OPEN.
- [ ] DOC-3: Docs state production-ready limitations correctly.
- [ ] DOC-4: MCP client config example exists.
- [ ] DOC-5: Policy guide has at least 5 templates/examples.

## Evidence required

- `docs/implementation-path/artifacts/2026-05-19-quickstart-validation-evidence.md`
- Timer logs for quickstart validation
- Review signoff that no doc overclaims readiness

## Non-claims

- **NOT a marketing site**: These are repo docs, not a public website.
- **NOT production-ready**: Docs do not change the production-ready posture.
- **NOT validated until tested**: Quickstart timing claims require actual new-user testing.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.8, §4 Phase 7
- [`docs/guides/`](../../guides/) — Guide scaffolds.
