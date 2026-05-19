# Evidence: Local-safe guides and landing expansion

> **Date**: 2026-05-19
> **Scope**: Expand docs/guides/ and create Zola landing scaffold
> **Owner**: Engineering
> **Parent**: [`docs/production-readiness-v2/07-product-docs-plan.md`](../production-readiness-v2/07-product-docs-plan.md)

---

## What was added

### Guide expansions

| File | Change | Status |
|------|--------|--------|
| `docs/guides/concepts.md` | Expanded with architecture overview, lineage chain, risk-tier vs rollback-class distinction, adapter summary table | Meaningfully explains all required concepts |
| `docs/guides/api.md` | Created — endpoint inventory (all 28 endpoints from server.rs), auth modes, error format, execution lifecycle curl example, rate limiting | Documents endpoint lifecycle, auth, errors, examples without overclaiming |
| `docs/guides/operator.md` | Expanded with local-vs-hosted caveats table, SQLite WAL notes, common incident patterns table, token rotation verification note | Covers config, health, backup/restore, token rotation, monitoring, incident response |
| `docs/guides/adapter-reference.md` | Expanded with JSON examples per adapter, rollback/risk summary table, "When rollback fails" section | Covers fs, git, http, sqlite, maildraft with rollback and risk caveats |
| `docs/guides/README.md` | Updated guide index statuses and added landing page section | Index reflects current state |

### Landing page scaffold

| File | Purpose |
|------|---------|
| `site/config.toml` | Zola site configuration with status banner and Block A notice in extra vars |
| `site/templates/base.html` | Base HTML template with header, footer, and status banner |
| `site/templates/index.html` | Landing page content: problem solved, when to use, when NOT to use, architecture, quickstart CTA, doc grid, status table |
| `site/static/css/main.css` | Professional dark-theme stylesheet |
| `site/content/_index.md` | Root page front matter |

The landing page includes:
- Prominent `production-ready = NO` banner
- Block A disclaimer (WAIVED/CONDITIONAL)
- Architecture/lineage explanation
- Quickstart CTA linking to repo docs
- Links to existing docs/guides (no duplication)
- Status table with current blockers

### Planning/checklist updates

| File | Change |
|------|--------|
| `docs/production-readiness-v2/07-product-docs-plan.md` | Tasks 1, 3, 4, 7, 8 marked complete; OpenAPI generation deferred |
| `docs/production-readiness-v2/10-evidence-checklist.md` | Phase 7 items 7.6–7.10 added and marked complete |

## What was NOT validated

| Item | Reason |
|------|--------|
| Zola build | `zola` binary is not installed in this environment. The scaffold uses standard Zola conventions and is expected to build when `zola build` is run. `make site-build` target is optional and documented as such. |
| Fresh-user test (DOC-1) | Still not performed. Independent fresh-user test remains required to close DOC-1. |
| Target-host guide validation | Guides were expanded from local code and existing evidence; no new target-host actions were taken. |
| OpenAPI spec | Not generated; API guide links to source code instead. |

## Blockers that remain

| Blocker | Status | Impact |
|---------|--------|--------|
| DOC-1 fresh-user test | NOT performed | Cannot claim quickstart is fully validated for new users |
| Block A (real owned domain/DNS) | WAIVED/CONDITIONAL | Cannot claim full G2 closure; DuckDNS accepted for pilot only |
| HA / multi-node | Not implemented | Production deployment at scale requires Phase 9 work |
| OpenAPI spec generation | Deferred | API guide is manual; machine-readable spec is future work |

## Verification performed

- [x] All required sections present in concepts guide (intent, proposal, policy, capability, approval, rollback, provenance, lineage, adapter, R0–R3)
- [x] API guide covers all 28 endpoints from `crates/ferrum-gateway/src/server.rs`
- [x] Operator guide includes backup/restore, token rotation, monitoring, incident response
- [x] Adapter reference includes all 5 adapters with rollback and risk caveats
- [x] Zola scaffold files exist (config.toml, templates, CSS, content)
- [x] No production-ready/full-G2/Block-A-closed claims in any new or updated file
- [x] Grep check for "production-ready" confirms all occurrences are in caveat banners or status tables

## Non-claims

- **NOT a public website**: The `site/` scaffold is local-only. No deployment, domain, or hosting is configured.
- **NOT validated by build**: Zola build was not run locally due to missing binary.
- **NOT a substitute for OpenAPI**: The API guide is human-readable reference; a machine-readable spec is planned.
- **NOT changing production posture**: These are documentation-only changes.
