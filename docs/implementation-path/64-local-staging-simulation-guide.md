# 64 — Local Staging Simulation Guide

> **Status**: Documentation-only. Option 3 — local-only staging simulation.
> **Purpose**: Guide for running local-only staging simulation drills that preserve Path 2/G2 boundaries while practicing operational procedures.
> **Scope**: Local host only. No target environment required. No PostgreSQL/multi-node. Not G2/production evidence.
> **Constraint**: Do not sign doc 54, do not claim G2 complete, do not start PostgreSQL, do not add secrets, do not run remote commands.

---

## Purpose

This guide describes **Option 3**: a local-only staging simulation that allows operators to
practice Path 2 operational procedures (ferrumd startup, readiness probes, D1–D6 drills,
restore drill, backup scheduling) on a local development environment without deploying to
a target non-prod/production host.

**This is explicitly NOT G2/production evidence.** It is a practice/runbook validation
workflow that:
- Confirms operator familiarity with ferrumd, ferrumctl, and drill command sequences
- Validates that local tooling works before target environment deployment
- Provides a low-stakes environment for operator training
- Does NOT complete any G2 gate
- Does NOT authorize any production pilot

---

## Explicit Non-G2 Boundary

| Boundary | What This Guide Provides | What This Guide Does NOT Provide |
|----------|-------------------------|--------------------------------|
| G2.1 Workload Model | Local SQLite performance baseline | Target workload fit analysis |
| G2.2 Auth/TLS | Local `auth_mode=disabled` smoke | Target bearer/TLS configuration |
| G2.3 Backup Schedule | Local backup/restore dry-run | Target backup scheduler implementation |
| G2.4 Restore Drill | Local temp-file restore drill | Target environment restore evidence |
| G2.5 RPO/RTO | Local timing baseline | Target SLA acceptance |
| G2.6 Production Evaluation | Local smoke | Target environment evaluation |
| G2.7 Accepted-Risk Review | Local observation | Target risk acceptance |
| G2.8 Compensate Noop | Local adapter-level tests | Target adapter signoff |

**Bridging to G2**: Use [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) to capture
target environment specifics after completing local simulation practice. Local simulation confirms
operator readiness; target environment evidence completes G2.

---

## Option 3 vs Option 2

| Dimension | Option 2 (Target Environment) | Option 3 (Local Simulation) |
|-----------|-------------------------------|-----------------------------|
| Target host required | Yes | No |
| SSH access required | Yes | No |
| Real TLS/certificates | Yes | No |
| Real backup directory | Yes | No |
| G2 evidence | Yes | No |
| Purpose | Production pilot preparation | Operator practice / runbook validation |

**Recommendation**: Complete Option 3 first to validate operator familiarity, then proceed to
Option 2 for actual G2 evidence capture.

---

## Local Environment Assumptions

| Component | Local Value | Notes |
|-----------|------------|-------|
| ferrumd binary | In PATH | Local build or `cargo run --bin ferrumd` |
| ferrumctl binary | In PATH | Local build or `cargo run --bin ferrumctl` |
| Config | `configs/ferrumgate.dev.toml` | `auth_mode=disabled`, in-memory SQLite |
| Store | `sqlite::memory:` | In-memory, non-persistent |
| Bind address | `127.0.0.1:8080` | Loopback only |
| Evidence output | `/tmp/ferrum-local-drills/` | Temporary, non-persistent |
| Backup output | `/tmp/ferrum-backup-drill/` | Temporary directory |

---

## Phase 1 — Local ferrumd Startup

### 1.1 Build (if needed)

```bash
# Build ferrumd locally
cd <repo-root>
cargo build --bin ferrumd --release 2>&1 | tail -5

# Verify binary
ls -la target/release/ferrumd
```

### 1.2 Start Local ferrumd

```bash
# Use dev config (auth disabled, in-memory SQLite)
./target/release/ferrumd --config configs/ferrumgate.dev.toml &

# Wait for startup
sleep 3

# Verify process running
pgrep -f "ferrumd" || echo "ferrumd not running"
```

