# 02 — Release plan

## Release execution pack — Q1–Q2 sequencing and gates

This document (`02`) is the release-gate reference for the execution pack.
It defines scope per release and specifies what evidence is required at each gate.

### Cross-release dependencies
| From | To | Gate |
|---|---|---|
| v1.1 exit gate passed | v1.2 work begins | Q1 exit gate is the entry precondition for Q2 |

### Evidence expectations per release gate
Each release gate requires at minimum one of:
- A test output summary (passing integration test run)
- A short note in `docs/artifacts/<date>/` confirming the gate item is resolved or risk-accepted
- A code reference pointing to the implementing file + line

---

## Release taxonomy

- `v1.1-kernel-hardening`
- `v1.2-governed-engineering-changes-beta`
- `v1.3-self-hosted-commercial-beta`
- `v1.4-mcp-governance-beta`
- `v1.5-enterprise-evidence-alpha`

> **Canonical v1 boundary**: FerrumGate v1 single-node support is defined exclusively
> by `19-v1-single-node-support-contract.md`. All releases listed above (v1.2 through
> v1.5) are **post-v1 scope**. The existence of code in the repo for adapters, CLI
> commands, or routes not listed in the v1 support contract does not expand v1 scope.
> Only a formal amendment to the v1 support contract can change what is supported in v1.

---

## Release 1 — v1.1 Kernel Hardening

### Scope
- Defect closure for all v1 accepted risks (Weak Spots 1–4 from the v1 support contract)
- Docs/spec/route/OpenAPI synchronization
- Invariant matrix pass

### In scope checklist
- [x] Close prepare-step rollback class gap (Weak Spot 1) — evidence: Q1-P6 adversarial test `test_r3_contracts_have_auto_commit_false`; Q1-P7 gate confirmed (`08-q1-p7-invariant-matrix-pass-evidence.md`)
- [x] Enforce single-use capability end-to-end at authorize path (Weak Spot 2) — evidence: Q1-P6 adversarial test `test_authorize_can_only_be_called_once`; Q1-P7 gate confirmed (`08-q1-p7-invariant-matrix-pass-evidence.md`)
- [x] Revalidate draft-only on prepare or equivalent safe checkpoint (Weak Spot 3) — evidence: Q1-P6 adversarial test `test_draft_only_intent_cannot_reach_prepare_by_bypassing_evaluate`; Q1-P7 gate confirmed (`08-q1-p7-invariant-matrix-pass-evidence.md`)
- [x] Add full provenance minimum-chain integration test (Weak Spot 4) — evidence: Q1-P6 adversarial test `test_lineage_adversarial_partial_execution_no_terminal`; Q1-P7 gate confirmed (`08-q1-p7-invariant-matrix-pass-evidence.md`)
- [x] Reconcile evaluate endpoint docs/spec/runtime — evidence: route parity 19/19 confirmed (`08-q1-p7-invariant-matrix-pass-evidence.md`)
- [x] Publish canonical route table; ensure OpenAPI and docs match runtime — evidence: OpenAPI vs runtime route parity 19/19 confirmed (`08-q1-p7-invariant-matrix-pass-evidence.md`)
- [x] Update release checklist — evidence: gate evidence recorded in `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`; Q1 exit gate passed

### Out of scope
- Any adapter-backed real implementation
- Any new route not in the v1 support contract
- HA, multi-node, postgres, operator UI

> **V1 boundary**: All items above are defect closure within the existing v1 support
> contract. No item in this release expands the support contract. Where the v1 support
> contract lists an accepted risk, this release aims to reduce or close that risk
> without redefining the contract boundary.

### Release gate
- All four Weak Spots (1–4) have passing integration tests or explicit risk-accepted documentation
- Route table matches v1 support contract; no undocumented routes claimed as v1-supported
- OpenAPI spec and docs are in sync with runtime route table

