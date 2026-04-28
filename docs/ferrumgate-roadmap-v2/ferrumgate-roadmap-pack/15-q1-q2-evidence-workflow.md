# 15 — Q1/Q2 Evidence Workflow

## Purpose

This document defines how evidence is recorded for Q1/Q2 gate passes, including
the `docs/artifacts/<date>/` directory structure, file naming conventions, the
artifact note template, and the gate evidence checklist.

This document is a **living reference** for agents and engineers executing
Q1-P1 through Q2-P8 and any subsequent quarter packages.

> **V1 boundary note**: Evidence recorded under `docs/artifacts/<date>/` documents
> the state of work at a point in time. It does not change the v1 support contract.
> Q1 evidence may close v1 accepted risks; Q2 evidence is entirely post-v1 scope.

---

## 1. `docs/artifacts/<date>/` Directory Structure

### Purpose

`docs/artifacts/<date>/` is the canonical location for all gate evidence in the
FerrumGate roadmap execution pack. Evidence is any output that demonstrates a
gate criterion has been satisfied: test output, code references, audit notes,
or signed confirmations.

### Directory Naming

```
docs/artifacts/<date>/
```

- `<date>` is `YYYY-MM-DD` in ISO 8601 format
- One directory per day evidence is recorded
- A single gate pass may produce evidence across multiple dates
- The directory is created once per evidence-recording session

**Example**: `docs/artifacts/2026-03-30/`

### File Naming Convention

Files follow a two-digit prefix + descriptive name pattern:

```
<NN>-<short-description>.<ext>
```

- `<NN>` is a sequential two-digit number starting at `01` within each directory
- `<short-description>` is a lowercase dash-separated name
- `<ext>` is `.txt` for plain text, `.md` for markdown, or `.json` for structured data

**Rules**:
- Prefix numbers are assigned in the order files are created within a session
- Gaps in numbering are allowed (e.g., `01`, `02`, `05` is fine)
- File names must not contain spaces
- One logical artifact per file; do not combine unrelated evidence

**Examples**:
```
01-cargo-check.txt
02-cargo-test.txt
03-pdp-audit-notes.txt
04-cap-mark-used-test.txt
05-rollback-class-test.txt
06-lineage-chain-test.txt
07-route-table-reconciliation.txt
manifest.txt
```

### Required Files Per Directory

Every `docs/artifacts/<date>/` directory **must** contain:

| File | Purpose |
|---|---|
| `manifest.txt` | Index of all files in the directory, their purpose, and overall pass/fail |
| At least one evidence file | Test output, code reference, or note confirming a gate criterion |

### Manifest File Format

```markdown
# FerrumGate Evidence Bundle
# Generated: <YYYY-MM-DD>
# Purpose: <what this bundle records>

## Artifact Index

| File | Description | Gate Criterion |
|------|-------------|----------------|
| 01-*.txt | <short description> | <gate ref, e.g., Gate A, v1.1 WS1> |
| 02-*.txt | <short description> | <gate ref> |
| ... | ... | ... |

## Evidence Summary

| Criterion | Status | Evidence File |
|-----------|--------|--------------|
| <criterion> | PASS / FAIL / RISK-ACCEPTED | <filename> |
| ... | ... | ... |

## Notes

- <any additional context>

Overall: ALL PASS | PARTIAL | FAILED
```

**Status values**:
- `PASS` — criterion satisfied by evidence in this bundle
- `FAIL` — criterion not satisfied; must be resolved before gate can close
- `RISK-ACCEPTED` — criterion not satisfied but accepted risk is documented in `19-v1-single-node-support-contract.md`

---

## 2. Artifact Note Template

Use this template when recording non-test evidence (audit notes, code references,
or gate confirmation notes). For test output, record the raw command output directly.

### When to Use the Note Template

Use an artifact note when:
- The evidence is a code reference (file + line) rather than a test run
- An accepted risk is being recorded
- A gate criterion is confirmed by inspection rather than automated test
- A package close confirmation is being recorded

### Template

```markdown
# Artifact Note: <short title>

**Date**: <YYYY-MM-DD>
**Package / Gate**: <e.g., Q1-P4, Gate A, v1.1 WS2>
**Author**: <name or agent identifier>
**Status**: PASS | PARTIAL | RISK-ACCEPTED

## Criterion

> "<exact criterion from the gate or package definition>"

## Evidence

### What was done / found

<Describe the action taken or finding. Be specific: include file paths, function
names, line numbers, or exact test commands used.>

### Result

<What the evidence shows: pass/fail/what was observed.>

### Code Reference (if applicable)

- File: `<relative path from repo root>`
- Location: `<function name>`, line <N>
- Relevant snippet:

```
<code excerpt>
```

## Gate Criterion Link

This note satisfies: <gate reference, e.g., "Gate A evidence: PDP rules deterministic,
no 'maybe' branches">

## V1 Boundary

- [ ] This evidence is for v1 kernel hardening (Q1)
- [ ] This evidence is for post-v1 scope (Q2)

## Next Action (if partial)

<What remains to be done before the criterion can be marked PASS.>
```

