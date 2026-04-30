# 61 — Path 2 Execution Plan

> **Status**: Documentation-only. Operator-owned execution checklist.
> **Purpose**: Ordered execution plan and checklist for FerrumGate v1 single-node SQLite production pilot.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. No production-ready claim.
> **RC status**: v0.1.0-rc.1 published (Path 1 complete); this plan activates Path 2 pilot preparation.
> **Constraint**: Do not mark G2 complete, do not start PostgreSQL, no production-ready claim, no operator signature.

---

## Purpose

This document provides an ordered execution checklist for completing Path 2 (Conditional Production Pilot).
It links the automated drill-runner evidence helper, restore drill, backup scheduler setup, TLS/reverse proxy
configuration, and Phase 3 decision gates.

**Operator-owned**: All signoff items in this plan require explicit operator action and signature.
Do not mark items complete on behalf of the operator.

---

## Ordered Execution Strategy

Execute steps in the order listed. Do not skip ahead or claim completion prematurely.

| # | Step | Owner | Dependency | PostgreSQL Status |
|---|---|---|---|---|
| 1 | Complete Path 2 pilot checklist (G2.1–G2.8) | Operator | None | Blocked until G2/Path2 complete |
| 2 | Use automated D1–D6 drill runner (`run_d1_d6_drills.py`) | Operator | Step 1 started | Blocked |
| 3 | Run real non-prod restore drill | Operator | Step 1 partial (G2.1) | Blocked |
| 4 | Configure backup scheduler + TLS/reverse proxy externally | Operator | None (external) | Blocked |
| 5 | Decide Phase 3 only after G2/Path2 evidence | Operator + Engineering | All above | Decision gated on Path 2 |

**PostgreSQL is deferred until this plan is complete.** Do not start PostgreSQL now.

---

## Step 1 — Complete Path 2 Pilot Checklist (G2.1–G2.8)

**Owner**: Operator

### 1.1 Pilot Readiness Evidence Packet

Complete [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) G2.1–G2.8:

| G2 Item | Description | Evidence Required | Status |
|---|---|---|---|
| G2.1 | Backup/Restore Drill | `PRAGMA integrity_check` passes on restored DB | ☐ Pending |
| G2.2 | Compensation Drill (D1–D6) | Per [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) | ☐ Pending |
| G2.3 | Readiness Probe | `GET /v1/readyz/deep` returns HTTP 200 | ☐ Pending |
| G2.4 | Metrics Baseline | `/v1/metrics` returns Prometheus-compatible output | ☐ Pending |
| G2.5 | Known Limitation Acceptance | All limitations reviewed and accepted | ☐ Pending |
| G2.6 | Rollback Acceptance | R0/R1/R2/R3 semantics understood | ☐ Pending |
| G2.7 | SQLite Capacity Acceptance | Workload model confirms ≤300 writes/s sustained | ☐ Pending |
| G2.8 | Operator Signoff | Signed acceptance statement | ☐ Pending |

### 1.2 Workload-Fit Review Checklist

