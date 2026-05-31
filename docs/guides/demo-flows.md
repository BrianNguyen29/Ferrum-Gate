# Demo Flows — Copy-Paste Runnable Guides

> **Scope**: Local loopback-only. Do not run against any remote target or real bearer token.
> **Parent**: [`guides/README.md`](./README.md)

---

## Caveats

- **Docs are scaffolds, not execution proof**: This guide provides copy-paste command skeletons. It does not certify that each flow was run end-to-end in this session unless existing evidence is cited.
- **Local dev config only**: Use `configs/ferrumgate.dev.toml` or `docker-compose.demo.yml`; auth disabled, loopback binding, temp dirs only.
- **No remote target**: Do not run any demo command against a remote IP. Use `127.0.0.1` or `localhost` only.
- **No real secrets**: Do not use real bearer tokens, credentials, or storage DSNs.

---

## Demo 1 — Governed File Write

### Purpose

Demonstrate a full governed file write: intent submission → evaluation → capability minting → snapshot → execute → verify → lineage query → rollback.

### Prerequisites

- ferrumd built (`cargo build --release`)
- ferrumd running locally with dev config: `FERRUMD_BIND_ADDR=127.0.0.1:18080 ./target/release/ferrumd --config configs/ferrumgate.dev.toml`
- or via Docker Compose: `docker compose -f docker-compose.demo.yml up -d`
- curl available

### Command Skeleton

```bash
# Variables — replace placeholders with values from earlier steps
FERRUM_GATEWAY="http://127.0.0.1:18080"   # or http://127.0.0.1:8080 when using compose
INTENT_ID="<INTENT_ID>"
PROPOSAL_ID="<PROPOSAL_ID>"
CAPABILITY_ID="<CAPABILITY_ID>"
EXECUTION_ID="<EXECUTION_ID>"
TEMP_FILE="/tmp/ferrum-demo-$(date +%s).txt"

# Step 1 — Submit intent
INTENT_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/intents/compile" \
  -H "Content-Type: application/json" \
  -d "{
    \"principal_id\":\"d228bf73-c4f5-467a-b47b-53110dca7270\",
    \"title\":\"demo-file-write\",
    \"goal\":\"write a demo file\",
    \"agent_plan_summary\":\"write $TEMP_FILE\",
    \"trusted_context\":{},
    \"raw_inputs\":[],
    \"requested_resource_scope\":[{\"kind\":\"FilesystemPath\",\"path\":\"$TEMP_FILE\",\"mode\":\"Write\"}],
    \"metadata\":{}
  }")
INTENT_ID=$(echo "$INTENT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['envelope']['intent_id'])")
echo "INTENT_ID=$INTENT_ID"

# Step 2 — Evaluate proposal (submit proposal + evaluate)
PROPOSAL_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
curl -s -X POST "$FERRUM_GATEWAY/v1/proposals/$PROPOSAL_ID/evaluate" \
  -H "Content-Type: application/json" \
  -d "{
    \"proposal_id\":\"$PROPOSAL_ID\",
    \"intent_id\":\"$INTENT_ID\",
    \"step_index\":1,
    \"title\":\"demo file write\",
    \"tool_name\":\"fs.write\",
    \"server_name\":\"fs\",
    \"raw_arguments\":{\"path\":\"$TEMP_FILE\",\"content\":\"hello governed write\"},
    \"expected_effect\":\"FileMutation\",
    \"estimated_risk\":\"Low\",
    \"requested_rollback_class\":\"R0NativeReversible\",
    \"taint_inputs\":[],
    \"metadata\":{},
    \"created_at\":\"2026-05-30T00:00:00Z\"
  }"

# Step 3 — Mint capability
CAP_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/capabilities/mint" \
  -H "Content-Type: application/json" \
  -d "{
    \"intent_id\":\"$INTENT_ID\",
    \"proposal_id\":\"$PROPOSAL_ID\",
    \"tool_binding\":{\"server_name\":\"fs\",\"tool_name\":\"fs.write\"},
    \"resource_bindings\":[{\"kind\":\"File\",\"path\":\"$TEMP_FILE\",\"mode\":\"Write\"}],
    \"argument_constraints\":[],
    \"taint_budget\":{\"max_taint_score\":10,\"allow_external_tool_output\":true,\"allow_external_metadata\":true,\"allow_untrusted_text\":true},
    \"approval_binding\":null,
    \"requested_ttl_secs\":60,
    \"metadata\":{}
  }")
CAPABILITY_ID=$(echo "$CAP_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['lease']['capability_id'])")
echo "CAPABILITY_ID=$CAPABILITY_ID"

# Step 4 — Authorize
AUTH_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/executions/authorize" \
  -H "Content-Type: application/json" \
  -d "{\"proposal_id\":\"$PROPOSAL_ID\",\"capability_id\":\"$CAPABILITY_ID\",\"dry_run\":false}")
EXECUTION_ID=$(echo "$AUTH_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['execution']['execution_id'])")
echo "EXECUTION_ID=$EXECUTION_ID"

# Step 5 — Prepare
curl -s -X POST "$FERRUM_GATEWAY/v1/executions/$EXECUTION_ID/prepare" -H "Content-Type: application/json"

# Step 6 — Execute
curl -s -X POST "$FERRUM_GATEWAY/v1/executions/$EXECUTION_ID/execute" \
  -H "Content-Type: application/json" \
  -d '{"payload":{"content":"hello governed write"}}'

# Step 7 — Verify
curl -s -X POST "$FERRUM_GATEWAY/v1/executions/$EXECUTION_ID/verify" -H "Content-Type: application/json"

# Step 8 — Evaluate outcome
curl -s -X POST "$FERRUM_GATEWAY/v1/executions/$EXECUTION_ID/evaluate-outcome" \
  -H "Content-Type: application/json" \
  -d "{\"execution_id\":\"$EXECUTION_ID\",\"actual_effect\":\"FileMutation\",\"description\":\"demo file write\",\"result_digest\":null,\"adapter_success\":true,\"adapter_metadata\":{}}"

# Step 9 — Query lineage
curl -s "$FERRUM_GATEWAY/v1/provenance/lineage/$EXECUTION_ID"

# Step 10 — Rollback (compensate)
curl -s -X POST "$FERRUM_GATEWAY/v1/executions/$EXECUTION_ID/compensate" \
  -H "Content-Type: application/json" \
  -d "{\"execution_id\":\"$EXECUTION_ID\",\"reason\":\"demo rollback\"}"
```

