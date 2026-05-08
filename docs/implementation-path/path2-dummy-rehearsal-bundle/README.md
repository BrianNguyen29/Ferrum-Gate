# ⚠️ DUMMY / LOCAL-TEST ONLY — NOT OPERATOR EVIDENCE

> **Status**: LOCAL-TEST/DUMMY/REHEARSAL ONLY — NOT G2 Evidence, NOT Production Ready, NOT Operator Evidence
> **Purpose**: Provides a safe, local-only Path 2 rehearsal bundle for tooling validation, runbook practice, and orchestration script development.
> **Scope**: Local host only. Single-node SQLite. No target host, SSH, domain, or TLS required.
> **Constraint**: Do NOT modify canonical docs 54, 58, 59, 63, or 65 with dummy values. Do NOT claim G2/pilot/production readiness from this bundle's outputs. Do NOT use dummy values as real target values.

---

## ⚠️ CRITICAL WARNINGS — READ BEFORE USE

**THIS BUNDLE IS LOCAL-TEST/DUMMY ONLY:**

- All values in this bundle are **ARTIFICIAL/LOCAL-ONLY** — they do NOT represent any real target environment
- Evidence generated using this bundle is **LABELED "LOCAL-TEST/DUMMY/NOT OPERATOR EVIDENCE"** and does NOT constitute G2 completion
- No production-ready, pilot-accepted, G2-gate-complete, or operator-signed claim is made or implied
- This bundle is for **local rehearsal, runbook practice, and tooling validation ONLY**

**EXPLICITLY NOT CLAIMED:**
- ☐ G2 complete (any gate)
- ☐ Pilot authorized
- ☐ Production-ready
- ☐ Operator signed
- ☐ HTTP workload trigger active
- ☐ PostgreSQL/multi-node/HA operational

---

## Purpose

This bundle provides a **local-test-only** Path 2 rehearsal environment using safe artificial values. It enables:
- Orchestration script development and validation (`run_dummy_path2_rehearsal.sh`)
- Local auth smoke checks (`run_local_auth_smoke.sh`)
- Local restore drills (`run_local_restore_drill.sh`)
- D1-D6 drill execution in safe environment (`run_d1_d6_drills.py`)
- Evidence skeleton generation (`generate_evidence_skeleton.py`)
- Runbook command-sequence practice without requiring target environment access
- Tooling validation before actual target deployment

Real target deployment requires completing the **canonical** evidence bundle template at `../path2-evidence-bundle-template/` with actual infrastructure values provided by the operator.

---

## Bundle Structure

```
path2-dummy-rehearsal-bundle/
├── README.md              ← THIS FILE — DUMMY/LOCAL-TEST ONLY
├── MANIFEST.md           ← File checklist for dummy rehearsal phases
│
├── 00-config/            ← Dummy config values (from doc 69)
│   └── .gitkeep          ← Placeholder for local config output
│
├── 01-probes/            ← Probe outputs (healthz, readyz, readyz/deep, metrics)
│   └── .gitkeep          ← Placeholder for probe output
│
├── 02-auth-smoke/        ← Auth smoke test outputs
│   └── .gitkeep          ← Placeholder for auth test output
│
├── 03-backup-restore/     ← Backup/restore drill evidence
│   └── .gitkeep          ← Placeholder for restore drill log
│
├── 04-d1-d6-drills/      ← Compensation drill evidence (D1–D6)
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
├── 05-evidence-skeleton/ ← Generated evidence skeleton outputs
│   └── .gitkeep          ← Placeholder for skeleton output
│
└── 06-summary/           ← Rehearsal summary and findings
    └── .gitkeep          ← Placeholder for summary output
```

---

## Relationship to Other Docs

| Doc | Relationship | Notes |
|-----|--------------|-------|
| `../path2-evidence-bundle-template/` | Sister template | Real evidence bundle template; operator copies before target run |
| `69-local-dummy-target-values.md` | Source of dummy values | Provides artificial/local-only dummy values for rehearsal |
| `61-path-2-execution-plan.md` | Reference | Path 2 execution plan context |
| `62-path-2-operator-runbook.md` | Reference | Real operator runbook; not used in dummy rehearsal |
| `64-local-staging-simulation-guide.md` | Reference | Broader local simulation guide |
| **NOT 54** | Do NOT modify | Canonical operator signoff; kept untouched |
| **NOT 58** | Do NOT modify | Canonical drill evidence template; kept untouched |
| **NOT 59** | Do NOT modify | Canonical pilot readiness packet; kept untouched |
| **NOT 63** | Do NOT modify | Canonical target environment spec; kept untouched |
| **NOT 65** | Do NOT modify | Canonical target questionnaire; kept untouched |

---

## Dummy Values Source

Dummy values are sourced from:
- `docs/implementation-path/69-local-dummy-target-values.md` — provides artificial/local-only dummy values

---

## Orchestration Script

The orchestration script `scripts/run_dummy_path2_rehearsal.sh` runs these phases:

1. **Phase 0 — Preflight**: Validate script syntax and dependencies
2. **Phase 1 — Dummy Config**: Create dummy config using doc 69 values
3. **Phase 2 — Local Probes**: Run probe sequence on temporary local instance
4. **Phase 3 — Auth Smoke**: Run `run_local_auth_smoke.sh` locally
5. **Phase 4 — Restore Drill**: Run `run_local_restore_drill.sh` locally
6. **Phase 5 — D1-D6 Drills**: Run `run_d1_d6_drills.py` locally
7. **Phase 6 — Evidence Skeleton**: Generate evidence skeleton from drill outputs
8. **Phase 7 — Summary**: Generate watermarked rehearsal summary

---

## Evidence Label

All evidence from dummy rehearsal must be clearly labeled:
```
# FerrumGate v1 — LOCAL-TEST/DUMMY EVIDENCE
# NOT G2 EVIDENCE — NOT OPERATOR EVIDENCE — NOT PRODUCTION READY
```

---

## Boundaries — What This Bundle Does NOT Provide

| Dimension | What This Bundle Provides | What This Bundle Does NOT Provide |
|-----------|--------------------------|----------------------------------|
| Real target values | Artificial placeholders | Real infrastructure values |
| SSH access | n/a | Real SSH credentials |
| TLS certificates | n/a | Real certificates |
| G2 evidence | None | G2.1–G2.8 evidence |
| Operator signoff | None | Doc 54 signature |
| Pilot authorization | None | Pilot acceptance |
| Production readiness | None | Production-ready claim |

---

## Disclaimer

**LOCAL-TEST ONLY — NOT G2 EVIDENCE — NOT OPERATOR EVIDENCE — NOT PRODUCTION READY**

- No G2 complete claim is made by using this dummy bundle
- No pilot accepted or production-ready claim is made
- Dummy evidence is labeled "LOCAL-TEST/DUMMY" and cannot substitute for target environment evidence
- FerrumGate v1 is RC-ready/conditional for single-node SQLite only
- PostgreSQL/multi-node/HA are not implemented
- For G2 completion, operator must complete canonical bundle at `../path2-evidence-bundle-template/` with real target values and execute target-environment drills per `61-path-2-execution-plan.md`
- Phase 3 remains blocked until G2/operator evidence is complete

---

*Created: 2026-05-08. LOCAL-TEST/DUMMY ONLY bundle — no G2 claim, no production-ready claim, no operator evidence.*
*Canonical docs 54, 58, 59, 63, 65 kept untouched.*
*Orchestration script: `scripts/run_dummy_path2_rehearsal.sh`*