Per [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Workload-fit review:

- [ ] Confirm expected sustained write rate ≤300 writes/s
- [ ] Confirm single-node topology (no HA/replica/multi-node required)
- [ ] Confirm bounded execution history is acceptable for target use case
- [ ] Confirm target workflow is in the supported flows list (`25-v1-single-node-rc-evidence.md` Evidence 9)
- [ ] If any of the above do not fit: defer to Path 3 (Phase 3 PostgreSQL)

### 1.3 Stop Conditions for Step 1

| Trigger | Action |
|---|---|
| Write throughput exceeds Phase 1 capacity (>300 writes/s sustained) | Abort Path 2; proceed to Path 3 |
| Any G2 signoff item declined by operator | Abort Path 2; resolve or formally accept risk |
| Compensate noop risk unacceptable for target adapters | Abort Path 2; adapter implementation required before R1/R2/R3 use |

### 1.4 Evidence Files Generated

- `59-pilot-readiness-evidence-packet.md` (completed G2.1–G2.8)
- `58-workload-compensation-drill-evidence-template.md` (completed D1–D6 drills)
- Operator workload model document (sustained write rate analysis)

---

## Step 2 — Automated D1–D6 Drill Runner

**Owner**: Operator

### 2.1 Automated Evidence Helper

Use [`scripts/run_d1_d6_drills.py`](../../scripts/run_d1_d6_drills.py) to run bounded local D1–D6 evidence commands and generate markdown/JSON output. Use [`scripts/generate_evidence_skeleton.py`](../../scripts/generate_evidence_skeleton.py) when you need to convert additional command output into operator-fillable markdown skeletons.

```bash
# Run local D1-D6 evidence commands and generate markdown/JSON evidence
python3 scripts/run_d1_d6_drills.py

# Include optional readiness smoke against a live non-prod server
python3 scripts/run_d1_d6_drills.py --server-url http://127.0.0.1:8080

# Convert additional captured output to operator-fillable skeletons
python3 scripts/generate_evidence_skeleton.py --type d1-d6 --file drill_output.txt
```

### 2.2 D1–D6 Drill Execution Sequence

Execute drills in adapter order. Capture output for evidence skeleton generation.

| Drill | Adapter | Intent Type | Key Verification |
|---|---|---|---|
| D1.1 | FS | FileWrite | File created, compensate restores/deletes |
| D1.2 | FS | FileDelete | File deleted, compensate restores |
| D2.1 | Git | GitCommit | Commit exists, compensate removes |
| D2.2 | Git | GitPush | Fail-closed behavior verified |
| D3.1 | Git remote | GitPush | Remote rollback/fail-closed behavior verified |
| D4.1 | HTTP | HttpMutation (POST) | Idempotency replay verified |
| D4.2 | HTTP | HttpMutation (non-idempotent) | Fail-closed verified |
| D5 | SQLite | SqliteMutation | DML rollback via SAVEPOINT |
| D6 | Maildraft | MailDraftCreate | In-memory draft removed |

### 2.3 Drill Command Template

```bash
# Start ferrumd in non-prod environment
ferrumd --config /path/to/nonprod-ferrumgate.toml

# Verify readiness
curl http://127.0.0.1:8080/v1/readyz/deep  # Expected: 200

# Execute drill (example: D1.1 FileWrite)
curl -X POST http://127.0.0.1:8080/v1/intents \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "intent_type": "FileWrite",
    "resource": "/tmp/ferrum_drill_D1.txt",
    "content": "D1 FileWrite drill content",
    "rollback_class": "R1"
  }'

# [Submit proposal, approve, execute flow...]

# Capture drill output
cat drill_output.txt | python3 scripts/generate_evidence_skeleton.py --type d1-d6
```

### 2.4 Stop Conditions for Step 2

| Trigger | Action |
|---|---|
| Any drill `recovered: false` with unacceptable risk | Operator evaluates; may abort or accept with compensating control |
| `fail_closed_verified: false` on GitPush or HTTP | Abort pilot; adapter implementation required |
| Compensate noop confirmed for target adapter | Operator accepts noop risk or aborts |

### 2.5 Evidence Files Generated

- Completed [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) with operator annotations
- Drill command output logs
- Runner output from `run_d1_d6_drills.py` (`drill_summary.md`, `drill_summary.json`, raw command logs)
- Optional evidence skeleton output from `generate_evidence_skeleton.py`

---

## Step 3 — Real Non-Prod Restore Drill

**Owner**: Operator

### 3.1 Restore Drill Procedure

Per [`18-single-node-operations-runbook.md`](../ferrumgate-roadmap-v1/18-single-node-operations-runbook.md) §6 and [`60-bounded-hardening-examples.md`](./60-bounded-hardening-examples.md) §3:

```bash
STORE_PATH="/var/lib/ferrumgate/ferrumgate.db"
BACKUP_DIR="/var/backups/ferrumgate"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.db"

# 1. Create fresh backup
ferrumctl backup create --db-path "$STORE_PATH" --output-dir "$BACKUP_DIR"

# 2. Verify backup integrity
ferrumctl backup verify --db-path "$BACKUP_FILE"
# Expected: OK

# 3. Stop ferrumd
FERRUM_PID=$(pgrep -f ferrumd)
if [ -n "$FERRUM_PID" ]; then
  echo "Stopping ferrumd (PID: $FERRUM_PID)"
  kill "$FERRUM_PID"
  sleep 2
fi

# 4. Perform restore
ferrumctl backup restore \
    --db-path "$STORE_PATH" \
    --from "$BACKUP_FILE" \
    --confirm

# 5. Post-restore verify
ferrumctl backup verify --db-path "$STORE_PATH"
# Expected: OK

# 6. Restart and verify
ferrumd --config /path/to/nonprod-ferrumgate.toml
curl http://127.0.0.1:8080/v1/readyz/deep
# Expected: HTTP 200
```

### 3.2 Restore Drill Evidence Fields

| Field | Value |
|---|---|
| `backup_file_used` | /path/to/backup/file.db |
| `backup_verify_pre_restore` | OK / FAILED |
| `restore_completed` | true / false |
| `pre_restore_copy_created` | true / false |
| `backup_verify_post_restore` | OK / FAILED |
| `ferrumd_restarted` | true / false |
| `readyz_deep_returns_200` | true / false |
| `operator_annotation` | <any anomalies or deviations> |

### 3.3 Acceptance Criteria

- `ferrumctl backup verify` passes on the backup file before restore
- `ferrumctl backup restore` completes with `--confirm` flag
- `.pre_restore` copy created automatically
- `ferrumctl backup verify` passes on restored store
- `GET /v1/readyz/deep` returns HTTP 200 after restart
- `PRAGMA integrity_check` passes (verified by `backup verify`)

### 3.4 Stop Conditions for Step 3

| Trigger | Action |
|---|---|
| `ferrumctl backup verify` fails pre-restore | Do not restore; take new backup; investigate |
| `ferrumctl backup restore` refuses (DB locked) | Stop ferrumd; retry restore |
| `ferrumctl backup verify` fails post-restore | Abort restore; restore `.pre_restore` copy; investigate |
| `readyz/deep` returns non-200 after restart | Abort; investigate; restore `.pre_restore` if needed |

### 3.5 Evidence Files Generated

- Restore drill command log
- Completed [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) G2.1 section
- Completed [`60-bounded-hardening-examples.md`](./60-bounded-hardening-examples.md) §3 restore evidence fields

---

## Step 4 — Backup Scheduler + TLS/Reverse Proxy (External)

**Owner**: Operator (external configuration, not FerrumGate-built)

### 4.1 Backup Scheduler (External)

Example files are provided under `configs/examples/`:

- `configs/examples/ferrumgate-backup.cron`
- `configs/examples/ferrumgate-backup.service`
- `configs/examples/ferrumgate-backup.timer`

FerrumGate v1 does not include built-in backup scheduling. Implement externally per [`18-single-node-operations-runbook.md`](../ferrumgate-roadmap-v1/18-single-node-operations-runbook.md) §5.4.

#### cron example

```bash
# /etc/cron.d/ferrumgate-backup
# Run backup at 02:00 daily, keep 7 daily snapshots
SHELL=/bin/bash
PATH=/usr/local/bin:/usr/bin:/bin
0 2 * * * root /usr/local/bin/ferrumctl backup create \
    --db-path "/var/lib/ferrumgate/ferrumgate.db" \
    --output-dir "/var/backups/ferrumgate" \
    && find /var/backups/ferrumgate -name "ferrumgate_*.db" -mtime +7 -delete
```

#### systemd timer example

```bash
# /etc/systemd/system/ferrumgate-backup.service
[Unit]
Description=FerrumGate SQLite Backup
Requires=ferrumd.service
[Service]
Type=oneshot
ExecStart=/usr/local/bin/ferrumctl backup create \
    --db-path "/var/lib/ferrumgate/ferrumgate.db" \
    --output-dir "/var/backups/ferrumgate"
PrivateTmp=true

# /etc/systemd/system/ferrumgate-backup.timer
[Unit]
Description=FerrumGate SQLite Backup (daily)
[Timer]
OnCalendar=daily
Persistent=true
[Install]
WantedBy=timers.target

systemctl enable ferrumgate-backup.timer
systemctl start ferrumgate-backup.timer
```

### 4.2 TLS/Reverse Proxy Configuration (External)

Example file: `configs/examples/nginx-ferrumgate.conf`.

FerrumGate v1 does not include TLS termination. Deploy behind a TLS-terminating reverse proxy.

#### nginx example

```nginx
server {
    listen 443 ssl;
    server_name ferrumgate.example.com;

    ssl_certificate /etc/ssl/certs/ferrumgate.crt;
    ssl_certificate_key /etc/ssl/private/ferrumgate.key;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Authorization "Bearer $http_authorization";
    }
}

server {
    listen 80;
    server_name ferrumgate.example.com;
    return 301 https://$server_name$request_uri;
}
```

### 4.3 Acceptance Criteria

- Backup scheduler runs `ferrumctl backup create` on configured schedule
- Backup retention policy enforced (e.g., 7 daily snapshots)
- `ferrumctl backup verify` passes after each backup
- TLS termination confirmed at reverse proxy (not at ferrumd)
- Bearer auth token configured at reverse proxy or FerrumGate config
- Health endpoints (`/v1/healthz`, `/v1/readyz`) reachable through proxy
- `/v1/readyz/deep` returns HTTP 200 through proxy

### 4.4 Stop Conditions for Step 4

| Trigger | Action |
|---|---|
| Backup scheduler not operational before pilot start | Do not begin pilot; implement scheduling first |
| TLS not configured | Do not expose non-loopback without TLS; abort pilot |
| `ferrumctl backup verify` fails in production schedule | Investigate; do not proceed until resolved |

### 4.5 Evidence Files Generated

- Backup scheduler configuration file (cron, systemd timer, or equivalent)
- TLS/reverse proxy configuration excerpt (redacted credentials)
- Backup schedule evidence log showing successful runs

---

## Step 5 — Phase 3 Decision Gate

**Owner**: Operator + Engineering Lead

**Trigger**: Phase 3 decision is made only after G2/Path2 evidence is complete.

### 5.1 Decision Prerequisites

All of the following must be satisfied before Phase 3 decision:

| Prerequisite | Evidence | Owner |
|---|---|---|
| Path 2 pilot checklist (G2.1–G2.8) complete | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) signed | Operator |
| D1–D6 compensation drills executed | [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) signed | Operator |
| Non-prod restore drill successful | Restore drill log with `PRAGMA integrity_check` passing | Operator |
| Backup scheduler operational | Configuration + evidence of successful runs | Operator |
| TLS/reverse proxy operational | Configuration + probe verification | Operator |

