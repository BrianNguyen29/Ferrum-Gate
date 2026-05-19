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
- No quickstart validated end-to-end. API/curl flow validated locally through lineage endpoint; ferrumctl and MCP locally validated after bugfix.
- Guide scaffolds created in `docs/guides/`; quickstart API/curl path has local validation evidence; ferrumctl and MCP have local validation evidence.

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
   - [x] Write 1-page explanation — `site/` Zola scaffold created with landing content.
   - [x] Include: problem solved, when to use, when NOT to use, architecture explanation — present in `site/templates/index.html`.

2. **Quickstart 10 minutes**
   - [x] Validate curl/API version — full API/curl flow validated locally (`healthz` through `lineage`).
   - [x] Validate ferrumctl version — all 7 tested commands pass locally after bugfix.
   - [x] Validate MCP version — all tested tools pass locally after bugfix (connection, auth, lifecycle, read queries, query_lineage).
   - [x] Time the flow — API/curl flow elapsed 0.384 s locally; ferrumctl test execution 0.367 s; supports <30 min target for validated paths.

3. **Concepts guide**
   - [x] Explain: Intent, Proposal, Policy decision, Capability, Approval, Rollback class, Provenance, Lineage, Adapter, R0/R1/R2/R3 — `docs/guides/concepts.md` expanded with architecture overview and lineage chain.

4. **API guide**
   - [x] Document endpoint lifecycle, auth, errors, examples — `docs/guides/api.md` created. OpenAPI spec not yet generated; linked to server.rs source.
   - [ ] Generate/link OpenAPI spec (post-v1).

5. **MCP integration guide**
   - [ ] How to run MCP server.
   - [ ] Sample client config.
   - [ ] Tools list, auth setup, security warnings.

6. **Policy authoring guide**
   - [ ] Schema, examples, templates, common patterns.
   - [ ] At least 5 templates/examples.

7. **Adapter guide**
   - [x] Per-adapter: operations, rollback, limitations, examples, risk class — `docs/guides/adapter-reference.md` expanded with JSON examples and rollback/risk summary table.

8. **Operator guide**
   - [x] Config, deployment, backup/restore, token rotation, incident response, monitoring, SLO/SLA — `docs/guides/operator.md` expanded with local-vs-hosted caveats, SQLite WAL notes, and common incident patterns.

9. **Hosted deployment guide**
   - [ ] systemd, Docker Compose, Kubernetes/Helm later, reverse proxy/TLS, PostgreSQL, backup/restore.

## Acceptance criteria

- [-] DOC-1 (PARTIAL / NOT CLOSED): API/curl flow + ferrumctl + MCP complete in <30 min — validated locally (API/curl 0.384 s, ferrumctl 0.367 s, MCP regression 0.33 s). Fresh-user test NOT performed. Acceptance criterion remains OPEN.
- [x] DOC-2 (LOCAL COMPLETE): All local demo paths (API/curl, ferrumctl, MCP) run without live secrets — `auth_mode=disabled` for API/curl/ferrumctl; MCP used documented dummy placeholder token because MCP has its own auth gate. Target-host validation NOT claimed.
- [x] DOC-3 (COMPLETE): Docs state production-ready limitations correctly — hosted-deployment.md DEP-4 corrected to target-host validated; Block A/DuckDNS context added; no overclaims found.
- [x] DOC-4 (COMPLETE): MCP client config example exists in `docs/guides/mcp-integration.md`.
- [x] DOC-5 (COMPLETE): Policy guide has 7 templates/examples — `docs/guides/policy-authoring.md` updated 2026-05-19. Evidence: `docs/implementation-path/artifacts/2026-05-19-doc5-policy-templates-evidence.md`.

## Evidence required

- `docs/implementation-path/artifacts/2026-05-19-quickstart-validation-evidence.md`
- `docs/implementation-path/artifacts/2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md`
- Timer logs for quickstart validation
- Review signoff that no doc overclaims readiness

## Non-claims

- **NOT a marketing site**: These are repo docs, not a public website.
- **NOT production-ready**: Docs do not change the production-ready posture.
- **NOT validated until tested**: Quickstart timing claims require actual new-user testing.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.8, §4 Phase 7
- [`docs/guides/`](../../guides/) — Guide scaffolds.
