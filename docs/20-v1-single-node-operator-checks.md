# 20 — v1 Single-Node Operator Checks

CLI-first operator guide for FerrumGate v1 single-node operators.
Covers health ladders, routine checks, safe command catalog, and execution control.

**Scope**: single-node, SQLite-backed, v1 only.
**Audience**: operators, on-call engineers, SREs.
**Last updated**: 2026-04-02.

---

## 1. Boundary Note

This document covers operator verification and control of a running FerrumGate
v1 single-node instance. For support scope, limits, and known caveats, see:

[19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

For deployment, startup, backup, restore, and recovery procedures, see:

[18-single-node-operations-runbook.md](./18-single-node-operations-runbook.md)

For observability signals, log watchpoints, and alert thresholds, see:

[21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md)

---

## 2. Check Modes Matrix

Canonical reference: which operator actions have CLI support, and what the
HTTP fallback is.

| Check | CLI command | HTTP fallback | Notes |
|---|---|---|---|
| Shallow process health | `ferrumctl server health` | `GET /v1/healthz` | No auth required |
| Shallow readiness | `ferrumctl server ready` | `GET /v1/readyz` | No auth required; shallow only |
| Minimum functional readiness probe | `ferrumctl server inspect-approvals` | `GET /v1/approvals?limit=1` | Required after startup; requires bearer auth when `auth_mode=bearer` |
| Inspect execution record | `ferrumctl server inspect-execution <id>` | `GET /v1/executions/<id>` | Requires bearer auth |
| List approvals | `ferrumctl server inspect-approvals` | `GET /v1/approvals` | Requires bearer auth |
| Inspect single approval | `ferrumctl server inspect-approval <id>` | `GET /v1/approvals/<id>` | Requires bearer auth |
| Approvals pagination/filter | `ferrumctl server inspect-approvals --limit N --proposal-id X` | `GET /v1/approvals?limit=N&proposal_id=X` | CLI supports `--limit`, `--cursor`, `--proposal-id`, `--execution-id` |
| Inspect capability | `ferrumctl server inspect-capability <id>` | `GET /v1/capabilities/<id>` | Requires bearer auth |
| Revoke capability | `ferrumctl server revoke-capability <id>` | `POST /v1/capabilities/<id>/revoke` | Mutating; requires bearer auth |
| Resolve approval | `ferrumctl server resolve-approval <id> --approve\|--deny` | `POST /v1/approvals/<id>/resolve` | Mutating; requires bearer auth |
| Fetch lineage for execution | `ferrumctl server inspect-lineage <exec_id>` | `GET /v1/provenance/lineage/<exec_id>` | Requires bearer auth |
| Provenance event query | `ferrumctl server inspect-provenance` | `POST /v1/provenance/query` | CLI supports multiple filters, pagination, and `--all-pages` |
| Cancel execution | `ferrumctl server cancel-execution <id>` | `POST /v1/executions/<id>/cancel` | Mutating; pre-execute states only |
| Pause execution | `ferrumctl server pause-execution <id>` | `POST /v1/executions/<id>/pause` | Mutating; running states only |
| Resume execution | `ferrumctl server resume-execution <id>` | `POST /v1/executions/<id>/resume` | Mutating; paused state only |
| Prepare execution | `ferrumctl server prepare-execution <id>` | `POST /v1/executions/<id>/prepare` | Mutating; non-terminal states |
| Execute execution | `ferrumctl server execute-execution <id>` | `POST /v1/executions/<id>/execute` | Mutating; prepared state only |
| Compensate execution | `ferrumctl server compensate-execution <id>` | `POST /v1/executions/<id>/compensate` | Mutating; may be noop |
| Rollback execution | `ferrumctl server rollback-execution <id>` | `POST /v1/executions/<id>/rollback` | Mutating; terminal-state guarded |
| Watch execution terminal state | `ferrumctl server watch-execution <id>` | — | Bounded polling; read-only |
| Watch approvals | `ferrumctl server watch-approvals` | — | Bounded polling; read-only |
| Multi-hop lineage query | `ferrumctl server inspect-lineage-query` | `POST /v1/provenance/lineage` | Read-only; --ancestry/--descendants |

**Important**: healthz and readyz are shallow. They confirm the HTTP endpoint
is reachable and the process is alive. Neither validates the store, migrations,
or governance loop. A functional probe is required after startup (see Section 3).

---

## 3. Startup Health Verification Ladder

Perform these checks after starting or restarting a FerrumGate v1 single-node
instance. Run each step in order.

### Step 1 — Shallow health (no auth required)

```bash
curl http://127.0.0.1:8080/v1/healthz
# Expected: 200 OK, {"status":"ok"}
```

### Step 2 — Shallow readiness (no auth required)

```bash
ferrumctl server ready

curl http://127.0.0.1:8080/v1/readyz
# Expected: 200 OK, {"status":"ready"}
```

### Step 3 — Required functional probe (`auth_mode=bearer`)

Set up the environment:

```bash
export FERRUMCTL_SERVER_URL=http://127.0.0.1:8080
export FERRUMCTL_BEARER_TOKEN="$FERRUM_BEARER_TOKEN"
```

Then run at least one functional probe:

```bash
ferrumctl server inspect-approvals
# Returns approvals data (possibly empty) through the authenticated API

curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
# Expected: 200 OK, {"items":[...]} or {"items":[]} (empty list is normal)
```

Do not treat `ferrumctl server health` as a functional probe. It only checks the
shallow health endpoint.

### Step 4 — Required functional probe (`auth_mode=disabled`)

```bash
curl http://127.0.0.1:8080/v1/approvals?limit=1
# Expected: 200 OK, {"items":[...]} or {"items":[]} (empty list is normal)
```

A successful response (200 with valid JSON) confirms the store, auth, and
governance loop are functional. This is the minimum end-to-end readiness check.

### Step 5 — Verify a known execution record (optional)

If you have a known execution_id from prior operations:

```bash
ferrumctl server inspect-execution 00000000-0000-0000-0000-000000000001
# Returns 404 for unknown IDs (expected); transport error indicates connectivity issue
```

---

## 4. Routine Checks

### 4.1 Per restart

Run the full ladder in Section 3. Do not rely on healthz/readyz alone.

### 4.2 Daily / shift change

Run a functional probe and spot-check approvals:

```bash
# Functional probe — approvals reachable with auth
curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"

# Inspect pending approvals via CLI
ferrumctl server inspect-approvals

# Check a specific approval if IDs are known
ferrumctl server inspect-approval <approval_id>
```

### 4.3 Incident triage

When investigating an issue, collect the following in order:

1. Shallow health/ready:
   ```bash
   curl http://127.0.0.1:8080/v1/healthz
   curl http://127.0.0.1:8080/v1/readyz
   ```

2. Execution record for the affected execution_id:
   ```bash
   ferrumctl server inspect-execution <execution_id>
   ```

3. Lineage for the execution:
   ```bash
   ferrumctl server inspect-lineage <execution_id>
   ```

4. Approval state if the execution involved an approval:
   ```bash
   ferrumctl server inspect-approval <approval_id>
   ```

5. Provenance events for the affected resource scope:
   ```bash
   ferrumctl server inspect-provenance --intent-id <intent_id>
   ferrumctl server inspect-provenance --execution-id <execution_id>
   ```
   Note: CLI supports richer filtering (`--proposal-id`, `--execution-id`,
   `--capability-id`, `--event-kind`, `--since`, `--until`, `--limit`,
   `--cursor`, `--all-pages`). Use the HTTP POST endpoint directly only when
   you prefer raw request payloads.

---

## 5. Safe Command Catalog (Read-Only)

All commands in this section are read-only. They do not mutate state.

### ferrumctl server health

Shallow health check via CLI.

```bash
ferrumctl server health
# Or with explicit server URL and token:
ferrumctl --server-url http://127.0.0.1:8080 --bearer-token "$FERRUM_BEARER_TOKEN" server health
```

**What it checks**: GET /v1/healthz endpoint, confirms JSON parse.
**What it does not check**: store, migrations, governance loop.
**Auth**: None required (healthz is unauthenticated in the gateway).

---

### ferrumctl server ready

Shallow readiness check via CLI.

```bash
ferrumctl server ready
# Or with explicit server URL and token:
ferrumctl --server-url http://127.0.0.1:8080 --bearer-token "$FERRUM_BEARER_TOKEN" server ready
```

**What it checks**: GET /v1/readyz endpoint, confirms JSON parse.
**What it does not check**: store, migrations, governance loop.
**Auth**: None required (readyz is unauthenticated in the gateway).

---

### ferrumctl server inspect-execution \<execution_id\>

Fetch the execution record for a specific execution.

```bash
ferrumctl server inspect-execution 00000000-0000-0000-0000-000000000001
```

**What it checks**: GET /v1/executions/{execution_id}
**Auth**: Required (bearer). Unauthenticated requests return 401.

---

### ferrumctl server inspect-approvals

List approvals, with optional pagination and filtering.

```bash
ferrumctl server inspect-approvals

# Paginate
ferrumctl server inspect-approvals --limit 10
ferrumctl server inspect-approvals --limit 10 --cursor <cursor>

# Filter
ferrumctl server inspect-approvals --proposal-id <proposal_id>
ferrumctl server inspect-approvals --execution-id <execution_id>
```

**What it checks**: GET /v1/approvals
**Auth**: Required (bearer).
**Pagination/filtering**: CLI supports `--limit`, `--cursor`, `--proposal-id`, and
`--execution-id`. Use the HTTP endpoint directly only if you prefer raw JSON/API access.

---

### ferrumctl server inspect-approval \<approval_id\>

Fetch a single approval by ID.

```bash
ferrumctl server inspect-approval <approval_id>
```

**What it checks**: GET /v1/approvals/{approval_id}
**Auth**: Required (bearer).

---

### ferrumctl server inspect-lineage \<execution_id\>

Fetch the lineage (event chain) for an execution.

```bash
ferrumctl server inspect-lineage <execution_id>
ferrumctl server inspect-lineage <execution_id> --format json
ferrumctl server inspect-lineage <execution_id> --format dot --output lineage.dot
```

**What it checks**: GET /v1/provenance/lineage/{execution_id}
**Auth**: Required (bearer).
**Output formats**: text (default), json, dot.

---

### ferrumctl server inspect-provenance

Query provenance events with CLI filters and pagination.

```bash
ferrumctl server inspect-provenance --intent-id <intent_id>

# Filter by execution, proposal, capability, or event kind
ferrumctl server inspect-provenance --execution-id <execution_id>
ferrumctl server inspect-provenance --proposal-id <proposal_id> --event-kind IntentCompiled

# Time window and pagination
ferrumctl server inspect-provenance --since 2026-01-01T00:00:00Z --until 2026-12-31T23:59:59Z --limit 100
ferrumctl server inspect-provenance --limit 100 --cursor <cursor>

# Export all pages as JSON
ferrumctl server inspect-provenance --execution-id <execution_id> --all-pages
```

**What it checks**: POST /v1/provenance/query.
**Auth**: Required (bearer).

**CLI filters**: `--intent-id`, `--proposal-id`, `--execution-id`,
`--execution-ids`, `--capability-id`, `--event-kind`, `--terminal-only`,
`--since`, `--until`, `--limit`, `--cursor`, and `--all-pages`.

Use the HTTP endpoint directly only if you prefer raw request payloads or need
to work outside `ferrumctl`:

```bash
curl -X POST http://127.0.0.1:8080/v1/provenance/query \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "intent_id": "<intent_id>",
    "execution_id": "<execution_id>",
    "capability_id": "<capability_id>",
    "event_kind": "IntentCompiled",
    "since": "2026-01-01T00:00:00Z",
    "until": "2026-12-31T23:59:59Z"
  }'
```

---

### ferrumctl server inspect-capability \<capability_id\>

Fetch a capability record by ID.

```bash
ferrumctl server inspect-capability <capability_id>
ferrumctl server inspect-capability <capability_id> --json
```

**What it does**: GET /v1/capabilities/{capability_id}.
**Auth**: Required (bearer).

---

### ferrumctl server watch-execution \<execution_id\>

Watch an execution by polling until it reaches a terminal state.

```bash
# Watch with default 2000ms interval, exit 0 after 60 iterations
ferrumctl server watch-execution <execution_id>

# Poll every 5s, exit non-zero if terminal state not reached after 10 iterations
ferrumctl server watch-execution <execution_id> --poll-interval-ms 5000 --iterations 10 --require-terminal

# JSON output per iteration
ferrumctl server watch-execution <execution_id> --json
```

**What it does**: Polls `GET /v1/executions/{execution_id}` at the specified
interval and prints state transitions. Bounded by `--iterations` (default 60);
use `--require-terminal` to exit non-zero if the cap is reached without a
terminal state.
**Auth**: Required (bearer).

---

### ferrumctl server watch-approvals

Watch pending approvals by polling the approvals list endpoint at a fixed interval.

```bash
# Watch all pending approvals (default 5000ms interval, 1 iteration)
ferrumctl server watch-approvals

# Filter by proposal and poll every 10s for up to 5 iterations
ferrumctl server watch-approvals --proposal-id <proposal_id> --poll-interval-ms 10000 --iterations 5

# Single-shot watch with JSON output
ferrumctl server watch-approvals --iterations 1 --json
```

**What it does**: Polls `GET /v1/approvals` at the specified interval and prints
human-readable summaries (or raw JSON with `--json`). Bounded by `--iterations`
(default 1); use `--iterations 0` for unlimited (Ctrl+C to stop).
**Auth**: Required (bearer).

---

### ferrumctl server inspect-lineage-query \<execution_id\> --event-id \<event_id\>

Multi-hop lineage traversal from a seed event via ancestry and/or descendant edges.

```bash
# Walk ancestry backwards from a seed event (8 hops default)
ferrumctl server inspect-lineage-query <execution_id> --event-id <event_id> --ancestry

# Walk descendants forwards
ferrumctl server inspect-lineage-query <execution_id> --event-id <event_id> --descendants

# Both directions, max 16 hops
ferrumctl server inspect-lineage-query <execution_id> --event-id <event_id> --ancestry --descendants --max-hops 16

# JSON output
ferrumctl server inspect-lineage-query <execution_id> --event-id <event_id> --ancestry --json

# Filter to specific edge types
ferrumctl server inspect-lineage-query <execution_id> --event-id <event_id> --ancestry --edge-type AuthorizedBy --edge-type TaintedBy
```

**What it does**: `POST /v1/provenance/lineage` with BFS traversal. `--ancestry`
walks parent edges backwards; `--descendants` walks child edges forwards.
`--edge-type` filters to specific edge kinds. `--max-hops` bounds traversal depth
(1-32, server-hard-capped at 32).
**Auth**: Required (bearer).

---

## 6. Raw HTTP Reference

These examples show the raw HTTP form of common operator checks. Use them when
you prefer curl or need to debug behavior below the CLI layer.

### GET /v1/readyz

Shallow readiness check. No auth required.

```bash
curl http://127.0.0.1:8080/v1/readyz
```

**Note**: This is shallow. Always follow with a functional probe (approvals
endpoint or inspect-execution) to confirm end-to-end readiness.

---

### GET /v1/approvals with pagination and filtering

```bash
# Get first 10 approvals
curl "http://127.0.0.1:8080/v1/approvals?limit=10" \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"

# Filter by proposal_id
curl "http://127.0.0.1:8080/v1/approvals?limit=10&proposal_id=<proposal_id>" \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

**Note**: Omit `-H "Authorization: Bearer ..."` when auth_mode=disabled.

---

## 7. Operator Control Commands

Commands in this section are mutating (they invoke gateway endpoints that
change execution or approval state). Each wraps the corresponding REST call.

### ferrumctl server resolve-approval \<approval_id\> (--approve | --deny)

Resolve a pending approval (approve or deny).

```bash
# Approve a pending approval
ferrumctl server resolve-approval <approval_id> --approve

# Deny with reason
ferrumctl server resolve-approval <approval_id> --deny --reason "security policy violation"

# Specify actor
ferrumctl server resolve-approval <approval_id> --approve --actor-type operator --actor-id "oncall-1"

# JSON output
ferrumctl server resolve-approval <approval_id> --approve --json
```

**What it does**: `POST /v1/approvals/{approval_id}/resolve` with the specified
decision. `--approve` or `--deny` is required. `--reason` is required when
denying. Actor fields default to `operator` / `ferrumctl`.
**Auth**: Required (bearer).

---

### ferrumctl server revoke-capability \<capability_id\>

Revoke a capability, marking it as used.

```bash
ferrumctl server revoke-capability <capability_id>
ferrumctl server revoke-capability <capability_id> --json
```

**What it does**: `POST /v1/capabilities/{capability_id}/revoke`. Revocation is
permanent; the capability cannot be reused after revocation.
**Auth**: Required (bearer).

---

### ferrumctl server cancel-execution \<execution_id\>

Cancel an execution in a pre-execute state (Proposed, Authorized, Prepared).

```bash
ferrumctl server cancel-execution <execution_id>
ferrumctl server cancel-execution <execution_id> --json
```

**What it does**: `POST /v1/executions/{execution_id}/cancel`. Only valid for
pre-execute states; returns an error if the execution is already Running,
AwaitingVerification, Completed, Failed, or RolledBack.
**Auth**: Required (bearer).

---

### ferrumctl server pause-execution \<execution_id\>

Pause an execution in a running state (Running, AwaitingVerification).

```bash
ferrumctl server pause-execution <execution_id>
ferrumctl server pause-execution <execution_id> --json
```

**What it does**: `POST /v1/executions/{execution_id}/pause`. Only valid for
running states; returns an error for non-running states.
**Auth**: Required (bearer).

---

### ferrumctl server resume-execution \<execution_id\>

Resume a paused execution.

```bash
ferrumctl server resume-execution <execution_id>
ferrumctl server resume-execution <execution_id> --json
```

**What it does**: `POST /v1/executions/{execution_id}/resume`. Only valid when
the execution is in Paused state; returns an error for other states.
**Auth**: Required (bearer).

---

### ferrumctl server prepare-execution \<execution_id\>

Prepare an execution (transition from Authorized or Proposed to Prepared).

```bash
ferrumctl server prepare-execution <execution_id>
ferrumctl server prepare-execution <execution_id> --json
```

**What it does**: `POST /v1/executions/{execution_id}/prepare`. Valid for
non-terminal states (Proposed, Authorized, Prepared, Running,
AwaitingVerification). Returns an error for terminal states.
**Auth**: Required (bearer).

---

### ferrumctl server execute-execution \<execution_id\> --payload '\<json\>'

Execute a prepared execution (transition from Prepared to Running).

```bash
# Pass a JSON payload to the execution adapter
ferrumctl server execute-execution <execution_id> --payload '{"path":"/tmp/test.txt","content":"hello"}'

# JSON output
ferrumctl server execute-execution <execution_id> --payload '{}' --json
```

**What it does**: `POST /v1/executions/{execution_id}/execute`. Only valid when
the execution is in Prepared state. The `--payload` flag accepts a JSON object
that is forwarded to the execution adapter.
**Auth**: Required (bearer).

---

### ferrumctl server compensate-execution \<execution_id\>

Request compensation (undo) on an execution. May be a noop depending on
adapter and rollback class.

```bash
ferrumctl server compensate-execution <execution_id>
ferrumctl server compensate-execution <execution_id> --json
```

**What it does**: `POST /v1/executions/{execution_id}/compensate`. Compensation
may not perform actual external undo depending on adapter implementation and
rollback class (R0/R1/R2/R3). Always verify resource state manually after
compensate.
**Auth**: Required (bearer).

---

### ferrumctl server rollback-execution \<execution_id\>

Trigger rollback on an execution via the rollback contract.

```bash
ferrumctl server rollback-execution <execution_id>
ferrumctl server rollback-execution <execution_id> --json
```

**What it does**: `POST /v1/executions/{execution_id}/rollback`. Guarded by
terminal-state check; returns an error if the execution is already in a
terminal state. Uses the per-mutation rollback contract (R0/R1/R2/R3).
**Auth**: Required (bearer).

---

## 8. Do-Not-Use / Out-of-Scope

The following are not covered in this document because they are either
post-v1 or not operator-facing:

| Command / Route | Reason not covered |
|---|---|
| `POST /v1/executions/{id}/commit` | Operator-facing use is rare in single-node; compensate/rollback are the primary recovery paths |
| `ferrumctl intent create`, `ferrumctl capability mint`, etc. | Intent/capability creation CLI is not in the v1 operator surface; `inspect-capability` and `revoke-capability` are covered above |
| `ferrumctl server replay`, `inspect-provenance-stats`, `export-provenance`, `inspect-event`, `resolve-approval-bulk`, `verify-ledger`, `ingest-external-event` | Available for specialized audit/integration workflows but not part of the day-1 operator surface in this guide |
| Adapter-backed undo (fs, sqlite, maildraft, git, http) | Skeleton implementations; no production-verified side effects in v1 |
| Multi-node, HA, read-replica configurations | Out of scope for v1 single-node |

---

## 9. Auth Header Reference

When auth_mode=bearer, most CLI commands and HTTP requests require the
Authorization header. The gateway skips auth for /v1/healthz and /v1/readyz.

### CLI (ferrumctl)

Set via environment variable or flag:

```bash
export FERRUMCTL_BEARER_TOKEN="$FERRUM_BEARER_TOKEN"
ferrumctl server inspect-approvals
```

Or inline:

```bash
ferrumctl --bearer-token "$FERRUM_BEARER_TOKEN" server inspect-approval <id>
```

### HTTP (curl)

Always include for protected endpoints:

```bash
curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
```

Omit the `-H` flag when auth_mode=disabled. In that mode the server accepts
all requests without credentials, but binding to non-loopback addresses is
blocked at startup unless allow_insecure_nonlocal_bind=true.

---

## 10. References

- Support contract (scope, limits, accepted risks):
  [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
- Operations runbook (deployment, backup, restore):
  [18-single-node-operations-runbook.md](./18-single-node-operations-runbook.md)
- Observability minimums (logs, probes, thresholds):
  [21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md)
- Configuration reference:
  [15-deployment-and-operations.md](./15-deployment-and-operations.md)
- Troubleshooting:
  [17-troubleshooting.md](./17-troubleshooting.md)
- API endpoint reference:
  [14-api-and-contracts-map.md](./14-api-and-contracts-map.md)