### Expected Outcomes

- `/v1/proposals/{id}/evaluate` → decision `Allow`
- `/v1/executions/{id}/prepare` → `prepared: true`
- `/v1/executions/{id}/execute` → `executed: true`
- `/v1/executions/{id}/verify` → `verified: true`
- `/v1/provenance/lineage/{id}` → JSON with `events` array
- `compensate` → restores original state (file deleted or contents reverted)

### Cleanup

```bash
rm -f /tmp/ferrum-demo-*.txt
# or when using compose:
docker compose -f docker-compose.demo.yml down
```

### Acceptance Checks

- [ ] Intent compiles with `intent_id` returned
- [ ] Evaluate returns `Allow` (or advisory with `Allow`)
- [ ] Capability minted with `capability_id` returned
- [ ] Execution lifecycle (authorize → prepare → execute → verify) completes without error
- [ ] Lineage query returns events array
- [ ] Compensate/rollback succeeds
- [ ] Temp file removed after cleanup

---

## Demo 2 — Governed Git Commit

### Purpose

Demonstrate governed git commit: create temp repo → file change → submit intent → evaluate → commit with snapshot → verify → rollback reset.

### Prerequisites

- ferrumd running locally (dev config, auth disabled)
- git available on PATH
- temp directory writable

### Command Skeleton

