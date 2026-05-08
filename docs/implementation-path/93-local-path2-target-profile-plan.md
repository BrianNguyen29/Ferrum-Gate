# 93 — Local Path 2 Target Profile — Plan

> **Status**: Created 2026-05-08
> **Purpose**: Document the local-only Path 2 target profile alternative when real target values are unavailable
> **Scope**: Single-node SQLite Path 2 only. Local-only. No real target values, SSH, domain, TLS, or secrets.
> **Constraint**: No G2/pilot/production-ready claim. No canonical doc 54/58/59/63/65 population. Ephemeral local token only.

---

## Purpose

When real target values (host, credentials, paths) are not yet available, this plan provides an
**alternative local-only target profile** that:

1. Validates tooling and runbook steps against a target-like directory structure
2. Uses a temporary local root with generated local-only token and in-memory SQLite (file-backed SQLite does not work in /tmp in this environment)
3. Produces outputs labeled "LOCAL-ONLY — NOT TARGET EVIDENCE"
4. Does NOT complete G2 or claim pilot authorization

This enables engineering to validate the full Path 2 tooling chain locally without waiting for
operator-provided target values.

---

## What This Is NOT

| Claim | Reality |
|-------|---------|
| NOT G2 evidence | All G2.1–G2.8 remain pending operator action |
| NOT target readiness | No real target values were collected or validated |
| NOT pilot authorized | Pilot remains unauthorized until operator signs doc 54 |
| NOT production-ready | FerrumGate v1 remains RC-ready/conditional |
| NOT operator signoff | Doc 54 remains unsigned |
| NOT real secrets | Only ephemeral locally-generated tokens in temp dirs |

---

## Profile Structure

When the local profile script runs, it creates a temporary target-like directory structure:

```
/tmp/ferrum-local-target-profile-XXXXX/
├── etc/ferrumgate/
│   ├── ferrumgate.toml    # Generated config (NOT real target config)
│   └── ferrumd.env        # Generated env (NOT real target env)
├── var/lib/ferrumgate/
│   └── (DB path; in-memory SQLite used)
├── var/backups/ferrumgate/
│   └── *.db               # Backup files
├── evidence/               # (empty — for real target evidence)
└── logs/
    └── ferrumd.log
```

Note: In-memory SQLite (sqlite::memory:) is used because file-backed SQLite does not work
in /tmp directories in this environment (SQLite error 14). The profile structure is still
fully created at the temp path.

This mimics the structure an operator would create on a real target host, enabling
runbook validation without real infrastructure.

---

## Script: `run_local_path2_target_profile.sh`

**Location**: `scripts/run_local_path2_target_profile.sh`
**Executable**: Yes
**No Docker/Compose required**: Uses local binaries only

### Usage

```bash
bash scripts/run_local_path2_target_profile.sh              # Run with defaults
bash scripts/run_local_path2_target_profile.sh --keep-output # Keep output directory after run
bash scripts/run_local_path2_target_profile.sh --output-dir /custom/path # Custom output dir
bash scripts/run_local_path2_target_profile.sh --skip-auth-smoke # Skip local auth smoke check
bash scripts/run_local_path2_target_profile.sh --skip-backup # Skip backup/restore drill
bash scripts/run_local_path2_target_profile.sh --help        # Show help
```

### Phases

| Phase | Name | What It Does |
|-------|------|--------------|
| 0 | Preflight | Validate dependencies, port availability |
| 1 | Profile Setup | Create temp target-like root (etc/, var/lib/, var/backups/, evidence/, logs/) |
| 2 | Token Generation | Generate ephemeral local-only bearer token (not committed) |
| 3 | Config/Env | Write ferrumgate.toml and ferrumd.env to temp locations |
| 4 | ferrumd Start | Start ferrumd with in-memory SQLite on local port |
| 5 | Probes | Run healthz, readyz, readyz/deep, metrics probes |
| 6 | Auth Checks | Verify no-token/wrong-token/correct-token behavior |
| 7 | Backup/Restore | Run backup create/verify and restore drill |
| 8 | Auth Smoke | Run local auth smoke check (calls run_local_auth_smoke.sh) |
| 9 | Artifact Output | Write summary artifact to output dir |