### Example Filled Note

```markdown
# Artifact Note: PDP Hard-Rules Audit — Gate A Evidence

**Date**: 2026-03-30
**Package / Gate**: Q1-P3 / Gate A
**Author**: agent-001
**Status**: PASS

## Criterion

> "Gate A: PDP rules must be deterministic — no 'maybe' branches in scope/taint/R3/draft-only enforcement"

## Evidence

### What was done / found

Audited all decision branches in `ferrum-pdp/src/rules/`. Enumerated every
scope enforcement path and confirmed each resolves to exactly one outcome.

### Result

All scope enforcement paths resolve to: ALLOW | DENY | QUARANTINE | APPROVAL_REQUIRED.
All taint scoring paths resolve to a numeric score. R3 `auto_commit=false`
enforcement resolves to block or approval-required. Draft-only revalidation
resolves to revalidate or proceed.

No "maybe" branches found in any of the four rule categories.

### Code Reference

- File: `crates/ferrum-pdp/src/rules/scope_enforce.rs`
- Location: `fn evaluate_scope`, line 47–128
- Relevant snippet:

```
pub fn evaluate_scope(ctx: &RuleContext) -> ScopeResult {
    match ctx.capability_scope() {
        Scope::Exact(p) => ExactScope::check(p, ctx),
        Scope::Subpath(p) => SubpathScope::check(p, ctx),
        Scope::Wildcard => ScopeResult::Deny("wildcard not supported".into()),
    }
}
```

## Gate Criterion Link

This note satisfies: Gate A evidence — PDP rules deterministic, no "maybe" branches

## V1 Boundary

- [x] This evidence is for v1 kernel hardening (Q1)

## Next Action (if partial)

N/A — Gate A satisfied.
```

---

## 3. Q1/Q2 Gate Evidence Checklist

Use this checklist when recording evidence for a gate pass. Each criterion
must have a corresponding evidence file in `docs/artifacts/<date>/`.

### Q1 Gate Chain

```
Q1 Exit Gate ──→ v1.1 Release Gate ──→ Q2 Entry Gate ──→ v1.2 Release Gate
```

---

### v1.1 Release Gate (Q1 Exit Gate)

**Location**: `02-release-plan.md` — Gate evidence (v1.1)

| # | Criterion | Evidence Required | Status Field |
|---|---|---|---|
| WS1 | Prepare-step rollback_class gap closed | Test or code reference showing `rollback_class` propagated at prepare | PASS / RISK-ACCEPTED |
| WS2 | Single-use capability enforced end-to-end at authorize | Test or code reference showing `mark_used` called at authorize path | PASS / RISK-ACCEPTED |
| WS3 | Draft-only revalidated at prepare | Test or code reference showing draft-only check at prepare | PASS / RISK-ACCEPTED |
| WS4 | Full provenance minimum-chain integration test | Test output showing all terminal-path events (authorize + prepare + execute + terminal) in API response | PASS / RISK-ACCEPTED |
| RT | Route table reconciled | Note confirming docs/spec/runtime agree on evaluate endpoint naming | PASS / FAIL |

**Gate pass rule**: All WS1–WS4 are PASS or RISK-ACCEPTED AND RT is PASS.

---

### Q2 Entry Gate

**Location**: `01-quarterly-plan.md` — Q2 Entry precondition

| # | Criterion | Evidence Required | Status Field |
|---|---|---|---|
| Q2-ENTRY | v1.1 release gate passed | `docs/artifacts/<date>/` contains v1.1 gate evidence | EXISTS / MISSING |

**Gate pass rule**: Q2-ENTRY is EXISTS.

---

### v1.2 Release Gate (Q2 Exit Gate)

**Location**: `02-release-plan.md` — Gate evidence (v1.2)

| # | Criterion | Evidence Required | Status Field |
|---|---|---|---|
| FS | fs adapter: backup + hash + restore path | Test output showing full prepare/execute/verify/compensate cycle | PASS / FAIL |
| FS-POLICY | fs adapter: policy pack exists | Test or code reference for path-scoped capability binding | PASS / FAIL |
| GIT | git adapter: before_ref/after_ref + revert path | Test output showing ref capture and revert | PASS / FAIL |
| GIT-POLICY | git adapter: protected branch enforcement | Test showing protected branch rule enforced | PASS / FAIL |
| DB | sqlite adapter: transaction wrapper + rollback | Test output showing transaction and rollback | PASS / FAIL |
| DB-POLICY | sqlite adapter: mutation class classification | Test or code reference for SQL risk mapping | PASS / FAIL |
| ORCH | Gateway orchestration integration | Integration test or code reference showing gateway → adapter wiring | PASS / FAIL |
| DEMO-FS | fs demo: operator-visible execution + lineage | Demo trace showing verify + compensate on real workload | PASS / FAIL |
| DEMO-DB | db demo: operator-visible execution + lineage | Demo trace showing verify + rollback on real workload | PASS / FAIL |

