# 62 — Path 2 Operator Runbook

> **Status**: Documentation-only. Operator-owned execution runbook.
> **Purpose**: Step-by-step operator guide for executing the non-prod/prod-like pilot preparation flow (Path 2 Option B).
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. No production-ready claim.
> **Constraint**: Do not claim G2 complete, do not start PostgreSQL, no production-ready claim, no operator signature.

---

## Purpose

This runbook provides exact operator command sequences for executing Path 2 pilot preparation
using Option B (single-node SQLite non-prod/prod-like deployment). It supplements
[`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) with precise command blocks,
evidence paths, and acceptance criteria.

**Operator-owned**: All signoff gates require explicit operator action. Do not mark items
complete on behalf of the operator.

---

## Document Map

| Phase | Document | Purpose |
|-------|----------|---------|
| Execution plan | [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Ordered checklist and dependencies |
| **This runbook** | **62-path-2-operator-runbook.md** | **Exact operator command sequences** |
| Drill template | [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) | D1–D6 evidence capture |
| Readiness evidence | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence packet |
| Signoff packet | [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) | Operator signoff form |
| Support contract | [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md) | Known constraints |

---

## Option B: Non-Prod / Prod-Like Pilot

**Option B selected**: Single-node SQLite deployment in a non-production environment that
resembles production configuration (authentic TLS proxy, real backup/restore, D1–D6 drills).

**Boundaries**:
- Do NOT start PostgreSQL
- Do NOT claim G2 complete
- Do NOT claim production-ready
- Do NOT sign operator documents

---

## Phase 0 — Preflight Checks

Perform before any deployment or drill execution.

### 0.1 Environment Prerequisites

```bash
# Verify ferrumd binary available
which ferrumd || echo "ferrumd not found in PATH"

# Verify ferrumctl available
which ferrumctl || echo "ferrumctl not found in PATH"

# Verify Python 3 for evidence skeleton generation
python3 --version

# Check available disk space (store + backup directory)
df -h <store-path-placeholder> <backup-dir-placeholder>
```

### 0.2 Config Verification (Non-Prod)

```bash
# Verify non-prod config exists and is readable
ls -la /path/to/nonprod-ferrumgate.toml

# Review config (auth mode, bind address, store path)
# Do NOT cat secrets; use redacted placeholders in evidence
grep -E "auth_mode|bind_addr|store" /path/to/nonprod-ferrumgate.toml
```

### 0.3 Preflight Acceptance Criteria

| Check | Expected | Pass/Fail |
|-------|----------|-----------|
| `ferrumd` in PATH | executable found | |
| `ferrumctl` in PATH | executable found | |
| Python 3 available | version >= 3.8 | |
| Config file readable | file exists | |
| Store path writable | directory exists and writable | |
| Backup directory writable | directory exists and writable | |

### 0.4 Stop Conditions

| Trigger | Action |
|---------|--------|
| `ferrumd`/`ferrumctl` not in PATH | Install or add to PATH before proceeding |
| Store/backup path not writable | Create or fix permissions before proceeding |
| Config file missing | Create from `configs/ferrumgate.dev.toml` template |

---

## Phase 1 — Config Adaptation (configs/examples)

Adapt example configs for the target non-prod environment.

### 1.1 Config Template Selection

FerrumGate ships two example configs:

| File | Purpose | Auth |
|------|---------|------|
| `configs/ferrumgate.dev.toml` | Development (auth disabled, in-memory SQLite) | Disabled |
| `configs/ferrumgate.prod.toml` | Production (bearer auth required) | Bearer |

**For Option B non-prod**: Use `ferrumgate.prod.toml` as base (bearer auth enabled) but
deploy in non-prod network zone.

### 1.2 Config Adaptation Commands

```bash
# 1. Copy production template to working location
cp configs/ferrumgate.prod.toml /path/to/<non-prod-url>-ferrumgate.toml

# 2. Review and update bind address (use non-prod host)
#    NOTE: Do NOT hardcode bearer tokens; use env var or config placeholder
grep -n "bind_addr" /path/to/<non-prod-url>-ferrumgate.toml
# Expected output format: bind_addr = "127.0.0.1:8080"
# Replace with: bind_addr = "<target-host>:8080"

# 3. Review store path
grep -n "store" /path/to/<non-prod-url>-ferrumgate.toml
# Expected: store = { type = "sqlite", path = "<store-path>" }
# Replace <store-path> with actual path, e.g., /var/lib/ferrumgate/ferrumgate.db

# 4. Verify auth_mode is Bearer (not disabled)
grep -n "auth_mode" /path/to/<non-prod-url>-ferrumgate.toml
# Expected: auth_mode = "Bearer"
```

### 1.3 Placeholder Mapping

| Placeholder | Example Value | Notes |
|-------------|---------------|-------|
| `<non-prod-url>` | `nonprod-ferrumgate` | Config filename base |
| `<target-host>` | `10.0.1.100` or `nonprod.example.com` | Non-prod host |
| `<domain>` | `example.com` | TLS domain for proxy |
| `<store-path>` | `/var/lib/ferrumgate/ferrumgate.db` | SQLite store path |
| `<backup-dir>` | `/var/backups/ferrumgate` | Backup output directory |
| `<bearer-token-redacted>` | `fg_live_xxxx...xxxx` | Redacted token for evidence |

### 1.4 Config Acceptance Criteria

| Check | Expected | Pass/Fail |
|-------|----------|-----------|
| bind_addr set to non-prod host | `<target-host>:8080` | |
| auth_mode = "Bearer" | Bearer (not disabled) | |
| store.path exists | writable file path | |
| No secrets hardcoded | using env var or placeholder | |

---

## Phase 2 — Server Startup and Preflight Probe

### 2.1 Start ferrumd (Non-Prod)

```bash
# Start ferrumd with non-prod config
ferrumd --config /path/to/<non-prod-url>-ferrumgate.toml &

# Wait for startup
sleep 3

# Verify process is running
pgrep -f "ferrumd" || echo "ferrumd not running"
```

### 2.2 Preflight Probe Sequence

```bash
# Base URL for probes
FERRUM_BASE="http://<target-host>:8080"

# 2.2.1 Health check (shallow)
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/healthz"
# Expected: 200

# 2.2.2 Ready check (shallow)
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/readyz"
# Expected: 200

# 2.2.3 Deep readiness probe (functional)
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/readyz/deep"
# Expected: 200

# 2.2.4 Bearer auth test (should be challenged without token)
curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/approvals?limit=1"
# Expected: 401 (unauthorized)

# 2.2.5 Metrics endpoint (unauthenticated)
curl -s "${FERRUM_BASE}/v1/metrics" | head -20
# Expected: Prometheus text format with ferrumgate_ metrics
```

### 2.3 Probe Acceptance Criteria

| Probe | Endpoint | Expected Status | Pass/Fail |
|-------|----------|-----------------|-----------|
| healthz | `/v1/healthz` | 200 | |
| readyz | `/v1/readyz` | 200 | |
| readyz/deep | `/v1/readyz/deep` | 200 | |
| approvals (no auth) | `/v1/approvals` | 401 | |
| metrics | `/v1/metrics` | 200 + prometheus format | |

### 2.4 Stop Conditions

| Trigger | Action |
|---------|--------|
| `/v1/readyz/deep` returns non-200 | Do not proceed; investigate store/probe failure |
| `/v1/approvals` returns 200 without auth | Auth misconfigured; fix before proceeding |
| ferrumd crashes on startup | Check store path, permissions, config syntax |

---

## Phase 3 — D1–D6 Compensation Drills

Execute compensation drills using [`scripts/generate_evidence_skeleton.py`](../../scripts/generate_evidence_skeleton.py)
and capture output for evidence templates.

### 3.1 D1–D6 Drill Runner

**Server URL**: `http://<target-host>:8080`
**Bearer token**: Use `FERRUM_BEARER_TOKEN` env var (do NOT hardcode)

#### 3.1.1 D1 — FS Adapter Drills

```bash
FERRUM_BASE="http://<target-host>:8080"
FERRUM_TOKEN="${FERRUM_BEARER_TOKEN}"
DRILL_OUTPUT="/tmp/d1_drill_output.txt"

# D1.1 FileWrite Compensation Drill
echo "=== D1.1 FileWrite ===" > "$DRILL_OUTPUT"

# Create intent
INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "intent_type": "FileWrite",
    "resource": "/tmp/ferrum_drill_D1_1.txt",
    "content": "D1.1 FileWrite drill content",
    "rollback_class": "R1"
  }')
echo "Intent response: ${INTENT_RESPONSE}" >> "$DRILL_OUTPUT"

# Extract IDs (jq or grep/sed)
INTENT_ID=$(echo "$INTENT_RESPONSE" | grep -o '"intent_id":"[^"]*"' | cut -d'"' -f4)
echo "Intent ID: ${INTENT_ID}" >> "$DRILL_OUTPUT"

# Submit proposal
PROPOSAL_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/proposals" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -d "{\"intent_id\": \"${INTENT_ID}\", \"requested_rollback_class\": \"R1\"}")
echo "Proposal response: ${PROPOSAL_RESPONSE}" >> "$DRILL_OUTPUT"
PROPOSAL_ID=$(echo "$PROPOSAL_RESPONSE" | grep -o '"proposal_id":"[^"]*"' | cut -d'"' -f4)

# Approve
APPROVAL_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/approvals" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -d "{\"proposal_id\": \"${PROPOSAL_ID}\"}")
echo "Approval response: ${APPROVAL_RESPONSE}" >> "$DRILL_OUTPUT"
EXECUTION_ID=$(echo "$APPROVAL_RESPONSE" | grep -o '"execution_id":"[^"]*"' | cut -d'"' -f4)

# Verify file exists
echo "=== File state before compensate ===" >> "$DRILL_OUTPUT"
cat /tmp/ferrum_drill_D1_1.txt >> "$DRILL_OUTPUT" 2>&1 || echo "File not found" >> "$DRILL_OUTPUT"

# Compensate
echo "=== Compensate ===" >> "$DRILL_OUTPUT"
COMP_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "Compensate response: ${COMP_RESPONSE}" >> "$DRILL_OUTPUT"

# Verify file state after compensate
echo "=== File state after compensate ===" >> "$DRILL_OUTPUT"
cat /tmp/ferrum_drill_D1_1.txt >> "$DRILL_OUTPUT" 2>&1 || echo "File deleted (expected)" >> "$DRILL_OUTPUT"

# D1.2 FileDelete (similar pattern, omitted for brevity — see doc 58 template)
```

#### 3.1.2 D2 — Git Adapter Drills

```bash
# D2.1 GitCommit Compensation Drill
# Requires local git repo at /path/to/test/repo
echo "=== D2.1 GitCommit ===" > /tmp/d2_drill_output.txt

GIT_REPO="/path/to/test/repo"
cd "$GIT_REPO" || exit 1

# Create intent
INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"intent_type\": \"GitCommit\",
    \"repository_path\": \"${GIT_REPO}\",
    \"message\": \"D2 GitCommit drill commit\",
    \"rollback_class\": \"R1\"
  }")
echo "Intent: ${INTENT_RESPONSE}" >> /tmp/d2_drill_output.txt

# [Submit proposal → approve → execute flow]

# Compensate and verify
COMP_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "Compensate: ${COMP_RESPONSE}" >> /tmp/d2_drill_output.txt

echo "=== Git log after compensate ===" >> /tmp/d2_drill_output.txt
git -C "$GIT_REPO" log --oneline -2 >> /tmp/d2_drill_output.txt 2>&1

# D2.2 GitPush fail-closed verification (see doc 58 §D2.2)
```

#### 3.1.3 D3 — Git Remote Push / Fail-Closed Drills

> **Critical**: This drill verifies remote-dependent `fail_closed` semantics. Use a **non-prod remote only** (e.g., `/tmp/ferrum-remote-drill.git` or `<non-prod-git-remote-url>`). Do NOT use production remotes.

```bash
# D3.1 Git Remote Push Compensation Drill
# NOTE: Operator must use non-prod remote only
echo "=== D3.1 Git Remote Push ===" > /tmp/d3_drill_output.txt

# Create a local bare repo as the non-prod remote
DRILL_REMOTE_DIR="/tmp/ferrum-remote-drill.git"
DRILL_LOCAL_DIR="/tmp/ferrum-drill-d3-local"
rm -rf "$DRILL_REMOTE_DIR" "$DRILL_LOCAL_DIR"

# Setup: initialize bare remote and local clone
mkdir -p "$DRILL_REMOTE_DIR"
git -C "$DRILL_REMOTE_DIR" init --bare 2>&1

git clone "$DRILL_REMOTE_DIR" "$DRILL_LOCAL_DIR" 2>&1
cd "$DRILL_LOCAL_DIR" || exit 1

# Configure git user for drill
git config user.email "drill@ferrum.local"
git config user.name "Ferrum Drill"

# Create initial commit to establish main branch
echo "initial" > README.md
git add README.md
git commit -m "Initial commit"

# Push to establish remote tracking
git push -u origin main 2>&1

# Capture pre-state remote HEAD
echo "=== Pre-state remote HEAD ===" >> /tmp/d3_drill_output.txt
git -C "$DRILL_REMOTE_DIR" rev-parse HEAD >> /tmp/d3_drill_output.txt 2>&1

# D3.1: Create a new commit and push via intent
echo "=== D3.1 Git Remote Push intent ===" >> /tmp/d3_drill_output.txt

# Create intent for remote push
INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"intent_type\": \"GitPush\",
    \"repository_path\": \"${DRILL_LOCAL_DIR}\",
    \"remote\": \"origin\",
    \"ref\": \"main\",
    \"rollback_class\": \"R1\"
  }")
echo "Intent response: ${INTENT_RESPONSE}" >> /tmp/d3_drill_output.txt

INTENT_ID=$(echo "$INTENT_RESPONSE" | grep -o '"intent_id":"[^"]*"' | cut -d'"' -f4)
echo "Intent ID: ${INTENT_ID}" >> /tmp/d3_drill_output.txt

# Submit proposal
PROPOSAL_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/proposals" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -d "{\"intent_id\": \"${INTENT_ID}\", \"requested_rollback_class\": \"R1\"}")
echo "Proposal response: ${PROPOSAL_RESPONSE}" >> /tmp/d3_drill_output.txt
PROPOSAL_ID=$(echo "$PROPOSAL_RESPONSE" | grep -o '"proposal_id":"[^"]*"' | cut -d'"' -f4)

# Approve
APPROVAL_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/approvals" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -d "{\"proposal_id\": \"${PROPOSAL_ID}\"}")
echo "Approval response: ${APPROVAL_RESPONSE}" >> /tmp/d3_drill_output.txt
EXECUTION_ID=$(echo "$APPROVAL_RESPONSE" | grep -o '"execution_id":"[^"]*"' | cut -d'"' -f4)

# Capture remote HEAD after push
echo "=== Remote HEAD after push ===" >> /tmp/d3_drill_output.txt
git -C "$DRILL_REMOTE_DIR" rev-parse HEAD >> /tmp/d3_drill_output.txt 2>&1

# D3.1 compensate: verify remote rollback
echo "=== D3.1 Compensate ===" >> /tmp/d3_drill_output.txt
COMP_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "Compensate response: ${COMP_RESPONSE}" >> /tmp/d3_drill_output.txt

# Capture remote HEAD after compensate
echo "=== Remote HEAD after compensate ===" >> /tmp/d3_drill_output.txt
git -C "$DRILL_REMOTE_DIR" rev-parse HEAD >> /tmp/d3_drill_output.txt 2>&1

# D3.2: Git Remote Push fail-closed (remote deletion blocked)
# Configure remote with pre-receive hook blocking deletions to verify fail-closed
echo "=== D3.2 Git Remote Push fail-closed ===" >> /tmp/d3_drill_output.txt

# Setup a new remote repo with a blocking pre-receive hook
DRILL_REMOTE_FC_DIR="/tmp/ferrum-remote-drill-fc.git"
rm -rf "$DRILL_REMOTE_FC_DIR"
mkdir -p "$DRILL_REMOTE_FC_DIR"
git -C "$DRILL_REMOTE_FC_DIR" init --bare 2>&1

# Create a blocking pre-receive hook (denies any deletion)
cat > "$DRILL_REMOTE_FC_DIR/hooks/pre-receive" << 'HOOK'
#!/bin/bash
# Block all deletions - fail-closed behavior
while read old_rev new_rev ref; do
  if [ "$new_rev" = "0000000000000000000000000000000000000000" ]; then
    echo "error: pre-receive hook denied deletion of $ref" >&2
    exit 1
  fi
done
exit 0
HOOK
chmod +x "$DRILL_REMOTE_FC_DIR/hooks/pre-receive"

# Clone and push initial state
DRILL_LOCAL_FC_DIR="/tmp/ferrum-drill-d3-fc-local"
rm -rf "$DRILL_LOCAL_FC_DIR"
git clone "$DRILL_REMOTE_FC_DIR" "$DRILL_LOCAL_FC_DIR" 2>&1
cd "$DRILL_LOCAL_FC_DIR" || exit 1
git config user.email "drill@ferrum.local"
git config user.name "Ferrum Drill"
echo "fc_initial" > README.md
git add README.md
git commit -m "FC initial"
git push -u origin main 2>&1

# Capture pre-state
echo "Pre-state remote HEAD: $(git -C "$DRILL_REMOTE_FC_DIR" rev-parse HEAD)" >> /tmp/d3_drill_output.txt

# Create intent for push that will attempt deletion on compensate
INTENT_RESPONSE_FC=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"intent_type\": \"GitPush\",
    \"repository_path\": \"${DRILL_LOCAL_FC_DIR}\",
    \"remote\": \"origin\",
    \"ref\": \"main\",
    \"rollback_class\": \"R1\"
  }")
echo "FC Intent response: ${INTENT_RESPONSE_FC}" >> /tmp/d3_drill_output.txt

INTENT_ID_FC=$(echo "$INTENT_RESPONSE_FC" | grep -o '"intent_id":"[^"]*"' | cut -d'"' -f4)

# Submit proposal and approve
PROPOSAL_RESPONSE_FC=$(curl -s -X POST "${FERRUM_BASE}/v1/proposals" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -d "{\"intent_id\": \"${INTENT_ID_FC}\", \"requested_rollback_class\": \"R1\"}")
PROPOSAL_ID_FC=$(echo "$PROPOSAL_RESPONSE_FC" | grep -o '"proposal_id":"[^"]*"' | cut -d'"' -f4)

APPROVAL_RESPONSE_FC=$(curl -s -X POST "${FERRUM_BASE}/v1/approvals" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -d "{\"proposal_id\": \"${PROPOSAL_ID_FC}\"}")
EXECUTION_ID_FC=$(echo "$APPROVAL_RESPONSE_FC" | grep -o '"execution_id":"[^"]*"' | cut -d'"' -f4)

# Compensate - should fail because pre-receive hook blocks deletion
echo "FC Compensate response:" >> /tmp/d3_drill_output.txt
COMP_RESPONSE_FC=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID_FC}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "${COMP_RESPONSE_FC}" >> /tmp/d3_drill_output.txt

# Verify remote state unchanged (fail-closed: ref was NOT deleted)
echo "Remote HEAD after FC compensate: $(git -C "$DRILL_REMOTE_FC_DIR" rev-parse HEAD)" >> /tmp/d3_drill_output.txt

# Cleanup
rm -rf "$DRILL_REMOTE_DIR" "$DRILL_LOCAL_DIR" "$DRILL_REMOTE_FC_DIR" "$DRILL_LOCAL_FC_DIR"
```

**D3 Evidence / Expected Outputs:**

| Scenario | Expected `recovered` | Expected `failure_reason` |
|----------|---------------------|--------------------------|
| D3.1 Remote push compensates successfully | `true` | — |
| D3.2 Remote deletion blocked by hook | `false` | `remote_ref deletion denied` |

**D3 Acceptance Criteria:**

| Check | Expected | Pass/Fail |
|-------|----------|-----------|
| D3.1 Remote push creates commit on non-prod remote | remote ref updated | |
| D3.1 Compensate restores remote ref to pre-push state | `recovered: true` | |
| D3.2 Pre-receive hook blocks deletion | `recovered: false` | |
| D3.2 Remote ref unchanged after blocked compensate | remote ref unchanged | |

**D3 Stop Conditions:**

| Trigger | Action |
|---------|--------|
| D3.1 `recovered: false` with no valid failure reason | Investigate; do not proceed |
| D3.2 `recovered: true` when remote deletion blocked | Bug: fail-closed not working; abort pilot |
| Non-prod remote not available | Use `/tmp/ferrum-remote-drill.git` local bare repo |

**Constraint**: Operator must use non-prod remote only. Do NOT use production/persistent remotes for this drill.

#### 3.1.4 D4 — HTTP Adapter Drills

```bash
# D4.1 HTTP POST Replay Compensation
# Target: https://httpbin.example/post (replace with actual test endpoint)
echo "=== D4.1 HTTP POST ===" > /tmp/d4_drill_output.txt

IDEMPOTENCY_KEY="d4-drill-$(date +%s)"

INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"intent_type\": \"HttpMutation\",
    \"method\": \"POST\",
    \"url\": \"https://httpbin.example/post\",
    \"headers\": {\"Content-Type\": \"application/json\"},
    \"body\": \"{\\\"drill\\\": \\\"D4 HTTP POST\\\", \\\"key\\\": \\\"${IDEMPOTENCY_KEY}\\\"}\",
    \"compensation_plan\": {
      \"idempotency_key\": \"${IDEMPOTENCY_KEY}\",
      \"method\": \"DELETE\",
      \"url\": \"https://httpbin.example/delete/${IDEMPOTENCY_KEY}\"
    },
    \"rollback_class\": \"R1\"
  }")
echo "Intent: ${INTENT_RESPONSE}" >> /tmp/d4_drill_output.txt

# [Submit proposal → approve → execute flow]

# Compensate
COMP_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "Compensate: ${COMP_RESPONSE}" >> /tmp/d4_drill_output.txt

# D4.2 HTTP fail-closed (no compensation_plan) — see doc 58 §D4.2
```

#### 3.1.5 D5 — SQLite Adapter Drill

```bash
echo "=== D5 SQLite DML ===" > /tmp/d5_drill_output.txt

INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "intent_type": "SqliteMutation",
    "sql": "INSERT INTO drill_table (id, value) VALUES (1, '\''D5 drill'\'')",
    "compensation_sql": "DELETE FROM drill_table WHERE id = 1",
    "rollback_class": "R1"
  }')
echo "Intent: ${INTENT_RESPONSE}" >> /tmp/d5_drill_output.txt

# [Submit proposal → approve → execute flow]

# Compensate
COMP_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "Compensate: ${COMP_RESPONSE}" >> /tmp/d5_drill_output.txt
```

#### 3.1.6 D6 — Maildraft Adapter Drill

```bash
echo "=== D6 Maildraft ===" > /tmp/d6_drill_output.txt

INTENT_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/intents" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "intent_type": "MailDraftCreate",
    "to": "drill@example.com",
    "subject": "D6 drill",
    "body": "D6 maildraft compensation drill",
    "rollback_class": "R1"
  }')
echo "Intent: ${INTENT_RESPONSE}" >> /tmp/d6_drill_output.txt

# [Submit proposal → approve → execute flow]

# Compensate
COMP_RESPONSE=$(curl -s -X POST "${FERRUM_BASE}/v1/executions/${EXECUTION_ID}/compensate" \
  -H "Authorization: Bearer ${FERRUM_TOKEN}")
echo "Compensate: ${COMP_RESPONSE}" >> /tmp/d6_drill_output.txt
```

### 3.2 Evidence Skeleton Generation

After running drills, generate evidence skeletons:

```bash
# Generate D1-D6 evidence skeleton from drill output files
cat /tmp/d1_drill_output.txt /tmp/d2_drill_output.txt /tmp/d3_drill_output.txt \
    /tmp/d4_drill_output.txt /tmp/d5_drill_output.txt /tmp/d6_drill_output.txt \
    | python3 scripts/generate_evidence_skeleton.py --type d1-d6 \
    > /tmp/d1_d6_skeleton.md

# Copy to evidence template (operator reviews and signs)
cp /tmp/d1_d6_skeleton.md docs/implementation-path/58-workload-compensation-drill-evidence-template.md.operator-draft

echo "Evidence skeleton written to /tmp/d1_d6_skeleton.md"
echo "Copy to 58-workload-compensation-drill-evidence-template.md for operator review"
```

### 3.3 Evidence Paths

| Evidence | Path | Status |
|----------|------|--------|
| D1.1 FileWrite drill output | `/tmp/d1_drill_output.txt` | ☐ Operator pending |
| D1.2 FileDelete drill output | `/tmp/d1_drill_output.txt` | ☐ Operator pending |
| D2.1 GitCommit drill output | `/tmp/d2_drill_output.txt` | ☐ Operator pending |
| D3.1 Git Remote Push output | `/tmp/d3_drill_output.txt` | ☐ Operator pending |
| D3.2 Git Remote Push fail-closed output | `/tmp/d3_drill_output.txt` | ☐ Operator pending |
| D4.1 HTTP POST replay output | `/tmp/d4_drill_output.txt` | ☐ Operator pending |
| D4.2 HTTP fail-closed output | `/tmp/d4_drill_output.txt` | ☐ Operator pending |
| D5 SQLite DML output | `/tmp/d5_drill_output.txt` | ☐ Operator pending |
| D6 Maildraft output | `/tmp/d6_drill_output.txt` | ☐ Operator pending |
| D1-D6 skeleton | `/tmp/d1_d6_skeleton.md` | ☐ Operator pending |
| Final evidence template | `docs/implementation-path/58-workload-compensation-drill-evidence-template.md` | ☐ Operator signs |

### 3.4 Drill Acceptance Criteria

| Drill | recovered | Acceptable Exception? | Operator Signoff |
|-------|-----------|----------------------|------------------|
| D1.1 FileWrite | true / false | yes / no | |
| D1.2 FileDelete | true / false | yes / no | |
| D2.1 GitCommit | true / false | yes / no | |
| D3.1 Git Remote Push | true / false | yes / no | |
| D3.2 Git Remote Push fail-closed | verified | yes / no | |
| D4.1 HTTP POST replay | true / false | yes / no | |
| D4.2 HTTP fail-closed | verified | yes / no | |
| D5 SQLite DML | true / false | yes / no | |
| D6 Maildraft | true / false | yes / no | |

### 3.5 Stop Conditions

| Trigger | Action |
|---------|--------|
| Any drill `recovered: false` with unacceptable risk | Abort pilot; adapter implementation required |
| `fail_closed_verified: false` on D3.2 Git Remote Push or D4.2 HTTP | Abort pilot; adapter fix required |
| Compensate noop confirmed for target adapter | Operator accepts noop risk or aborts |

---

## Phase 4 — Evidence Copy and Review (Docs 58/59)

### 4.1 Copy Evidence to Templates

```bash
# Copy D1-D6 skeleton to doc 58 (operator reviews and signs)
# NOTE: Do NOT auto-approve or mark complete
cp /tmp/d1_d6_skeleton.md docs/implementation-path/58-workload-compensation-drill-evidence-template.md.operator-draft

# Generate G2 readiness skeleton from probe output
# Run probes first, save output:
curl -s "${FERRUM_BASE}/v1/readyz/deep" > /tmp/readyz_deep_output.txt
curl -s "${FERRUM_BASE}/v1/metrics" > /tmp/metrics_output.txt

cat /tmp/readyz_deep_output.txt /tmp/metrics_output.txt \
    | python3 scripts/generate_evidence_skeleton.py --type g2 \
    > /tmp/g2_skeleton.md

echo "G2 skeleton written to /tmp/g2_skeleton.md"
```

### 4.2 Evidence Review Checklist

Operator must review and sign:

- [ ] [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) — D1–D6 complete with operator annotations
- [ ] [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) — G2.1–G2.8 complete with operator annotations

### 4.3 Evidence Fields for Doc 59 (G2.1–G2.8)

| G2 Item | Evidence Path | Operator Action |
|---------|---------------|-----------------|
| G2.1 Workload Model | Operator workload model document | Operator fills and signs §G2.1 |
| G2.2 Auth/TLS Configuration | Adapted config + TLS/proxy evidence | Operator fills and signs §G2.2 |
| G2.3 Backup Schedule | Scheduler config + backup job log | Operator fills and signs §G2.3 |
| G2.4 Restore Drill | `/tmp/restore_drill_output.txt` | Operator fills and signs §G2.4 |
| G2.5 RPO/RTO Acceptance | Restore timing + backup interval evidence | Operator fills and signs §G2.5 |
| G2.6 Production Evaluation | Completed evaluation framework | Operator fills and signs §G2.6 |
| G2.7 Accepted-Risk Review | Weak Spots + support contract review | Operator fills and signs §G2.7 |
| G2.8 Compensate Noop Acceptance | Signed Doc 58 + adapter matrix | Operator fills and signs §G2.8 |

---

## Phase 5 — Target Restore Drill

Per [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) §Step 3.

### 5.1 Restore Drill Commands

```bash
# Configuration
STORE_PATH="<store-path>"           # e.g., /var/lib/ferrumgate/ferrumgate.db
BACKUP_DIR="<backup-dir>"           # e.g., /var/backups/ferrumgate
FERRUM_BASE="http://<target-host>:8080"
FERRUM_TOKEN="${FERRUM_BEARER_TOKEN}"
DRILL_LOG="/tmp/restore_drill_output.txt"

echo "=== Restore Drill Log ===" > "$DRILL_LOG"
echo "Date: $(date)" >> "$DRILL_LOG"
echo "Store: ${STORE_PATH}" >> "$DRILL_LOG"
echo "Backup: ${BACKUP_DIR}" >> "$DRILL_LOG"

# 5.1.1 Create fresh backup
echo "=== Step 1: Create backup ===" >> "$DRILL_LOG"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.db"

ferrumctl backup create --db-path "$STORE_PATH" --output-dir "$BACKUP_DIR" 2>&1
BACKUP_RESULT=$?
echo "Backup exit code: ${BACKUP_RESULT}" >> "$DRILL_LOG"

# Find latest backup
LATEST_BACKUP=$(ls -t "${BACKUP_DIR}"/ferrumgate_*.db 2>/dev/null | head -1)
echo "Latest backup: ${LATEST_BACKUP}" >> "$DRILL_LOG"

# 5.1.2 Verify backup integrity
echo "=== Step 2: Verify backup ===" >> "$DRILL_LOG"
ferrumctl backup verify --db-path "$LATEST_BACKUP" >> "$DRILL_LOG" 2>&1
VERIFY_RESULT=$?
echo "Verify exit code: ${VERIFY_RESULT}" >> "$DRILL_LOG"

# 5.1.3 Stop ferrumd
echo "=== Step 3: Stop ferrumd ===" >> "$DRILL_LOG"
FERRUM_PID=$(pgrep -f "ferrumd" | head -1)
if [ -n "$FERRUM_PID" ]; then
    echo "Stopping ferrumd (PID: ${FERRUM_PID})" >> "$DRILL_LOG"
    kill "$FERRUM_PID"
    sleep 2
    echo "ferrumd stopped" >> "$DRILL_LOG"
else
    echo "ferrumd not running" >> "$DRILL_LOG"
fi

# 5.1.4 Perform restore
echo "=== Step 4: Restore ===" >> "$DRILL_LOG"
ferrumctl backup restore \
    --db-path "$STORE_PATH" \
    --from "$LATEST_BACKUP" \
    --confirm 2>&1
RESTORE_RESULT=$?
echo "Restore exit code: ${RESTORE_RESULT}" >> "$DRILL_LOG"

# 5.1.5 Post-restore verify
echo "=== Step 5: Post-restore verify ===" >> "$DRILL_LOG"
ferrumctl backup verify --db-path "$STORE_PATH" >> "$DRILL_LOG" 2>&1
POST_VERIFY=$?
echo "Post-restore verify exit code: ${POST_VERIFY}" >> "$DRILL_LOG"

# 5.1.6 Restart and probe
echo "=== Step 6: Restart and probe ===" >> "$DRILL_LOG"
ferrumd --config /path/to/<non-prod-url>-ferrumgate.toml &
sleep 3

READYZ_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "${FERRUM_BASE}/v1/readyz/deep")
echo "readyz/deep status: ${READYZ_STATUS}" >> "$DRILL_LOG"

echo "=== Restore drill complete ===" >> "$DRILL_LOG"
echo "Log: ${DRILL_LOG}"
```

### 5.2 Restore Drill Evidence Fields

| Field | Value |
|-------|-------|
| `backup_file_used` | `<backup-dir>/ferrumgate_<timestamp>.db` |
| `backup_verify_pre_restore` | OK / FAILED |
| `restore_completed` | true / false |
| `pre_restore_copy_created` | true / false |
| `backup_verify_post_restore` | OK / FAILED |
| `ferrumd_restarted` | true / false |
| `readyz_deep_returns_200` | true / false |
| `operator_annotation` | `<any anomalies or deviations>` |

### 5.3 Restore Drill Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| `ferrumctl backup verify` passes pre-restore | OK | |
| `.pre_restore` copy created | true | |
| `ferrumctl backup restore` completes | true | |
| `ferrumctl backup verify` passes post-restore | OK | |
| `GET /v1/readyz/deep` returns 200 after restart | 200 | |

### 5.4 Stop Conditions

| Trigger | Action |
|---------|--------|
| `ferrumctl backup verify` fails pre-restore | Do not restore; take new backup; investigate |
| `ferrumctl backup restore` refuses (DB locked) | Stop ferrumd; retry restore |
| `ferrumctl backup verify` fails post-restore | Abort; restore `.pre_restore`; investigate |
| `readyz/deep` returns non-200 after restart | Abort; investigate; restore `.pre_restore` if needed |

---

## Phase 6 — Backup Scheduler Verification

Verify backup scheduler is operational (external to FerrumGate).

### 6.1 Scheduler Configuration Review

```bash
# Check cron configuration
ls -la /etc/cron.d/ferrumgate-backup 2>/dev/null || echo "No cron.d backup config"
crontab -l 2>/dev/null | grep ferrum || echo "No crontab entry"

# Check systemd timer
systemctl list-timers --all 2>/dev/null | grep ferrum || echo "No systemd timer found"
systemctl status ferrumgate-backup.timer 2>/dev/null || echo "Timer not found"

# Verify scheduler can run manually
ferrumctl backup create --db-path "<store-path>" --output-dir "<backup-dir>"
```

### 6.2 Backup Scheduler Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| Scheduler config exists | cron, systemd, or CI job | |
| Backup runs on schedule | cron/systemd timer active | |
| Retention policy enforced | old backups pruned | |
| `ferrumctl backup verify` passes after each backup | OK | |
| Backup evidence logged | log output available | |

---

## Phase 7 — TLS Proxy Verification

Verify TLS termination is configured at reverse proxy (external to FerrumGate).

### 7.1 TLS Proxy Verification Commands

```bash
# 7.1.1 TLS certificate check
echo "=== TLS Certificate Check ==="
openssl s_client -connect <domain>:443 -servername <domain> </dev/null 2>/dev/null \
    | openssl x509 -noout -dates -subject 2>/dev/null \
    || echo "TLS check failed (expected if proxy not yet configured)"

# 7.1.2 HTTPS probe through proxy
curl -s -o /dev/null -w "%{http_code}" "https://<domain>/v1/healthz"
# Expected: 200 (or 401 for authed routes)

# 7.1.3 HTTP redirect check
curl -s -o /dev/null -w "%{http_code}" "http://<domain>/v1/healthz"
# Expected: 301 (redirect to HTTPS)

# 7.1.4 Bearer token passthrough
curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer <bearer-token-redacted>" \
    "https://<domain>/v1/readyz/deep"
# Expected: 200

# 7.1.5 Health endpoints unauthenticated through proxy
curl -s -o /dev/null -w "%{http_code}" "https://<domain>/v1/healthz"
# Expected: 200 (intentionally unauthenticated)
```

### 7.2 TLS Proxy Acceptance Criteria

| Criterion | Expected | Pass/Fail |
|-----------|----------|-----------|
| TLS 1.2+ configured | certificate valid | |
| HTTP → HTTPS redirect | 301 redirect | |
| Health endpoints reachable | 200 | |
| Bearer token forwarded to backend | 200 on authed routes | |
| `/v1/readyz/deep` returns 200 through proxy | 200 | |

### 7.3 Stop Conditions

| Trigger | Action |
|---------|--------|
| TLS not configured | Do not expose non-loopback without TLS |
| Certificate invalid or expired | Fix before production pilot |
| Health endpoints not reachable through proxy | Fix proxy configuration |

---

## Phase 8 — Doc 54 Signoff Gate

Complete operator signoff per [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md).

### 8.1 Signoff Prerequisites

All of the following must be satisfied before signing doc 54:

| Prerequisite | Evidence | Status |
|--------------|----------|--------|
| G2.1–G2.8 complete | Doc 59 signed | ☐ |
| D1–D6 drills complete | Doc 58 signed | ☐ |
| Restore drill successful | `/tmp/restore_drill_output.txt` | ☐ |
| Backup scheduler operational | Scheduler config + evidence | ☐ |
| TLS/reverse proxy operational | Proxy config + probe | ☐ |

### 8.2 Signoff Action

```bash
# Copy evidence to signoff packet
echo "Evidence collected. Operator must sign 54-operator-signoff-packet.md §Pilot Acceptance Statement"
echo ""
echo "Signoff Prerequisites Summary:"
echo "  - Doc 59 (G2.1-G2.8): Pending operator signature"
echo "  - Doc 58 (D1-D6): Pending operator signature"
echo "  - Restore drill: /tmp/restore_drill_output.txt"
echo "  - Backup scheduler: Verified"
echo "  - TLS proxy: Verified"
echo ""
echo "Next action: Operator reviews and signs 54-operator-signoff-packet.md"
```

### 8.3 Stop Conditions

| Trigger | Action |
|---------|--------|
| Any G2 prerequisite not satisfied | Do not sign; resolve gaps first |
| Operator declines any signoff item | Abort or formally accept risk with notation |

---

## Phase 9 — Phase 3 Decision Gate

Decide Phase 3 outcome only after all Path 2 evidence is complete.

### 9.1 Decision Prerequisites

| Prerequisite | Evidence | Status |
|--------------|----------|--------|
| G2.1–G2.8 signed | Doc 59 signed | ☐ |
| D1–D6 drills signed | Doc 58 signed | ☐ |
| Restore drill logged | `/tmp/restore_drill_output.txt` | ☐ |
| Backup scheduler verified | Config + evidence | ☐ |
| TLS proxy verified | Config + probe | ☐ |
| Operator signoff complete | Doc 54 signed | ☐ |

### 9.2 Decision Criteria

| Decision | Criteria | Next Action |
|----------|----------|-------------|
| **Proceed to Phase 3** | Pilot confirms single-node SQLite inadequate (e.g., >300 writes/s) OR operator prefers PostgreSQL | Engineering lead initiates Phase P1 per ADR-50 |
| **Continue Path 2 (bounded)** | Pilot confirms single-node SQLite acceptable | Operator continues bounded production use; Phase 3 deferred |
| **Abort pilot** | Any abort trigger fires | Investigate, fix, and re-evaluate or formally close |

### 9.3 Phase 3 Decision Command

```bash
# Log Phase 3 decision
DECISION_LOG="/tmp/phase3_decision_$(date +%Y%m%d_%H%M%S).log"
echo "=== Phase 3 Decision Log ===" > "$DECISION_LOG"
echo "Date: $(date)" >> "$DECISION_LOG"
echo "Operator: <operator-name>" >> "$DECISION_LOG"
echo "Decision: <Proceed to Phase 3 | Continue Path 2 | Abort pilot>" >> "$DECISION_LOG"
echo "Rationale: <operator justification>" >> "$DECISION_LOG"
echo "" >> "$DECISION_LOG"
echo "Evidence reviewed:" >> "$DECISION_LOG"
echo "  - Doc 59: <signed|unsigned>" >> "$DECISION_LOG"
echo "  - Doc 58: <signed|unsigned>" >> "$DECISION_LOG"
echo "  - Restore drill log: /tmp/restore_drill_output.txt" >> "$DECISION_LOG"
echo "  - Backup scheduler: Verified" >> "$DECISION_LOG"
echo "  - TLS proxy: Verified" >> "$DECISION_LOG"
echo "" >> "$DECISION_LOG"
echo "Next steps: <based on decision>" >> "$DECISION_LOG"

echo "Decision log: ${DECISION_LOG}"
```

---

## Evidence Summary

| Phase | Evidence | Location |
|-------|----------|----------|
| Phase 0 preflight | Environment check | Operator terminal |
| Phase 1 config | Adapted config | `/path/to/<non-prod-url>-ferrumgate.toml` |
| Phase 2 probes | Probe output | `/tmp/readyz_deep_output.txt`, `/tmp/metrics_output.txt` |
| Phase 3 drills | D1–D6 drill logs | `/tmp/d{1,2,3,4,5,6}_drill_output.txt` |
| Phase 3 skeleton | Evidence skeleton | `/tmp/d1_d6_skeleton.md` |
| Phase 4 review | Doc 58, Doc 59 | `docs/implementation-path/58-*.md`, `59-*.md` |
| Phase 5 restore | Restore drill log | `/tmp/restore_drill_output.txt` |
| Phase 6 scheduler | Scheduler config | External (cron/systemd) |
| Phase 7 TLS | Proxy probe output | Operator terminal |
| Phase 8 signoff | Doc 54 | `docs/implementation-path/54-operator-signoff-packet.md` |
| Phase 9 decision | Decision log | `/tmp/phase3_decision_<timestamp>.log` |

---

## Rollback and Recovery Notes

| Scenario | Recovery Action |
|----------|----------------|
| ferrumd fails to start | Check store path, permissions, config syntax; restore from backup |
| Drill causes state corruption | Restore from backup via `ferrumctl backup restore` |
| Restore drill fails | Do not proceed; investigate and retake backup |
| Proxy TLS misconfigured | Revert to direct HTTP on loopback; do not expose |
| Any G2 item fails | Resolve before operator signoff |

---

## Explicit Non-Claims

- **No G2 complete claim**: G2 gates remain pending until operator signs doc 59
- **No production-ready claim**: FerrumGate v1 remains RC-ready/conditional
- **No PostgreSQL**: PostgreSQL is blocked until Phase 3 decision gates are satisfied
- **No operator signature pre-populated**: All signature fields remain blank for operator

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- This runbook provides execution guidance for Option B non-prod/prod-like pilot preparation
- No production-ready claim is made in this document
- PostgreSQL/multi-node/HA are not implemented and not in scope
- All operator signoff gates require explicit operator action and signature
- This document does not authorize production deployment

---

*Created: 2026-04-30. Documentation-only operator runbook — no G2 complete, no production-ready, no PostgreSQL start, no operator signature pre-populated.*

(End of file — total 862 lines)