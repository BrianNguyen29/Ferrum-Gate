# ⚠️ TEMPLATE ONLY — NOT EVIDENCE

> **Status**: Blank operator-copyable template. **NOT OPERATOR EVIDENCE.**
> **Purpose**: Provides a pre-structured evidence bundle directory that operator copies before a real target run.
> **Constraint**: Do NOT treat this template as real evidence. Do NOT mark G2 complete from this template.
> **Scope**: Single-node SQLite Path 2 target deployment evidence collection.

---

## Purpose

This directory is a **blank evidence bundle template** for FerrumGate v1 Path 2 target deployment evidence collection.

**Do NOT:**
- Submit this template as G2 or operator evidence
- Mark G2 gates complete using this template
- Claim production-ready from this template
- Use placeholder values as real target values

**Do:**
- Copy this template to a dated target-run directory before use
- Fill required metadata fields with real (non-shared) operator-provided values
- Collect real evidence from the target environment into this structure
- Replace `.gitkeep` files with actual evidence outputs

---

## Instructions for Operator

### Step 1 — Copy Template to Dated Directory

```bash
# Copy template to a timestamped target-run directory
cp -r docs/implementation-path/path2-evidence-bundle-template /tmp/ferrumgate-target-run-YYYYMMDD
cd /tmp/ferrumgate-target-run-YYYYMMDD
```

### Step 2 — Fill Required Metadata

Open `METADATA.md` and fill all required fields:
- Operator name
- Target host FQDN/IP
- Target deployment date
- Bearer token (generate fresh: `openssl rand -hex 32`)
- Config file path
- Backup directory path

### Step 3 — Execute Target Drills

Follow the operator runbook [`62-path-2-operator-runbook.md`](./62-path-2-operator-runbook.md) to execute:

1. Phase 2 probes → capture output to `00-probes/`
2. Auth smoke → capture output to `01-auth-smoke/`
3. Backup/restore drill → capture output to `02-backup-restore/`
4. D1–D6 compensation drills → capture output to `03-d1-d6-drills/`
5. Metrics collection → capture output to `04-metrics/`
6. Log files → capture output to `05-logs/`

### Step 4 — Complete G2 Evidence

After collecting real evidence, complete the G2 evidence sections in [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md).

### Step 5 — Sign Off

Complete and sign [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) only after all G2 gates are satisfied.

---

## Directory Structure

```
path2-evidence-bundle-template/
├── README.md              ← THIS FILE
├── MANIFEST.md            ← File checklist (operator fills)
├── METADATA.md            ← Required metadata fields (operator fills)
│
├── 00-probes/             ← Probe outputs (healthz, readyz, readyz/deep, metrics)
│   └── .gitkeep           ← Replace with actual probe output files
│
├── 01-auth-smoke/         ← Auth smoke test outputs
│   └── .gitkeep           ← Replace with auth test output
│
├── 02-backup-restore/     ← Backup/restore drill evidence
│   └── .gitkeep           ← Replace with restore drill log
│
├── 03-d1-d6-drills/      ← Compensation drill evidence (D1–D6)
│   ├── d1-filesystem/     ← FileWrite, FileDelete, FileMove drill outputs
│   │   └── .gitkeep
│   ├── d2-git-local/      ← GitCommit, GitBranchCreate, GitTagCreate drill outputs
│   │   └── .gitkeep
│   ├── d3-git-remote/     ← GitRemotePush drill outputs (baseline + fail-closed)
│   │   └── .gitkeep
│   ├── d4-http-replay/    ← HTTP POST replay drill outputs
│   │   └── .gitkeep
│   ├── d5-sqlite/         ← SQLite INSERT, UPDATE, DELETE drill outputs
│   │   └── .gitkeep
│   └── d6-maildraft/      ← Maildraft drill outputs
│       └── .gitkeep
│
├── 04-metrics/            ← Prometheus metrics captures
│   └── .gitkeep           ← Replace with metrics output
│
├── 05-logs/               ← Server and drill logs
│   └── .gitkeep           ← Replace with log files
│
├── 06-g2-evidence/        ← G2 gate evidence references (links to doc 59)
│   └── .gitkeep           ← Populated after doc 59 completion
│
└── 07-signoff/            ← Operator signoff documents
    └── .gitkeep           ← Populated after doc 54 completion
```

---

## Explicit Non-Claims

- ☐ **G2 complete**: No G2 gate is marked complete in this template
- ☐ **Pilot authorized**: No pilot authorization implied or stated
- ☐ **Production-ready**: No production-ready claim
- ☐ **Operator signed**: All signature fields remain blank
- ☐ **Real evidence**: This is a blank template; no real evidence is contained herein

---

## Required Metadata Fields

See `METADATA.md` for required operator-filled fields.

---

## Cross-References

| Doc | Purpose |
|-----|---------|
| [62-path-2-operator-runbook.md](./62-path-2-operator-runbook.md) | Full command sequences |
| [65-path-2-target-questionnaire.md](./65-path-2-target-questionnaire.md) | Required operator inputs |
| [59-pilot-readiness-evidence-packet.md](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence packet |
| [54-operator-signoff-packet.md](./54-operator-signoff-packet.md) | Pilot acceptance signoff |

---

*Template created: 2026-05-06. Blank evidence bundle template — NOT OPERATOR EVIDENCE.*