```bash
FERRUM_GATEWAY="http://127.0.0.1:18080"
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"
git init
git config user.email "demo@example.com"
git config user.name "Demo User"

# Make a file change
echo "initial content" > demo.txt
git add demo.txt
git commit -m "initial commit"
HEAD_BEFORE=$(git rev-parse HEAD)
echo "HEAD before=$HEAD_BEFORE"

# Modify the file
echo "modified content" > demo.txt

# Submit intent for git commit
INTENT_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/intents/compile" \
  -H "Content-Type: application/json" \
  -d "{
    \"principal_id\":\"d228bf73-c4f5-467a-b47b-53110dca7270\",
    \"title\":\"demo-git-commit\",
    \"goal\":\"commit demo file change\",
    \"agent_plan_summary\":\"git add demo.txt && git commit\",
    \"trusted_context\":{},
    \"raw_inputs\":[],
    \"requested_resource_scope\":[{\"kind\":\"GitRepository\",\"path\":\"$TEMP_DIR\",\"mode\":\"Write\"}],
    \"metadata\":{}
  }")
INTENT_ID=$(echo "$INTENT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['envelope']['intent_id'])")
echo "INTENT_ID=$INTENT_ID"

# Evaluate (submit proposal + evaluate)
# Generate a UUID for the proposal_id
PROPOSAL_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
curl -s -X POST "$FERRUM_GATEWAY/v1/proposals/$PROPOSAL_ID/evaluate" \
  -H "Content-Type: application/json" \
  -d "{
    \"proposal_id\":\"$PROPOSAL_ID\",
    \"intent_id\":\"$INTENT_ID\",
    \"step_index\":1,
    \"title\":\"demo git commit\",
    \"tool_name\":\"git.commit\",
    \"server_name\":\"git\",
    \"raw_arguments\":{\"dir\":\"$TEMP_DIR\",\"message\":\"demo commit\"},
    \"expected_effect\":\"GitMutation\",
    \"estimated_risk\":\"Low\",
    \"requested_rollback_class\":\"R0NativeReversible\",
    \"taint_inputs\":[],
    \"metadata\":{},
    \"created_at\":\"2026-05-30T00:00:00Z\"
  }"

# Mint capability, authorize, prepare, execute, verify (see Demo 1 skeleton)
# ... (substitute git tool bindings: server_name=git, tool_name=git.commit)

# After execution, verify commit was created
git -C "$TEMP_DIR" log --oneline -1

# Rollback: reset to HEAD before
git -C "$TEMP_DIR" reset --hard "$HEAD_BEFORE"
echo "Rolled back to $HEAD_BEFORE"

# Cleanup
cd /
rm -rf "$TEMP_DIR"
```

### Expected Outcomes

- Git commit executes via governed tool path
- `git log` shows new commit after execute
- `git reset --hard` restores pre-commit state after compensate
- Temp repo removed after cleanup

### Cleanup

```bash
rm -rf "$TEMP_DIR"
```

### Acceptance Checks

- [ ] Temp git repo created and initial commit made
- [ ] Intent submitted with `intent_id` returned
- [ ] Proposal evaluated (expect `Allow` for low-risk git commit)
- [ ] Execution lifecycle completes
- [ ] New commit visible after execute
- [ ] `git reset --hard` succeeds during rollback
- [ ] Temp repo removed after cleanup

---

## Demo 3 — Governed SQLite Mutation

### Purpose

Demonstrate governed SQLite mutation: create temp DB → submit SQL mutation intent → prepare savepoint → execute → verify → compensate restore.

### Prerequisites

- ferrumd running locally (dev config, auth disabled)
- sqlite3 available on PATH
- ferrumd built with `fs` and SQLite adapter available

### Command Skeleton

