# MANIFEST — Dummy Rehearsal Bundle Checklist

> **Status**: DUMMY/LOCAL-TEST ONLY — NOT OPERATOR EVIDENCE
> **Purpose**: Provides a checklist of expected output files from the dummy Path 2 rehearsal orchestration script.
> **Constraint**: All files are labeled "LOCAL-TEST/DUMMY". This manifest is a rehearsal tracker only.

---

## ⚠️ IMPORTANT — NOT REAL EVIDENCE

This manifest tracks **dummy/local-test outputs only**. Evidence collected here:
- Is labeled "LOCAL-TEST/DUMMY/NOT OPERATOR EVIDENCE"
- Does NOT satisfy any G2 gate requirement
- Does NOT constitute pilot authorization
- Does NOT claim production-ready
- Is for tooling/runbook validation ONLY

---

## Phase 0 — Preflight Check

| File | Description | Status |
|------|-------------|--------|
| `00-config/syntax_validation.txt` | Script syntax validation output | ☐ Not generated |
| `00-config/dependency_check.txt` | Dependency availability check | ☐ Not generated |

---

## Phase 1 — Dummy Config

| File | Description | Status |
|------|-------------|--------|
| `00-config/dummy-ferrumgate.toml` | Generated dummy config file | ☐ Not generated |
| `00-config/dummy-token.txt` | Generated dummy token (not committed) | ☐ Not generated |
| `00-config/config_validation.txt` | Config validation output | ☐ Not generated |

---

## Phase 2 — Probe Sequence

| File | Description | Expected Result |
|------|-------------|-----------------|
| `01-probes/healthz_output.txt` | `/v1/healthz` probe output | 200 (LOCAL-TEST) |
| `01-probes/readyz_output.txt` | `/v1/readyz` probe output | 200 (LOCAL-TEST) |
| `01-probes/readyz_deep_output.txt` | `/v1/readyz/deep` probe output | 200 (LOCAL-TEST) |
| `01-probes/metrics_output.txt` | `/v1/metrics` probe output | 200 + prometheus format |
| `01-probes/approvals_noauth_output.txt` | `/v1/approvals` without auth | 401 (LOCAL-TEST) |

---

## Phase 3 — Auth Smoke

| File | Description | Expected Result |
|------|-------------|-----------------|
| `02-auth-smoke/approvals_wrong_token.txt` | `/v1/approvals` with wrong token | 401 (LOCAL-TEST) |
| `02-auth-smoke/approvals_correct_token.txt` | `/v1/approvals` with correct token | 200 (LOCAL-TEST) |
| `02-auth-smoke/auth_smoke_summary.txt` | Summary of auth smoke results | PASS (LOCAL-TEST) |

---

## Phase 4 — Backup/Restore Drill

| File | Description | Expected Result |
|------|-------------|-----------------|
| `03-backup-restore/backup_create_output.txt` | `ferrumctl backup create` output | PASS (LOCAL-TEST) |
| `03-backup-restore/backup_verify_pre_output.txt` | `ferrumctl backup verify` pre-restore | PASS (LOCAL-TEST) |
| `03-backup-restore/restore_drill_output.txt` | Full restore drill sequence | PASS (LOCAL-TEST) |
| `03-backup-restore/backup_verify_post_output.txt` | `ferrumctl backup verify` post-restore | PASS (LOCAL-TEST) |

---

## Phase 5 — D1-D6 Compensation Drills

### D1 — Filesystem Adapter

| File | Description | Status |
|------|-------------|--------|
| `04-d1-d6-drills/d1-filesystem/d1_test_output.txt` | FileWrite/FileDelete/FileMove drill output | ☐ Not collected |
| `04-d1-d6-drills/d1-filesystem/d1_recovered_summary.txt` | recovered=true/false summary for D1 | ☐ Not collected |

### D2 — Git Local Ref Operations