### 5.2 Decision Criteria

| Decision | Criteria | Next Action |
|---|---|---|
| **Proceed to Phase 3** | Pilot confirms single-node SQLite inadequate for workload (e.g., >300 writes/s, multi-node required) OR operator prefers PostgreSQL for production scale | Engineering lead initiates Phase P1 per [`50-p4-postgres-store-facade-adr.md`](./50-p4-postgres-store-facade-adr.md) |
| **Continue Path 2 (bounded single-node)** | Pilot confirms single-node SQLite acceptable for target workload | Operator continues bounded production use; Phase 3 deferred |
| **Abort pilot** | Any abort trigger from [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 2 fires | Investigate, fix, and re-evaluate or formally close pilot |

### 5.3 Phase 3 Go/No-Go Gates (G3.1–G3.4)

Per [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 3 Gate:

| Gate | Criterion | Evidence | Owner | Status |
|---|---|---|---|---|
| G3.1 | v1 RC tag cut and Path 1 complete | RC tag `v0.1.0-rc.1` at commit `5fce844d` | Release engineer | ☑ DONE |
| G3.2 | Production pilot (Path 2) confirmed single-node SQLite acceptable | Operator signoff per [`27-production-evaluation-plan.md`](./27-production-evaluation-plan.md) | Operator | ☐ Pending |
| G3.3 | Engineering capacity confirmed for ~2000–3000 LOC + migrations + tests | ADR-50 effort estimate | Engineering lead | ☐ Pending |
| G3.4 | ADR-50 Phase P1 reviewed and approved | [`50-p4-postgres-store-facade-adr.md`](./50-p4-postgres-store-facade-adr.md) §3 | Engineering lead | ☐ Pending |

**Do not begin Phase P1 until G3.1–G3.4 are satisfied.**

### 5.4 Stop Conditions for Step 5

| Trigger | Action |
|---|---|
| G3.2 (Path 2 complete) not satisfied | Do not proceed to Phase 3; resolve Path 2 gaps first |
| G3.3 (Engineering capacity) not confirmed | Do not begin Phase P1; evaluate capacity or defer |
| G3.4 (ADR-50 Phase P1) not approved | Do not begin Phase P1; resolve ADR-50 open items first |

### 5.5 Evidence Files Generated

- Phase 3 decision log entry (date, decision, owner, rationale)
- [`55-phase-3-go-no-go-review.md`](./55-phase-3-go-no-go-review.md) updated with decision

---

## Master Execution Checklist

Complete steps in order. Do not mark complete on behalf of the operator.

### Pre-Pilot Preparation

| # | Action | Owner | Done | Evidence |
|---|---|---|---|---|
| 1 | Complete G2.1–G2.8 pilot readiness evidence | Operator | ☐ | `59-pilot-readiness-evidence-packet.md` signed |
| 2 | Execute D1–D6 compensation drills | Operator | ☐ | `58-workload-compensation-drill-evidence-template.md` signed |
| 3 | Run non-prod restore drill | Operator | ☐ | Restore drill log with `PRAGMA integrity_check` passing |
| 4 | Configure backup scheduler externally | Operator | ☐ | Scheduler configuration + evidence of successful runs |
| 5 | Configure TLS/reverse proxy externally | Operator | ☐ | Proxy configuration + probe verification |
| 6 | Complete operator signoff | Operator | ☐ | `54-operator-signoff-packet.md` signed |

### Phase 3 Decision

| # | Action | Owner | Done | Evidence |
|---|---|---|---|---|
| 7 | Assess Path 2 pilot outcome | Operator + Engineering | ☐ | Decision log entry |
| 8 | Confirm or deny Phase 3 need | Operator + Engineering | ☐ | Decision log entry |
| 9 | Complete G3.2–G3.4 if proceeding to Phase 3 | Operator + Engineering | ☐ | `55-phase-3-go-no-go-review.md` updated |

---

## Cross-Reference Index

| From | To | Purpose |
|---|---|---|
| This doc | [`31-release-paths-todo.md`](./31-release-paths-todo.md) | Path 2 G2 gates and checklists |
| This doc | [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) | Operator signoff form |
| This doc | [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) | D1–D6 drill template |
| This doc | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence packet |
| This doc | [`60-bounded-hardening-examples.md`](./60-bounded-hardening-examples.md) | Bounded hardening examples |
| This doc | [`18-single-node-operations-runbook.md`](../ferrumgate-roadmap-v1/18-single-node-operations-runbook.md) | Backup/restore procedures |
| This doc | [`50-p4-postgres-store-facade-adr.md`](./50-p4-postgres-store-facade-adr.md) | Phase 3 PostgreSQL plan |
| This doc | [`55-phase-3-go-no-go-review.md`](./55-phase-3-go-no-go-review.md) | Phase 3 go/no-go gates |
| This doc | [`scripts/run_d1_d6_drills.py`](../../scripts/run_d1_d6_drills.py) | Automated local D1–D6 evidence runner |
| This doc | [`scripts/generate_evidence_skeleton.py`](../../scripts/generate_evidence_skeleton.py) | Automated evidence skeleton helper |

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- No production-ready claim is made in this document
- PostgreSQL/multi-node/HA are not implemented and not in scope
- Phase 2 transaction batching was deferred/regressed
- G2 remains pending/operator-owned until signed in `54-operator-signoff-packet.md`
- Phase 3 decision is blocked until G2/Path2 evidence is complete
- PostgreSQL is blocked until Phase 3 decision gates (G3.1–G3.4) are satisfied

---

*Document created: 2026-04-29. Operator-owned execution plan — no G2 complete claim, no PostgreSQL start, no production-ready claim, no operator signature.*