```bash
FERRUM_GATEWAY="http://127.0.0.1:18080"
TEMP_DB="/tmp/ferrum-demo-$(date +%s).db"
TEMP_TABLE="demo_table"

# Create temp SQLite DB
sqlite3 "$TEMP_DB" "CREATE TABLE $TEMP_TABLE (id INTEGER PRIMARY KEY, value TEXT);"
sqlite3 "$TEMP_DB" "INSERT INTO $TEMP_TABLE (value) VALUES ('initial');"
echo "DB created at $TEMP_DB"

# Submit intent for SQL mutation
INTENT_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/intents/compile" \
  -H "Content-Type: application/json" \
  -d "{
    \"principal_id\":\"d228bf73-c4f5-467a-b47b-53110dca7270\",
    \"title\":\"demo-sqlite-mutation\",
    \"goal\":\"insert into $TEMP_TABLE\",
    \"agent_plan_summary\":\"sqlite3 insert\",
    \"trusted_context\":{},
    \"raw_inputs\":[],
    \"requested_resource_scope\":[{\"kind\":\"DatabasePath\",\"path\":\"$TEMP_DB\",\"mode\":\"Write\"}],
    \"metadata\":{}
  }")
INTENT_ID=$(echo "$INTENT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['envelope']['intent_id'])")
echo "INTENT_ID=$INTENT_ID"

# Evaluate proposal (use a UUID for proposal_id)
PROPOSAL_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
curl -s -X POST "$FERRUM_GATEWAY/v1/proposals/$PROPOSAL_ID/evaluate" \
  -H "Content-Type: application/json" \
  -d "{
    \"proposal_id\":\"$PROPOSAL_ID\",
    \"intent_id\":\"$INTENT_ID\",
    \"step_index\":1,
    \"title\":\"demo sqlite insert\",
    \"tool_name\":\"sqlite.write\",
    \"server_name\":\"sqlite\",
    \"raw_arguments\":{\"dsn\":\"$TEMP_DB\",\"sql\":\"INSERT INTO $TEMP_TABLE (value) VALUES ('governed-mutation')\"},
    \"expected_effect\":\"DatabaseMutation\",
    \"estimated_risk\":\"Low\",
    \"requested_rollback_class\":\"R1SnapshotRecoverable\",
    \"taint_inputs\":[],
    \"metadata\":{},
    \"created_at\":\"2026-05-30T00:00:00Z\"
  }"

# Mint capability, authorize, prepare, execute, verify (see Demo 1 skeleton)
# ... substitute sqlite tool bindings

# Verify DB state
echo "DB state after execute:"
sqlite3 "$TEMP_DB" "SELECT * FROM $TEMP_TABLE;"

# Compensate (restore savepoint)
curl -s -X POST "$FERRUM_GATEWAY/v1/executions/$EXECUTION_ID/compensate" \
  -H "Content-Type: application/json" \
  -d "{\"execution_id\":\"$EXECUTION_ID\",\"reason\":\"demo rollback\"}"

# Verify restored state
echo "DB state after rollback:"
sqlite3 "$TEMP_DB" "SELECT * FROM $TEMP_TABLE;"

# Cleanup
rm -f "$TEMP_DB"
```

### Expected Outcomes

- SQL insert executes via governed path
- After compensate, data is restored to pre-execute state
- Temp DB removed after cleanup

### Cleanup

```bash
rm -f /tmp/ferrum-demo-*.db
```

### Acceptance Checks

- [ ] Temp SQLite DB created with initial row
- [ ] Intent submitted with `intent_id` returned
- [ ] Proposal evaluated
- [ ] Execute succeeds; row count increases
- [ ] After compensate, row count returns to baseline
- [ ] Temp DB removed after cleanup

---

## Demo 4 — Approval-Required R3

### Purpose

Demonstrate the approval flow: submit an R3 (irreversible high-consequence) intent → evaluate returns `RequireApproval` → operator approves → execution continues → verify no auto-commit.

### Prerequisites

- ferrumd running locally (dev config, auth disabled)
- Policy bundle with R3 approval rule active (e.g., `r3-approval-required` from policy-authoring.md)
- Operator has access to list and approve intents

### Command Skeleton