| File | Description | Status |
|------|-------------|--------|
| `04-d1-d6-drills/d2-git-local/d2_test_output.txt` | GitCommit/GitBranch/GitTag drill output | ☐ Not collected |
| `04-d1-d6-drills/d2-git-local/d2_recovered_summary.txt` | recovered=true/false summary for D2 | ☐ Not collected |

### D3 — Git Remote Push

| File | Description | Status |
|------|-------------|--------|
| `04-d1-d6-drills/d3-git-remote/d3_push_baseline_output.txt` | Baseline remote push drill output | ☐ Not collected |
| `04-d1-d6-drills/d3-git-remote/d3_push_failclosed_output.txt` | Fail-closed remote push drill output | ☐ Not collected |
| `04-d1-d6-drills/d3-git-remote/d3_recovered_summary.txt` | recovered/fail-closed summary for D3 | ☐ Not collected |

### D4 — HTTP Strict Replay

| File | Description | Status |
|------|-------------|--------|
| `04-d1-d6-drills/d4-http-replay/d4_replay_output.txt` | HTTP POST replay drill output | ☐ Not collected |
| `04-d1-d6-drills/d4-http-replay/d4_failclosed_output.txt` | HTTP fail-closed drill output | ☐ Not collected |
| `04-d1-d6-drills/d4-http-replay/d4_recovered_summary.txt` | recovered/fail-closed summary for D4 | ☐ Not collected |

### D5 — SQLite Adapter

| File | Description | Status |
|------|-------------|--------|
| `04-d1-d6-drills/d5-sqlite/d5_test_output.txt` | SQLite INSERT/UPDATE/DELETE drill output | ☐ Not collected |
| `04-d1-d6-drills/d5-sqlite/d5_recovered_summary.txt` | recovered=true/false summary for D5 | ☐ Not collected |

### D6 — Maildraft Adapter

| File | Description | Status |
|------|-------------|--------|
| `04-d1-d6-drills/d6-maildraft/d6_test_output.txt` | DraftCreate/Update/Delete drill output | ☐ Not collected |
| `04-d1-d6-drills/d6-maildraft/d6_recovered_summary.txt` | recovered=true/false summary for D6 | ☐ Not collected |

---

## Phase 6 — Evidence Skeleton

| File | Description | Status |
|------|-------------|--------|
| `05-evidence-skeleton/d1-d6_evidence_skeleton.md` | Generated D1-D6 evidence skeleton | ☐ Not generated |
| `05-evidence-skeleton/d1-d6_raw_output.txt` | Raw drill output used for skeleton | ☐ Not generated |

---

## Phase 7 — Rehearsal Summary

| File | Description | Status |
|------|-------------|--------|
| `06-summary/rehearsal_summary.md` | Watermarked rehearsal summary | ☐ Not generated |
| `06-summary/rehearsal_summary.json` | JSON summary of all phases | ☐ Not generated |
| `06-summary/rehearsal_watermark.txt` | Evidence watermark confirmation | ☐ Not generated |

---

## Completion Attestation

> **DUMMY REHEARSAL ONLY**: This rehearsal was executed in a local/test environment using artificial dummy values. It does NOT constitute real target evidence collection.

> Dummy rehearsal completed by: _______________________________
> Date: _______________

---

## Security Notes

- **Dummy token**: Auto-generated per session via `openssl rand -hex 16` — never committed
- **Temp directories**: All outputs written to user-specified or `/tmp/ferrum-dummy-rehearsal-*/` — cleaned up automatically
- **No real secrets**: No real bearer tokens, SSH keys, TLS certificates, or infrastructure credentials used
- **No network exposure**: All testing uses `127.0.0.1` loopback only

---

*Dummy rehearsal manifest: 2026-05-08. LOCAL-TEST/DUMMY ONLY — NOT OPERATOR EVIDENCE.*
*Canonical docs 54, 58, 59, 63, 65 kept untouched.*