**Gate pass rule**: FS, GIT, DB, ORCH, DEMO-FS, DEMO-DB are all PASS.

---

### Per-Package Evidence Checklist (Reference)

Each work package in `13-q1-work-packages.md` and `14-q2-work-packages.md`
specifies its own `Evidence required` field. The checklist above summarizes
gate-level evidence; per-package evidence fills the gate-level requirements.

**Quick reference per package**:

| Package | Evidence Files to Create |
|---|---|
| Q1-P1 (Proto Shape Lock) | Proto diff or field-lock note; schema sync note |
| Q1-P2 (Store Integrity) | State transition test or code reference; gap note if any |
| Q1-P3 (PDP Hard-Rules Audit) | PDP audit notes; Gate A criterion confirmed |
| Q1-P4 (Cap mark_used + Rollback) | Test/code ref for mark_used at authorize; test/code ref for rollback_class at prepare |
| Q1-P5 (Gateway Lineage) | Test output for full authorize→prepare→execute→terminal chain |
| Q1-P6 (Adversarial Suite) | Adversarial test output per weak spot |
| Q1-P7 (Invariant Matrix Pass) | Full test suite summary; route table reconciliation note; manifest |
| Q2-P1 (Proto Extension) | Proto diff; compile check output |
| Q2-P2 (Store Artifact Persistence) | Test output for artifact save/retrieve |
| Q2-P3 (fs adapter) | Backup/restore test output; path deny test |
| Q2-P4 (git adapter) | Ref capture + revert test; protected branch test |
| Q2-P5 (sqlite adapter) | Transaction/rollback test; mutation classification test |
| Q2-P6 (Gateway Orchestration) | Integration test output; provenance event evidence |
| Q2-P7 (Policy Packs) | Test output for fs, git, db policy pack rules |
| Q2-P8 (End-to-End Demo) | Demo trace for fs verify+compensate; demo trace for db verify+rollback |

---

## 4. Evidence Recording Rules

### What counts as evidence

Valid evidence includes:
- **Test output**: Raw output from `cargo test`, `cargo clippy`, etc.
- **Code reference**: File path + function/line reference to the implementing code
- **Artifact note**: A filled-in note template confirming a criterion is satisfied
- **Manifest summary**: The `manifest.txt` index summarizing all evidence in a directory

### Minimum evidence bar

- **Gate-level**: Every criterion in the gate checklist must have a corresponding
  evidence file or an explicit RISK-ACCEPTED note linked to the v1 support contract.
- **Package-level**: Every package's `Evidence required` field must be addressed
  before the package is marked done.
- **No empty directories**: An `docs/artifacts/<date>/` directory with no evidence
  files is not valid gate evidence.

### v1 boundary constraint

Evidence recorded in `docs/artifacts/<date>/` does **not** modify the v1 support
contract. Accepted risks documented here must also be documented in
`19-v1-single-node-support-contract.md` to be formally accepted. The artifact
directory is evidence of work done, not a contract amendment mechanism.

---

## 5. Quick-Reference: Creating a New Evidence Bundle

### Step by step

1. **Create directory**: `mkdir -p docs/artifacts/YYYY-MM-DD`
2. **Execute the work**: Run tests, write code, audit rules
3. **Name files**: Use `NN-description.txt` pattern; reserve `manifest.txt` for last
4. **Fill in manifest**: List every file with its gate criterion
5. **Verify overall**: Check all gate criteria are PASS or RISK-ACCEPTED
6. **Link from roadmap docs**: Add the evidence path to `02-release-plan.md` gate
   evidence section and `01-quarterly-plan.md` evidence table

### Command conventions for test evidence

```sh
# Unit-level evidence
cargo test --package <crate> -- --nocapture > docs/artifacts/YYYY-MM-DD/NN-cargo-test.txt

# Workspace-level evidence
cargo test --workspace > docs/artifacts/YYYY-MM-DD/NN-cargo-test.txt

# Clippy evidence
cargo clippy --workspace --all-targets -- -D warnings > docs/artifacts/YYYY-MM-DD/NN-clippy.txt 2>&1

# Format check
cargo fmt --all --check > docs/artifacts/YYYY-MM-DD/NN-cargo-fmt.txt 2>&1
```

### Common evidence file names by type

| Evidence Type | Suggested Name |
|---|---|
| Test output (workspace) | `NN-cargo-test.txt` |
| Clippy output | `NN-clippy.txt` |
| Format check | `NN-cargo-fmt.txt` |
| Code reference note | `NN-<area>-note.txt` (e.g., `NN-pdp-audit-note.txt`) |
| Gate criterion confirmation | `NN-<criterion>-confirmed.txt` (e.g., `NN-ws1-rollback-confirmed.txt`) |
| Accepted risk note | `NN-accepted-risk-<issue>.txt` |
| Demo trace | `NN-demo-<name>-trace.txt` |
| Manifest (last) | `manifest.txt` |