```bash
FERRUM_GATEWAY="http://127.0.0.1:18080"

# Submit R3 intent (rollback_class = R3IrreversibleHighConsequence)
INTENT_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/intents/compile" \
  -H "Content-Type: application/json" \
  -d "{
    \"principal_id\":\"d228bf73-c4f5-467a-b47b-53110dca7270\",
    \"title\":\"demo-r3-irreversible\",
    \"goal\":\"perform irreversible high-consequence action\",
    \"agent_plan_summary\":\"irreversible action\",
    \"trusted_context\":{},
    \"raw_inputs\":[],
    \"requested_resource_scope\":[],
    \"metadata\":{}
  }")
INTENT_ID=$(echo "$INTENT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['envelope']['intent_id'])")

# Evaluate — expect RequireApproval for R3
PROPOSAL_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
EVAL_RESP=$(curl -s -X POST "$FERRUM_GATEWAY/v1/proposals/$PROPOSAL_ID/evaluate" \
  -H "Content-Type: application/json" \
  -d "{
    \"proposal_id\":\"$PROPOSAL_ID\",
    \"intent_id\":\"$INTENT_ID\",
    \"step_index\":1,
    \"title\":\"demo r3 action\",
    \"tool_name\":\"fs.delete\",
    \"server_name\":\"fs\",
    \"raw_arguments\":{\"path\":\"/tmp/important-file.txt\"},
    \"expected_effect\":\"FileMutation\",
    \"estimated_risk\":\"High\",
    \"requested_rollback_class\":\"R3IrreversibleHighConsequence\",
    \"taint_inputs\":[],
    \"metadata\":{},
    \"created_at\":\"2026-05-30T00:00:00Z\"
  }")
echo "$EVAL_RESP"
# Expected: decision: "RequireApproval"

# List pending approvals
curl -s "$FERRUM_GATEWAY/v1/approvals/list" \
  -H "Content-Type: application/json"

# Approve the pending proposal
# Use the approval_id from list response
APPROVAL_ID="<APPROVAL_ID>"
curl -s -X POST "$FERRUM_GATEWAY/v1/approvals/$APPROVAL_ID/approve" \
  -H "Content-Type: application/json" \
  -d "{\"approval_id\":\"$APPROVAL_ID\",\"actor\":\"operator\",\"note\":\"approved for demo\"}"

# Continue execution after approval (mint capability, authorize, prepare, execute, verify)
# ... see Demo 1 skeleton

# Verify no auto-commit occurred before approval
# The action should not have executed until approval was granted
```

### Expected Outcomes

- `/v1/proposals/{id}/evaluate` → decision `RequireApproval`
- `/v1/approvals/list` → shows approval with correct `proposal_id`
- After approve, execution lifecycle proceeds normally
- Action does NOT execute before approval is granted

### Cleanup

```bash
# Remove any temp files created
rm -f /tmp/important-file.txt
```

### Acceptance Checks

- [ ] R3 intent submit succeeds
- [ ] Evaluate returns `RequireApproval` (not `Allow`)
- [ ] Approval appears in list
- [ ] After approve, execution proceeds
- [ ] Action does not execute before approval

---

## Demo 5 — MCP Agent Flow

### Purpose

Demonstrate the full governance lifecycle through the MCP server: start ferrumd → start ferrum-mcp-server → tools/list → submit/evaluate/mint/authorize/prepare/execute/verify through MCP → query lineage.

### Prerequisites

- ferrumd running locally (dev config, auth disabled)
- ferrum-mcp-server binary built (`cargo build --bin ferrum-mcp-server --release`)
- MCP client configured (e.g., Claude Desktop with stdio transport)

### Command Skeleton

```bash
# Terminal 1 — Start ferrumd (dev config)
FERRUMD_BIND_ADDR=127.0.0.1:18080 ./target/release/ferrumd --config configs/ferrumgate.dev.toml

# Terminal 2 — Start ferrum-mcp-server
FERRUM_GATEWAY_URL=http://127.0.0.1:18080 \
FERRUM_GATEWAY_BEARER_TOKEN="placeholder-local-dev-token" \
./target/release/ferrum-mcp-server

# MCP client — call tools via JSON-RPC over stdio
# Example: tools/list
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}

# Example: ferrum_gate_submit_intent
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "ferrum_gate_submit_intent",
    "arguments": {
      "principal_id": "d228bf73-c4f5-467a-b47b-53110dca7270",
      "title": "demo-mcp-intent",
      "goal": "demonstrate MCP flow",
      "agent_plan_summary": "MCP governed write",
      "trusted_context": {},
      "raw_inputs": [],
      "requested_resource_scope": [{"kind": "FilesystemPath", "path": "/tmp/mcp-demo.txt", "mode": "Write"}],
      "metadata": {}
    }
  }
}

# Example: ferrum_gate_evaluate_intent
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "ferrum_gate_evaluate_intent",
    "arguments": {
      "proposal_id": "<PROPOSAL_ID>",
      "intent_id": "<INTENT_ID>",
      "step_index": 1,
      "title": "demo mcp proposal",
      "tool_name": "fs.write",
      "server_name": "fs",
      "raw_arguments": {"path": "/tmp/mcp-demo.txt", "content": "MCP governed write"},
      "expected_effect": "FileMutation",
      "estimated_risk": "Low",
      "requested_rollback_class": "R0NativeReversible",
      "taint_inputs": [],
      "metadata": {},
      "created_at": "2026-05-30T00:00:00Z"
    }
  }
}

# Continue: mint_capability → authorize_execution → prepare_execution → execute_prepared → verify → query_lineage
# (Use the MCP tool names: ferrum_gate_mint_capability, ferrum_gate_authorize_execution,
#  ferrum_gate_prepare_execution, ferrum_gate_execute_prepared, ferrum_gate_verify,
#  ferrum_gate_query_lineage)
```