### 1.3 Preflight Probe Sequence

```bash
FERRUM_BASE="http://127.0.0.1:8080"

# Shallow health
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/healthz"
# Expected: 200

# Shallow ready
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/readyz"
# Expected: 200

# Deep readiness (functional)
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/readyz/deep"
# Expected: 200

# Metrics (unauthenticated)
curl -s "${FERRUM_BASE}/v1/metrics" | head -5
# Expected: Prometheus text format
```

### 1.4 Local Probe Acceptance Criteria

| Probe | Expected | Actual | Pass/Fail |
|-------|----------|--------|-----------|
| `/v1/healthz` | 200 | _____ | |
| `/v1/readyz` | 200 | _____ | |
| `/v1/readyz/deep` | 200 | _____ | |
| `/v1/metrics` | 200 + prometheus | _____ | |

---

## Phase 2 — Local Readiness Script

Use the automated readiness check script for local smoke:

```bash
# Run local readiness check
python3 scripts/check_pilot_readiness.py --server-url http://127.0.0.1:8080

# Expected output:
# shallow_readiness: PASS
# deep_readiness: PASS
# functional_readiness: PASS
# metrics: PASS
```

**Output is labeled "local/test-drill" — does NOT complete any G2 gate.**

---

## Phase 3 — Local D1–D6 Drill Runner

### 3.1 Automated Drill Runner

```bash
# Create evidence output directory
mkdir -p /tmp/ferrum-local-drills

# Run D1-D6 drills locally (no server URL = adapter-level only)
python3 scripts/run_d1_d6_drills.py --output-dir /tmp/ferrum-local-drills

# Run with server smoke (adapter + gateway-level)
python3 scripts/run_d1_d6_drills.py \
    --server-url http://127.0.0.1:8080 \
    --output-dir /tmp/ferrum-local-drills
```

### 3.2 Manual Drill Sequence (Optional)

For operator practice, run a subset manually:

```bash
FERRUM_BASE="http://127.0.0.1:8080"

# D1.1 FileWrite Drill (local temp file)
echo "=== D1.1 FileWrite ===" > /tmp/d1_manual_drill.txt

INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Content-Type: application/json" \
  -d '{
    "intent_type": "FileWrite",
    "resource": "/tmp/ferrum_drill_test.txt",
    "content": "D1.1 manual drill content",
    "rollback_class": "R1"
  }')
echo "Intent response: ${INTENT_RESPONSE}" >> /tmp/d1_manual_drill.txt

# Extract intent_id (simplified — use jq in production)
INTENT_ID=$(echo "$INTENT_RESPONSE" | grep -o '"intent_id":"[^"]*"' | cut -d'"' -f4)
echo "Intent ID: ${INTENT_ID}" >> /tmp/d1_manual_drill.txt

# [Continue: submit proposal → approve → execute → compensate]
# See doc 62 for full command sequence
```

### 3.3 Local Drill Evidence

| Drill | Local Evidence Path | G2 Bridging |
|-------|---------------------|-------------|
| D1.1 FileWrite | `/tmp/ferrum-local-drills/d1_file_write.md` | Target: real file path on target |
| D1.2 FileDelete | `/tmp/ferrum-local-drills/d1_file_delete.md` | Target: real file path on target |
| D2.1 GitCommit | `/tmp/ferrum-local-drills/d2_git_commit.md` | Target: real git repo on target |
| D3.1 Git Remote Push | `/tmp/ferrum-local-drills/d3_git_push.md` | Target: non-prod remote |
| D4.1 HTTP POST | `/tmp/ferrum-local-drills/d4_http_post.md` | Target: non-prod HTTP endpoint |
| D5 SQLite | `/tmp/ferrum-local-drills/d5_sqlite.md` | Target: real store path |
| D6 Maildraft | `/tmp/ferrum-local-drills/d6_maildraft.md` | Target: real SMTP (if any) |

---

## Phase 4 — Local Restore Drill

### 4.1 Create Temporary Store and Backup