### Constraints

- **No Docker/Compose**: Uses local binaries only
- **In-memory SQLite**: File-backed SQLite does not work in /tmp in this environment (SQLite error 14); profile structure is still fully created
- **No real secrets**: Only locally-generated tokens in temp dirs
- **No canonical docs modified**: Docs 54, 58, 59, 63, 65 NOT touched
- **Ephemeral token**: Generated token is temp and NOT stored in artifacts
- **Cleanup**: Temp dir cleaned up on exit unless `--keep-output` is specified

### CI Workflow

A GitHub Actions workflow is available for CI-hosted RC evidence:

**File**: `.github/workflows/local-profile-evidence.yml`
**Trigger**: `workflow_dispatch` (manual), or on push/PR to relevant files

The workflow:
1. Builds ferrumd and ferrumctl in release mode
2. Runs `run_pre_target_gate.sh` locally
3. Runs `run_local_path2_target_profile.sh --keep-output`
4. Uploads the evidence artifact (retained 30 days)
5. Writes a job summary with explicit non-claims

**CI Evidence Claim**: CI-hosted RC evidence only. NOT G2 evidence. NOT production-ready. NOT target evidence. NOT operator signoff.

**Usage**:
```bash
# Manual trigger
gh workflow run local-profile-evidence.yml

# Or via web UI: Actions tab > Local Profile Evidence > Run workflow
```

### Output

The script produces:

- `local-path2-target-profile-result.md` — Watermarked summary artifact
- `auth_smoke_output.txt` — Local auth smoke output
- `restore_drill_output.txt` — Backup/restore drill output

---

## Artifact: `2026-05-08-local-path2-target-profile.md`

**Location**: `docs/implementation-path/artifacts/2026-05-08-local-path2-target-profile.md`

This artifact records the observed result of running the local profile script.
It includes:

- Profile structure used
- Phase results
- Explicit non-claims
- Next steps for real target deployment

---

## Alternative Path in doc92

When real target values are unavailable, doc92 Phase B can use this alternative:

> **Alternative Path — No Real Target Values Available**
>
> If the operator does not yet have real target values (host, credentials, paths),
> the local target profile provides a local-only alternative to validate tooling:
>
> - **Script**: `bash scripts/run_local_path2_target_profile.sh --keep-output`
> - **Documentation**: [`doc93`](./93-local-path2-target-profile-plan.md)
> - **Artifact**: [`artifacts/2026-05-08-local-path2-target-profile.md`](./artifacts/2026-05-08-local-path2-target-profile.md)
>
> **This remains LOCAL-ONLY.** It does NOT constitute target evidence, G2 completion,
> or pilot authorization. Real target values from doc71 are still required for Phase B.

---

## Relationship to Other Docs

| Doc | Relationship |
|-----|--------------|
| doc71 | Source of real target values (when available) |
| doc92 | Next-actions plan; this doc provides Phase B alternative |
| doc66 | Phase A/B boundary; Phase B blocked until real target values available |
| doc69 | Local dummy values (existing alternative) |
| doc93 | This doc; local target profile plan |

---

## Todo-List

| Item | Owner | Status |
|------|-------|--------|
| Create script `run_local_path2_target_profile.sh` | Engineering | ✅ Done |
| Create doc93 plan | Engineering | ✅ Done |
| Run script locally | Engineering | ✅ Done |
| Create artifact `2026-05-08-local-path2-target-profile.md` | Engineering | ✅ Done |
| Add doc93 link to doc92 (alternative path) | Engineering | ✅ Done |
| Add doc93 link to doc71 (quick sequence) | Engineering | ✅ Done |
| Add doc93 link to README/doc91 | Engineering | ✅ Done |

---

## Explicit Non-Claims (Repeated)

- This profile does NOT complete any G2 gate
- This profile does NOT authorize the pilot
- This profile does NOT make FerrumGate production-ready
- This profile does NOT provide target evidence
- This profile does NOT constitute operator signoff
- Generated tokens are ephemeral and NOT committed
- Canonical docs 54/58/59/63/65 are NOT modified

---

*Created 2026-05-08. Local-only Path 2 target profile alternative. No G2/pilot/production-ready claim.*