### Expected Outcomes

- `ferrum_gate_health` → returns status
- Lifecycle tools return expected responses (capability minted, execution authorized, etc.)
- `ferrum_gate_query_lineage` → returns lineage JSON with events array
- Auth token passed via `FERRUM_GATEWAY_BEARER_TOKEN` env var

### Cleanup

```bash
rm -f /tmp/mcp-demo.txt
# Stop ferrumd and ferrum-mcp-server (Ctrl+C or kill)
```

### Acceptance Checks

- [ ] ferrum-mcp-server starts and connects to ferrumd
- [ ] `tools/list` returns all expected tools
- [ ] Submit intent via MCP succeeds
- [ ] Full lifecycle (evaluate → mint → authorize → prepare → execute → verify) completes via MCP
- [ ] Lineage query returns events array via MCP
- [ ] Temp file removed after cleanup

### Reference

See [`mcp-integration.md`](./mcp-integration.md) for full MCP tool reference and client configuration.

---

## Demo 6 — Policy Simulation

### Purpose

Demonstrate policy authoring, validation, simulation, and activation: write policy → validate → simulate against sample intent → see Allow/Deny/RequireApproval → apply and activate.

### Prerequisites

- ferrumd running locally (dev config, auth disabled)
- ferrumctl available (`cargo build --release` or from `$PATH`)
- Policy bundle YAML file ready (use templates from [`policy-authoring.md`](./policy-authoring.md))

### Command Skeleton

