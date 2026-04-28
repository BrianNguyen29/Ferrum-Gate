# 12 — Doc governance and status tags

## Purpose

This document defines how roadmap documents in the v2 pack are maintained,
what status tags mean, and how changes to roadmap docs should be handled.
It is a **living reference** for anyone editing or reviewing roadmap docs.

This document does **not** change the v1 support contract
(`19-v1-single-node-support-contract.md`). The v1 support contract is
the only authoritative source for v1 scope; this document only governs
the roadmap pack.

---

## 1. Document types in the roadmap pack

Each doc in the pack has a role:

| File | Role |
|---|---|
| `README.md` | Entry point and hierarchy guide |
| `00-roadmap-charter.md` | Product positioning and strategic intent |
| `01-quarterly-plan.md` | Quarterly goals and exit gates |
| `02-release-plan.md` | Release taxonomy and scope per release |
| `03-crate-workplan.md` | Crate-level work breakdown |
| `04-api-roadmap.md` | API surface evolution |
| `05-adapter-roadmap.md` | Adapter strategy and priority |
| `06-testing-and-quality-gates.md` | (not modified in this pass) |
| `07-operator-and-deployment-plan.md` | Ops and deployment story |
| `08-agent-execution-rules.md` | Agent behavior rules |
| `09-backlog-and-deferred-tracks.md` | Deferred but valuable work |
| `10-master-checklist.md` | Consolidated release checklist |
| `11-current-state-baseline.md` | Repo facts snapshot |
| `12-doc-governance-and-status-tags.md` | This document — doc governance rules |
| `13-q1-work-packages.md` | Execution-ready work packages for Q1 kernel hardening |
| `14-q2-work-packages.md` | Execution-ready work packages for Q2 governed engineering changes beta |
| `15-q1-q2-evidence-workflow.md` | Evidence workflow: `docs/artifacts/<date>/` structure, naming conventions, artifact note template, and gate evidence checklist |

---

## 2. Status tags

Use these tags when marking items in roadmap docs:

### Status values for work items

| Tag | Meaning |
|---|---|
| `canonical` | The authoritative current state; used for v1 support contract only |
| `committed` | Firmly planned; not expected to change without formal change control |
| `planned` | In plan but not committed; subject to revision based on progress or feedback |
| `in-progress` | Actively being worked |
| `deferred` | Valuable but not in current cycle; in `09-backlog-and-deferred-tracks.md` |
| `done` | Completed; still needs to be kept accurate in docs |
| `blocked` | Cannot proceed; dependency or external constraint not yet resolved |
| `removed` | Was planned; now explicitly removed; retain record with rationale |

### Scope tags for v1 boundary markers

When an item needs to be flagged for v1 scope implications:

| Tag | Meaning |
|---|---|
| `v1-scope` | Within the v1 single-node support contract |
| `post-v1-scope` | Outside v1; future work after v1 is stable |
| `v1-contract-note` | Annotation on a post-v1 item clarifying relationship to v1 boundary |

---

## 3. When roadmap docs can change

### Routine changes (permissive)

- Adding new `planned` items to backlog sections
- Updating item descriptions for clarity without changing scope
- Fixing typos, broken links, or formatting
- Updating status tags as work progresses (e.g., `planned` → `in-progress` → `done`)
- Adding `v1-contract-note` annotations to clarify boundary relationships

### Changes requiring higher scrutiny

- **Expanding v1 scope**: Any change that would expand what is supported in v1
  must be gated by a formal amendment to `19-v1-single-node-support-contract.md`.
  Roadmap docs cannot create new v1 support obligations.
- **Moving deferred items to committed**: Should be reviewed against revisit triggers
  in `09-backlog-and-deferred-tracks.md` (design partner request, release gate unblock,
  architecture pressure, usage evidence).
- **Removing committed items**: Should be documented with rationale and flagged
  to stakeholders.