### Gate evidence (v1.1)
Record in `docs/artifacts/<date>/`:
- Weak Spot 1: test or code reference showing prepare-step rollback_class fix
- Weak Spot 2: test or code reference showing mark_used called at authorize
- Weak Spot 3: test or code reference showing draft-only revalidated at prepare
- Weak Spot 4: lineage chain integration test output showing all terminal-path events
- Route table reconciliation note confirming docs/spec/runtime in sync

**v1.1 gate evidence (Q1 exit gate):** `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md` — cargo test --workspace passed; cargo test -p ferrum-gateway passed; route parity 19/19 confirmed; WS1-WS4 adversarial chain confirmed via Q1-P6.

**Q1 exit gate is passed for v1.1 scope. Q2 entry gate is satisfied.**

---

## Release 2 — v1.2 Governed Engineering Changes Beta

### Scope
- first real adapter-backed recovery semantics for engineering workflows

### In scope checklist
- [ ] fs adapter real implementation
- [ ] git adapter real implementation
- [ ] sqlite adapter real implementation
- [ ] db mutation policy/risk map
- [ ] verify path for fs/git/db
- [ ] examples for repo/file/db governed changes
- [ ] approval/quarantine templates for engineering workflows

### Out of scope
- MCP general runtime governance
- enterprise evidence plane
- HA and distributed deployment

> **V1 boundary**: HA, distributed deployment, and multi-node are explicitly
> unsupported in v1. MCP runtime governance and enterprise evidence are
> post-v1 scope. These remain out of scope for v1.2 regardless of any
> adapter code that may exist in the repo.

### Release gate
- end-to-end demo for file mutation
- end-to-end demo for git ref mutation
- end-to-end demo for db mutation with rollback
- operator can inspect lineage and execution for all 3 demos

### Gate evidence (v1.2)
Record in `docs/artifacts/<date>/`:
- fs adapter: test output showing backup + hash + restore path
- git adapter: test output showing before_ref/after_ref + revert path
- sqlite adapter: test output showing transaction wrapper + rollback
- Policy pack samples for fs and db workflows
- Demo trace showing operator-visible execution + lineage for fs mutation

---

## Release 3 — v1.3 Self-hosted Commercial Beta

### Scope
- productization for private deployments

### In scope checklist
- [ ] postgres support
- [ ] operator UI beta
- [ ] observability pack
- [ ] RBAC/auth for operator plane
- [ ] Docker/Compose packaging
- [ ] Helm draft or Kubernetes manifests
- [ ] production-like deployment docs
- [ ] support playbook / troubleshooting update

### Out of scope
- cloud multi-tenant SaaS
- marketplace integrations
- global edge deployment

### Release gate
- design partner can deploy in private environment
- basic incident flow can be handled via UI
- backup/restore tested in staging-like setup

---

## Release 4 — v1.4 MCP Governance Beta

### Scope
- runtime/tool governance for open runtime and MCP-style integrations

### In scope checklist
- [ ] tool call -> proposal mapping
- [ ] gateway/wrapper integration for MCP
- [ ] tool/resource scoped capability binding
- [ ] trust/taint propagation for tool outputs
- [ ] tool policy packs
- [ ] sample open runtime integration

### Out of scope
- all frameworks supported equally
- local computer-use / GUI automation

### Release gate
- one reference runtime integrated
- provenance preserved across runtime/tool boundary
- approval/quarantine/draft-only demonstrated for tool actions

---

## Release 5 — v1.5 Enterprise Evidence Alpha

### Scope
- audit/evidence layer for enterprise buyers

### In scope checklist
- [ ] tamper-evident ledger alpha
- [ ] signed approvals alpha
- [ ] evidence export bundles
- [ ] incident review workflow
- [ ] provenance graph export improvements

### Out of scope
- full compliance automation suite
- regulatory certification claims

### Release gate
- one execution can produce an exportable evidence bundle
- tamper-evident or hash-chain behavior documented and test-covered
- audit artifact can be consumed by operator without raw DB inspection