```bash
FERRUMCTL="ferrumctl"  # or ./target/release/ferrumctl
FERRUM_GATEWAY="http://127.0.0.1:18080"
POLICY_FILE="/tmp/demo-policy.yaml"

# Step 1 — Write a policy bundle (using r3-approval-required template)
cat > "$POLICY_FILE" << 'EOF'
version: "1.0.0"
bundle_id: demo-r3-approval
rules:
  - id: r3-approval
    description: "Require approval for R3 irreversible actions"
    decision: RequireApproval
    priority: 100
    matchers:
      - type: rollback_class_equals
        value: "R3IrreversibleHighConsequence"
EOF

# Step 2 — Validate locally (no server required)
$FERRUMCTL policy validate --file "$POLICY_FILE"

# Step 3 — Simulate against a sample proposal (requires running server)
# Create a sample proposal JSON
PROPOSAL_JSON=$(mktemp)
cat > "$PROPOSAL_JSON" << 'EOF'
{
  "proposal_id": "demo-proposal-001",
  "intent_id": "demo-intent-001",
  "step_index": 1,
  "title": "demo r3 proposal",
  "tool_name": "fs.delete",
  "server_name": "fs",
  "raw_arguments": {"path": "/tmp/important.txt"},
  "expected_effect": "FileMutation",
  "estimated_risk": "High",
  "requested_rollback_class": "R3IrreversibleHighConsequence",
  "taint_inputs": [],
  "metadata": {},
  "created_at": "2026-05-30T00:00:00Z"
}
EOF

$FERRUMCTL policy simulate --file "$POLICY_FILE" --proposal "$PROPOSAL_JSON"

# Step 4 — Apply as inactive bundle (requires running server)
$FERRUMCTL policy apply --file "$POLICY_FILE" --server-url "$FERRUM_GATEWAY"

# Step 5 — List versions
$FERRUMCTL policy versions --bundle-id demo-r3-approval --server-url "$FERRUM_GATEWAY"

# Step 6 — Activate
$FERRUMCTL policy apply --file "$POLICY_FILE" --activate --server-url "$FERRUM_GATEWAY"

# Step 7 — Runtime simulation: evaluate live intent against active runtime policy
curl -s -X POST "$FERRUM_GATEWAY/v1/policy/runtime-simulate" \
  -H "Content-Type: application/json" \
  -d "{
    \"proposal_id\":\"demo-proposal-001\",
    \"intent_id\":\"demo-intent-001\",
    \"step_index\":1,
    \"title\":\"demo r3 proposal\",
    \"tool_name\":\"fs.delete\",
    \"server_name\":\"fs\",
    \"raw_arguments\":{\"path\":\"/tmp/important.txt\"},
    \"expected_effect\":\"FileMutation\",
    \"estimated_risk\":\"High\",
    \"requested_rollback_class\":\"R3IrreversibleHighConsequence\",
    \"taint_inputs\":[],
    \"metadata\":{},
    \"created_at\":\"2026-05-30T00:00:00Z\"
  }"

# Step 8 — Rollback to previous version if needed
$FERRUMCTL policy rollback --bundle-id demo-r3-approval --target-version 1 --actor operator --server-url "$FERRUM_GATEWAY"

# Cleanup
rm -f "$POLICY_FILE" "$PROPOSAL_JSON"
```

### Expected Outcomes

- `ferrumctl policy validate` → validates without errors
- `ferrumctl policy simulate` → shows decision for the proposal (expect `RequireApproval` for R3 class)
- `ferrumctl policy apply` → bundle applied as inactive
- `ferrumctl policy apply --activate` → bundle active
- Runtime simulation returns the expected decision
- `ferrumctl policy rollback` → reverts to prior version

### Cleanup

```bash
rm -f /tmp/demo-policy.yaml /tmp/demo-proposal.json
```

### Acceptance Checks

- [ ] Policy validates without errors
- [ ] Simulate shows `RequireApproval` decision for R3-class proposal
- [ ] Bundle applied as inactive
- [ ] Bundle activated
- [ ] Runtime simulation returns expected decision
- [ ] Rollback succeeds

### Reference

See [`policy-authoring.md`](./policy-authoring.md) for full policy schema, matcher reference, and all CLI commands.

---

## Docker Compose Alternatives

For demos 1–4, you may substitute the binary-based ferrumd start with Docker Compose:

```bash
# Start
docker compose -f docker-compose.demo.yml up -d
# Use FERRUM_GATEWAY="http://127.0.0.1:8080"

# PostgreSQL variant (for Demo 3 with persistent store)
docker compose -f docker-compose.postgres-demo.yml up -d
# Use FERRUM_GATEWAY="http://127.0.0.1:19081"

# Stop and clean
docker compose -f docker-compose.demo.yml down
docker compose -f docker-compose.postgres-demo.yml down -v
```

> **Validated evidence**: `docker-compose.demo.yml` and `docker-compose.postgres-demo.yml` were locally validated on 2026-05-19.

---

## Related Docs

- [`quickstart.md`](./quickstart.md) — Local API/curl quickstart with validated endpoint sequence
- [`policy-authoring.md`](./policy-authoring.md) — Policy schema, templates, validate/simulate/apply/diff/rollback/versions
- [`mcp-integration.md`](./mcp-integration.md) — MCP server setup, tools reference, client config
- [`operator.md`](./operator.md) — Config, health, backup/restore, monitoring
- [`docker-compose.demo.yml`](../../docker-compose.demo.yml) — Local SQLite demo compose
- [`docker-compose.postgres-demo.yml`](../../docker-compose.postgres-demo.yml) — Local PostgreSQL demo compose
