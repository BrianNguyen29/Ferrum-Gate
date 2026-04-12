# Documentation Governance

## Purpose / Scope

This artifact establishes the FerrumGate documentation governance baseline for Phases 1 and 2.
Phase 1 defined canonical hierarchy, the doc family inventory, and conflict hotspots.
Phase 2 adds explicit ownership by domain, review cadence, and a deprecation/archival policy
modeled on the v1 single-node support contract pattern.

This document is the **canonical source of truth** for docs governance policy.
Any doc that describes governance expectations should link here rather than restating them.

## Doc Family Inventory

| File | Role | Domain |
|---|---|---|
| `00-project-canon.md` | Single source of truth for project definition | Core |
| `01-quickstart.md` | Getting-started guide | Onboarding |
| `02-project-overview.md` | High-level description | Core |
| `03-architecture.md` | System architecture | Core |
| `04-runtime-flow.md` | Runtime behavior | Core |
| `05-domain-model.md` | Domain entities and relationships | Core |
| `06-constraints-and-invariants.md` | Invariant specification | Core |
| `07-policy-and-security-model.md` | Security boundaries | Core |
| `08-repository-structure.md` | Code layout | Operations |
| `09-implementation-path.md` | Phase-by-phase build plan | Operations |
| `10-crate-by-crate-plan.md` | Crate-level decomposition | Operations |
| `11-testing-strategy.md` | Test philosophy and coverage | Quality |
| `12-persistence-and-data-model.md` | Storage design | Core |
| `13-adapter-contracts.md` | External adapter interfaces | Core |
| `14-api-and-contracts-map.md` | API surface mapping | Core |
| `15-deployment-and-operations.md` | Deployment guide | Operations |
| `16-release-checklist.md` | Release process | Operations |
| `17-troubleshooting.md` | Known issues and fixes | Operations |
| `18-phase-f-evidence-pack.md` | Phase F consolidated evidence | Core |
| `19-v1-single-node-support-contract.md` | v1 single-node support contract | Operations |
| `20-v2-single-node-production-support-contract.md` | v2 single-node support contract (**✅ RATIFIED**) | Operations |
| `implementation-path/60-long-term-vision.md` | Long-term strategic intent (**non-binding**; maps future planes to invariants; distinct from contract and roadmap docs) | Strategy |
| `90-docs-governance.md` | Doc governance policy (this document) | Operations |
| `runbooks/` | Operator runbooks | Operations |
| `implementation-path/41-production-execution-plan.md` | Sequential production evaluation plan (G-E1 → G-E5), per-phase doc update protocol, and commit/PR merge cadence | Operations |
| `implementation-path/44-v2-production-execution-plan.md` | v2 production scope and execution plan (Phase 1–6) (**✅ RATIFIED**) | Operations |
| `implementation-path/45-v2-adapter-promotion-criteria.md` | v2 adapter T2→T1 promotion gates per adapter (**✅ RATIFIED**) | Operations |
| `implementation-path/46-v2-readiness-signoff.md` | v2 sign-off artifact (**✅ RATIFIED**) | Operations |
| `implementation-path/50-post-v2-roadmap.md` | Post-v2 backlog structured as Horizons H1/H2/H3 (near-term policy tooling, U1 backlog, HA, U2/U3/U4) | Operations |
| `diagrams/` | Visual assets | Core |
| `.agents/` | Agent skill files | Tooling |

## Canonical Hierarchy by Domain

1. **Core** — project definition, architecture, domain model, constraints, security, runtime, adapters, API contracts
2. **Operations** — repository structure, implementation path, deployment, release, troubleshooting, runbooks
3. **Quality** — testing strategy
4. **Tooling** — agent skills, diagrams

When a doc claim conflicts, resolution follows the priority order in `docs/README.md`:
`00-project-canon.md` → `06-constraints-and-invariants.md` → `09-implementation-path.md` → `10-crate-by-crate-plan.md` → remaining `docs/`.

## Overlap / Conflict Hotspots

- `03-architecture.md` vs `04-runtime-flow.md` — architectural overview vs runtime sequencing; keep distinct
- `09-implementation-path.md` vs `10-crate-by-crate-plan.md` — roadmap vs crate decomposition; cross-reference but do not duplicate
- `13-adapter-contracts.md` vs `14-api-and-contracts-map.md` — adapter interface specs vs API surface mapping; both exist but serve different audiences (infra vs API consumers)

## Phase 2 Next Actions

~~1. Assign document owners by domain~~ ✅ implemented in Section 2
~~2. Define review cadence (e.g., per milestone)~~ ✅ implemented in Section 3
~~3. Formalize deprecation / archival policy for out-of-tree candidates~~ ✅ implemented in Section 4
4. Add ADR (Architecture Decision Record) section for future structural decisions

## 2. Document Ownership

Each domain is assigned an owner responsible for keeping its docs accurate, consistent, and current.
Ownership covers canonical hierarchy docs and any domain-specific derivative docs.