```bash
# Setup temporary directories
DRILL_DIR="/tmp/ferrum-backup-drill"
STORE_DIR="${DRILL_DIR}/store"
BACKUP_DIR="${DRILL_DIR}/backups"
mkdir -p "$STORE_DIR" "$BACKUP_DIR"

# Create a test store with known content
TEST_STORE="${STORE_DIR}/ferrum_test.db"
sqlite3 "$TEST_STORE" "SELECT 1; CREATE TABLE IF NOT EXISTS drill_test (id INTEGER PRIMARY KEY, data TEXT); INSERT INTO drill_test (data) VALUES ('drill-source-data');"

echo "Test store created at: ${TEST_STORE}"
echo "Test data: $(sqlite3 "$TEST_STORE" "SELECT data FROM drill_test WHERE id=1;")"
```

### 4.2 Backup and Verify

```bash
# Create backup using ferrumctl
./target/release/ferrumctl backup create \
    --db-path "$TEST_STORE" \
    --output-dir "$BACKUP_DIR" 2>&1

# List backups
ls -la "$BACKUP_DIR"

# Verify backup
BACKUP_FILE=$(ls -t "${BACKUP_DIR}"/ferrumgate_*.db 2>/dev/null | head -1)
echo "Latest backup: ${BACKUP_FILE}"

./target/release/ferrumctl backup verify --db-path "$BACKUP_FILE"
# Expected: OK
```

### 4.3 Restore Drill

```bash
# Stop any running ferrumd
pkill -f "ferrumd" 2>/dev/null || true
sleep 1

# Perform restore
RESTORE_TARGET="${STORE_DIR}/ferrum_restore.db"
./target/release/ferrumctl backup restore \
    --db-path "$RESTORE_TARGET" \
    --from "$BACKUP_FILE" \
    --confirm 2>&1

# Verify restored store
./target/release/ferrumctl backup verify --db-path "$RESTORE_TARGET"
# Expected: OK

# Verify data
echo "Restored data: $(sqlite3 "$RESTORE_TARGET" "SELECT data FROM drill_test WHERE id=1;")"
# Expected: drill-source-data
```

### 4.4 Local Restore Drill Acceptance Criteria

| Step | Expected | Pass/Fail |
|------|----------|-----------|
| Backup create succeeds | Exit 0 | |
| Backup verify passes | OK | |
| Restore with `--confirm` succeeds | Exit 0 | |
| Pre-restore copy created | `.pre_restore` file | |
| Restored store verify passes | OK | |
| Data matches original | `drill-source-data` | |

---

## Phase 5 — Local Backup Scheduler Dry-Run

### 5.1 cron Example (Local Dry-Run)

```bash
# Create a local cron entry for dry-run testing
mkdir -p /tmp/ferrum-cron-test

# Add to crontab (dry-run only)
echo "# FerrumGate backup dry-run (local test)" >> /tmp/crontab.dryrun
echo "*/5 * * * * root <repo-root>/target/release/ferrumctl backup create --db-path /tmp/ferrum-backup-drill/store/ferrum_test.db --output-dir /tmp/ferrum-backup-drill/backups >> /tmp/ferrum-cron-test/backup.log 2>&1" >> /tmp/crontab.dryrun

echo "Dry-run cron entry:"
cat /tmp/crontab.dryrun
```

### 5.2 systemd timer Example (Local Dry-Run)

```bash
# Create dry-run systemd service and timer
mkdir -p /tmp/ferrum-systemd-test

cat > /tmp/ferrum-systemd-test/ferrumgate-backup.service << 'EOF'
[Unit]
Description=FerrumGate Local Backup Dry-Run
[Service]
Type=oneshot
ExecStart=<repo-root>/target/release/ferrumctl backup create \
    --db-path /tmp/ferrum-backup-drill/store/ferrum_test.db \
    --output-dir /tmp/ferrum-backup-drill/backups
WorkingDirectory=/tmp
EOF

cat > /tmp/ferrum-systemd-test/ferrumgate-backup.timer << 'EOF'
[Unit]
Description=FerrumGate Local Backup Dry-Run Timer
[Timer]
OnCalendar=minutely
[Install]
WantedBy=timers.target
EOF

echo "Dry-run systemd files created in /tmp/ferrum-systemd-test/"
ls -la /tmp/ferrum-systemd-test/
```

