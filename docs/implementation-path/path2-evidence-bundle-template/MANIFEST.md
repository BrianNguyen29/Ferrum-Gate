# MANIFEST — Evidence File Checklist

> **Status**: TEMPLATE ONLY — NOT EVIDENCE. Checklist for operator to track evidence collection.
> **Purpose**: Provides a checklist of expected evidence files to collect during target run.
> **Constraint**: This manifest is a placeholder tracker — actual evidence must be collected from the live target environment.

---

## Probe Evidence (Phase 2)

| File | Description | Status |
|------|-------------|--------|
| `00-probes/healthz_output.txt` | `/v1/healthz` probe output | ☐ Not collected |
| `00-probes/readyz_output.txt` | `/v1/readyz` probe output | ☐ Not collected |
| `00-probes/readyz_deep_output.txt` | `/v1/readyz/deep` probe output | ☐ Not collected |
| `00-probes/metrics_output.txt` | `/v1/metrics` probe output | ☐ Not collected |
| `00-probes/approvals_noauth_output.txt` | `/v1/approvals` without auth (expect 401) | ☐ Not collected |

---

## Auth Smoke Evidence

| File | Description | Status |
|------|-------------|--------|
| `01-auth-smoke/approvals_wrong_token.txt` | `/v1/approvals` with wrong token (expect 401) | ☐ Not collected |
| `01-auth-smoke/approvals_correct_token.txt` | `/v1/approvals` with correct token (expect 200) | ☐ Not collected |
| `01-auth-smoke/auth_smoke_summary.txt` | Summary of auth smoke results | ☐ Not collected |

---

## Backup/Restore Drill Evidence (Phase 5)

| File | Description | Status |
|------|-------------|--------|
| `02-backup-restore/backup_create_output.txt` | `ferrumctl backup create` output | ☐ Not collected |
| `02-backup-restore/backup_verify_pre_output.txt` | `ferrumctl backup verify` pre-restore output | ☐ Not collected |
| `02-backup-restore/restore_drill_output.txt` | Full restore drill sequence output | ☐ Not collected |
| `02-backup-restore/backup_verify_post_output.txt` | `ferrumctl backup verify` post-restore output | ☐ Not collected |
| `02-backup-restore/readyz_deep_post_restore.txt` | `/v1/readyz/deep` after restore and restart | ☐ Not collected |

---

## D1–D6 Compensation Drill Evidence (Phase 3)

### D1 — Filesystem Adapter

| File | Description | Status |
|------|-------------|--------|
| `03-d1-d6-drills/d1-filesystem/d1_filewrite_output.txt` | FileWrite drill output | ☐ Not collected |
| `03-d1-d6-drills/d1-filesystem/d1_filedelete_output.txt` | FileDelete drill output | ☐ Not collected |
| `03-d1-d6-drills/d1-filesystem/d1_filemove_output.txt` | FileMove drill output | ☐ Not collected |
| `03-d1-d6-drills/d1-filesystem/d1_recovered_summary.txt` | recovered=true/false summary for D1 | ☐ Not collected |

### D2 — Git Local Ref Operations

| File | Description | Status |
|------|-------------|--------|
| `03-d1-d6-drills/d2-git-local/d2_gitcommit_output.txt` | GitCommit drill output | ☐ Not collected |
| `03-d1-d6-drills/d2-git-local/d2_gitbranch_output.txt` | GitBranchCreate drill output | ☐ Not collected |
| `03-d1-d6-drills/d2-git-local/d2_gittag_output.txt` | GitTagCreate drill output | ☐ Not collected |
| `03-d1-d6-drills/d2-git-local/d2_recovered_summary.txt` | recovered=true/false summary for D2 | ☐ Not collected |

### D3 — Git Remote Push

| File | Description | Status |
|------|-------------|--------|
| `03-d1-d6-drills/d3-git-remote/d3_push_baseline_output.txt` | Baseline remote push drill output | ☐ Not collected |
| `03-d1-d6-drills/d3-git-remote/d3_push_failclosed_output.txt` | Fail-closed remote push drill output | ☐ Not collected |
| `03-d1-d6-drills/d3-git-remote/d3_recovered_summary.txt` | recovered=true/false summary for D3 | ☐ Not collected |

### D4 — HTTP Strict Replay

| File | Description | Status |
|------|-------------|--------|
| `03-d1-d6-drills/d4-http-replay/d4_replay_output.txt` | HTTP POST replay drill output | ☐ Not collected |
| `03-d1-d6-drills/d4-http-replay/d4_failclosed_output.txt` | HTTP fail-closed drill output | ☐ Not collected |
| `03-d1-d6-drills/d4-http-replay/d4_recovered_summary.txt` | recovered=true/false summary for D4 | ☐ Not collected |

### D5 — SQLite Adapter

| File | Description | Status |
|------|-------------|--------|
| `03-d1-d6-drills/d5-sqlite/d5_insert_output.txt` | SQLite INSERT drill output | ☐ Not collected |
| `03-d1-d6-drills/d5-sqlite/d5_update_output.txt` | SQLite UPDATE drill output | ☐ Not collected |
| `03-d1-d6-drills/d5-sqlite/d5_delete_output.txt` | SQLite DELETE drill output | ☐ Not collected |
| `03-d1-d6-drills/d5-sqlite/d5_recovered_summary.txt` | recovered=true/false summary for D5 | ☐ Not collected |

### D6 — Maildraft Adapter

| File | Description | Status |
|------|-------------|--------|
| `03-d1-d6-drills/d6-maildraft/d6_draft_create_output.txt` | DraftCreate drill output | ☐ Not collected |
| `03-d1-d6-drills/d6-maildraft/d6_draft_update_output.txt` | DraftUpdate drill output | ☐ Not collected |
| `03-d1-d6-drills/d6-maildraft/d6_draft_delete_output.txt` | DraftDelete drill output | ☐ Not collected |
| `03-d1-d6-drills/d6-maildraft/d6_recovered_summary.txt` | recovered=true/false summary for D6 | ☐ Not collected |

---

## Metrics Evidence

| File | Description | Status |
|------|-------------|--------|
| `04-metrics/metrics_capture.txt` | Prometheus metrics endpoint output | ☐ Not collected |
| `04-metrics/metrics_summary.txt` | Metrics summary / interpretation | ☐ Not collected |

---

## Log Files

| File | Description | Status |
|------|-------------|--------|
| `05-logs/ferrumd_startup_log.txt` | Server startup log | ☐ Not collected |
| `05-logs/ferrumd_drill_log.txt` | Server log during drill execution | ☐ Not collected |
| `05-logs/restore_drill_log.txt` | Restore drill specific log | ☐ Not collected |

---

## G2 Evidence (References)

| File | Description | Status |
|------|-------------|--------|
| `06-g2-evidence/g2_evidence_link.txt` | Link/reference to completed doc 59 | ☐ Not linked |
| `06-g2-evidence/g2_gate_status.md` | G2.1–G2.8 gate status summary | ☐ Not completed |

---

## Signoff (References)

| File | Description | Status |
|------|-------------|--------|
| `07-signoff/signoff_link.txt` | Link/reference to signed doc 54 | ☐ Not linked |
| `07-signoff/operator_signature.txt` | Operator signature confirmation | ☐ Not signed |

---

## Completion Attestation

> **Evidence Collection Attestation**: I confirm all evidence files above were collected from the live target environment during the target run dated: _______________________________

Operator signature: _________________ Date: _________

---

*Template manifest: 2026-05-06. Evidence file checklist — NOT EVIDENCE itself.*