| Owner | Domain | Scope |
|-------|--------|-------|
| @core-team | Core | `00-project-canon`, `03-architecture`, `04-runtime-flow`, `05-domain-model`, `06-constraints-and-invariants`, `07-policy-and-security-model`, `12-persistence-and-data-model`, `13-adapter-contracts`, `14-api-and-contracts-map`, `18-phase-f-evidence-pack` |
| @ops-team | Operations | `08-repository-structure`, `09-implementation-path`, `10-crate-by-crate-plan`, `15-deployment-and-operations`, `16-release-checklist`, `17-troubleshooting`, `runbooks/`, `90-docs-governance.md` |
| @qa-team | Quality | `11-testing-strategy` |
| @onboarding | Onboarding | `01-quickstart`, `02-project-overview` |
| @tooling | Tooling | `.agents/`, `diagrams/` |
| @core-team | Strategy | `60-long-term-vision.md` |

Ownership responsibilities:
- **Accuracy**: Owner reviews doc changes touching their domain before merge.
- **Consistency**: Owner ensures their docs are consistent with upstream canonical docs (Section 1 priority order applies).
- **Currency**: Owner reviews domain docs at each milestone boundary or when significant implementation changes land.

## 3. Review Cadence

Docs are reviewed on a rolling basis. There is no fixed release schedule for docs — reviews are event-driven.

| Trigger | Review Scope | Owner Action |
|---------|-------------|--------------|
| Milestone completion | All docs in owner's domain | Verify docs reflect implemented state; update if not |
| Significant implementation change | Affected docs in owner's domain | Update to reflect new state; re-align with canonical hierarchy |
| New doc addition | Entire suite | Owner of new doc's domain ensures it is listed in this document and in `docs/README.md` |
| Periodic rolling review | All canonical docs | Annual pass — owner reviews entire domain; stale items flagged as `STALE` |

### STALE Marker

A doc or doc section is marked `STALE` when:
- It has not been reviewed within 12 months
- It describes planned-but-unbuilt behavior that has since changed
- It contradicts a later-implemented canonical doc

`STALE` is a warning to readers, not a deprecation. A `STALE` doc reflects the state at last review and may not reflect current implementation — treat it as potentially out of date until re-reviewed.

## 4. Deprecation & Archival Policy

This section defines how docs are deprecated, archived, or removed, using the same classification
pattern as the v1 single-node support contract (Section 9.2 of `19-v1-single-node-support-contract.md`).

### 4.1 Scope

This policy applies to all documents and document sections in `docs/` except:
- `.agents/` skill files (governed separately)
- `runbooks/` (governed by ops-team ownership)

### 4.2 Change Classification

| Class | Description | Examples |
|-------|-------------|----------|
| **Material scope change** | Removes or reclassifies a previously canonical doc or section; changes canonical priority order; retires an out-of-tree candidate to canonical | Removing a doc from the inventory; promoting an exploratory candidate to canonical; adding a new canonical priority tier |
| **Clarifying change** | Fixes typos, updates evidence links, refines descriptions without changing canonical surface or ownership obligations | Correcting a file path; adding a missing cross-reference; updating an evidence file path |

### 4.3 Deprecation Announcement Process (material scope changes)

For any material scope change:

1. **Advance notice**: A deprecation notice is published as a comment at the top of the affected doc
   describing what is changing, why, and the effective date. The notice is visible in plain text.
2. **Minimum notice period**: The change does not take effect sooner than **30 days** after the notice
   is merged. This gives contributors time to assess impact and update dependent docs.
3. **Doc update**: The affected doc (i.e., the doc being deprecated) is updated to reflect the deprecation at the time
   of announcement, with the deprecated item noted. The deprecated item remains in the inventory with
   a `DEPRECATED` marker and effective date until the effective date.
4. **No automatic migration**: There is no automated tooling for migrating doc references.

**What this policy does not require:**
- A fixed support window (e.g., "12 months of support after deprecation").
- A semantic-versioning policy for docs.

### 4.4 Doc and Section Removal

A doc or doc section is **never removed** without:
1. First being marked `DEPRECATED` with an effective date.
2. The deprecation announcement process in Section 4.3 being followed.

After the effective date, the deprecated doc or section may be removed without further notice.

### 4.5 Canonical Additions

New docs may be added at any time. Additions do not require a deprecation period but do require:
- This document to be updated to include the new doc in the inventory
- `docs/README.md` to be updated to include the new doc in the index
- Ownership to be assigned per Section 2

### 4.6 Relationship to Exploratory Candidates

Out-of-tree exploratory candidates (e.g., `40-out-of-tree-sqlite-performance-candidate.md`) are
non-canonical by default. They may be promoted to canonical via a material scope change process.
Promotion requires this document to be updated to reflect the new canonical status.

---

## Out-of-Tree SQLite Candidate Warning

`docs/implementation-path/40-out-of-tree-sqlite-performance-candidate.md` is **non-canonical**: it is an unmerged exploratory performance candidate and must not be cited as authoritative or relied upon for current implementation decisions. It will be re-evaluated in a future phase.