---

## Phase 6 — Bridging to Target Environment

### 6.1 Bridging Table: Local Simulation → Target Evidence

| Local Practice | Target Action | Reference |
|----------------|---------------|-----------|
| Local ferrumd startup | Start ferrumd on target host | Doc 63 §2, Doc 62 Phase 2 |
| Local readiness probes | Run probes against target URL | Doc 63 §1, Doc 62 Phase 2 |
| Local D1–D6 drills | Execute D1–D6 on target with real adapters | Doc 63 §10, Doc 62 Phase 3 |
| Local restore drill | Real restore drill on target store | Doc 63 §8, Doc 62 Phase 5 |
| Local backup scheduler | Implement target backup scheduler | Doc 63 §4, Doc 62 Phase 6 |
| Local TLS proxy | Configure target TLS/reverse proxy | Doc 63 §5-6, Doc 62 Phase 7 |

### 6.2 Evidence Transfer Checklist

After completing local simulation practice, transfer learning to target:

| Step | Action | Output |
|------|--------|--------|
| 1 | Document local drill observations | Local drill logs in `/tmp/ferrum-local-drills/` |
| 2 | Fill doc 63 target environment spec | Completed `63-path-2-target-environment-spec.md` |
| 3 | Execute Option 2 target environment drills | Real evidence in target environment |
| 4 | Complete doc 59 G2 evidence packet | Signed `59-pilot-readiness-evidence-packet.md` |
| 5 | Complete doc 58 D1–D6 drill evidence | Signed `58-workload-compensation-drill-evidence-template.md` |

### 6.3 Local Simulation Non-Claims

- Local simulation does **NOT** complete any G2 gate
- Local drill output is labeled "local/test-drill" and cannot be used as G2 evidence
- Target environment evidence must be collected per docs 58/59 after local practice
- Do not submit local drill output as G2 evidence

---

## Phase 7 — Cleanup

```bash
# Stop local ferrumd
pkill -f "ferrumd" 2>/dev/null || true

# Remove temporary drill directories
rm -rf /tmp/ferrum-local-drills /tmp/ferrum-backup-drill /tmp/ferrum-cron-test /tmp/ferrum-systemd-test

# Remove dry-run crontab entry
# sed -i '/FerrumGate backup dry-run/d' /tmp/crontab.dryrun 2>/dev/null || true

echo "Local simulation cleanup complete"
```

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `64-local-staging-simulation-guide.md` | [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Execution plan context |
| `64-local-staging-simulation-guide.md` | [`62-path-2-operator-runbook.md`](./62-path-2-operator-runbook.md) | Target environment commands |
| `64-local-staging-simulation-guide.md` | [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) | Target spec template |
| `64-local-staging-simulation-guide.md` | [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) | D1–D6 drill template |
| `64-local-staging-simulation-guide.md` | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2 evidence packet |
| `64-local-staging-simulation-guide.md` | [`scripts/run_d1_d6_drills.py`](../../scripts/run_d1_d6_drills.py) | Automated local drill runner |
| `64-local-staging-simulation-guide.md` | [`scripts/check_pilot_readiness.py`](../../scripts/check_pilot_readiness.py) | Automated readiness checker |

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- This guide is for local-only practice and runbook validation
- No G2 complete claim is made by completing local simulation
- No production-ready claim is made in this document
- Local drill evidence is labeled "local/test-drill" and is NOT G2 evidence
- PostgreSQL/multi-node/HA are not implemented and not in scope
- Target environment evidence must be collected separately per docs 58/59

---

*Created: 2026-04-30. Documentation-only local simulation guide — no G2 complete, no production-ready, no PostgreSQL start, no target host required.*