- **Changing exit gates or Q1/Q2/Q3/Q4 boundaries**: These affect committed plans
  and should be reviewed before merging.

### Changes that should never happen in this pack

- Claiming a post-v1 item is v1 scope without a formal v1 contract amendment
- Describing adapter work as "v1 supported" without a v1 contract amendment
- Listing multi-node, HA, postgres, operator UI, or MCP as v1 features
- Removing or downgrading the v1 boundary reminder sections added to each doc

---

## 4. Keeping docs consistent with the v1 support contract

The v1 support contract (`19-v1-single-node-support-contract.md`) is the
**canonical source of truth** for v1 scope. Any roadmap doc that references
v1 scope must agree with that document.

### The rule

> If a roadmap doc says something is in v1 scope, and `19-v1-single-node-support-contract.md`
> does not list it, the roadmap doc is wrong — not the v1 support contract.

### How to check

Before writing or approving any roadmap doc change that touches v1 scope:

1. Open `19-v1-single-node-support-contract.md`
2. Check if the claimed v1 item is listed under "Supported" sections
3. Check if any exclusion or known limitation covers it
4. If the roadmap doc claims something is v1-supported but it is not in that document,
   the roadmap doc must be corrected

### What "v1 boundary notes" do

Each roadmap doc in this pack has `v1 boundary note` sections added to make
the relationship between roadmap plans and v1 scope explicit. These notes:

- Clarify that planned items are post-v1 scope
- Point to the v1 support contract as the authoritative boundary
- Prevent accidental scope drift in roadmap language

Do not remove these notes. They are the structural mechanism for maintaining
boundary integrity as the roadmap evolves.

---

## 5. Adding new docs to the pack

New docs numbered `13` and above may be added to the pack by following
these rules:

1. Add the new doc to the reading order in `README.md`
2. Add the new doc to the document type table in section 1 of this file
3. Include a `v1 boundary note` if the new doc touches anything related to v1 scope
4. Update `10-master-checklist.md` if the new doc introduces new work items

---

## 6. Maintenance triggers

Roadmap docs in this pack should be reviewed and potentially updated when:

| Trigger | Action |
|---|---|
| New quarter starts | Review `01-quarterly-plan.md` and `10-master-checklist.md` for status updates |
| v1 contract is formally amended | Audit all docs with v1 boundary notes; update or remove notes as needed |
| A release is shipped | Update release taxonomy in `02-release-plan.md`; mark delivered items done |
| New adapter or route lands in v1 router | Cross-check with `04-api-roadmap.md` and `11-current-state-baseline.md` |
| Accepted risk is resolved in v1 | Update `19-v1-single-node-support-contract.md` (separate from this pack) |
| Deferred item is picked up | Move from `09-backlog-and-deferred-tracks.md` to appropriate quarterly plan |

---

## 7. Non-goals for this pack

- This pack does **not** override or amend `19-v1-single-node-support-contract.md`
- This pack does **not** create new v1 support commitments
- This pack does **not** guarantee any planned item will be delivered
- This pack does **not** represent a binding commitment from the project
- This pack does **not** include product code — it is documentation only

---

## 8. Relationship between docs

```
19-v1-single-node-support-contract.md  ← THE authoritative v1 boundary (canon)
                                             ↑
                                             │ (informs boundary notes in:)
                                             │
README.md ──→ 00-10 ──→ (standard roadmap docs)
       │
       └──→ 11-current-state-baseline.md ──→ (repo facts; cross-check with v1 contract)
       │
       └──→ 12-doc-governance-and-status-tags.md (this file)
```

- `19-v1-single-node-support-contract.md` is canon for v1 scope — do not contradict it
- `11` provides a consistent snapshot of current repo state — used by agents and engineers
- `12` governs how all docs in the pack (including future additions) are maintained
- `00-10` describe roadmap planning layered on top of v1; Q1 may harden v1 behavior, while Q2+ are post-v1 work. None of them should be interpreted as v1 scope claims unless the v1 contract says so
