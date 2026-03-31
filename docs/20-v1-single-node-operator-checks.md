# 20 — v1 Single-Node Operator Checks

CLI-first, read-only verification guide for FerrumGate v1 single-node operators.
Covers health ladders, routine checks, and safe command catalog.

**Scope**: single-node, SQLite-backed, v1 only.
**Audience**: operators, on-call engineers, SREs.
**Last updated**: 2026-03-30.

---

## 1. Boundary Note

This document covers read-only operator verification of a running FerrumGate
v1 single-node instance. For support scope, limits, and known caveats, see:

[19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

---

## 2. Check Modes Matrix

| Check | CLI command | HTTP fallback | Notes |
|---|---|---|---|
| Shallow process health | `ferrumctl server health` | `GET /v1/healthz` | No auth required |
| Shallow readiness | — | `GET /v1/readyz` | No auth required; shallow only |
| Inspect execution record | `ferrumctl server inspect-execution <id>` | `GET /v1/executions/<id>` | Requires bearer auth |
| List approvals (unpaginated) | `ferrumctl server inspect-approvals` | `GET /v1/approvals` | Requires bearer auth |
| Inspect single approval | `ferrumctl server inspect-approval <id>` | `GET /v1/approvals/<id>` | Requires bearer auth |
| Approvals pagination/filter | — | `GET /v1/approvals?limit=N&proposal_id=X` | HTTP-only; requires bearer auth |
| Resolve approval | `ferrumctl server resolve-approval <id> --approve|--deny` | `POST /v1/approvals/<id>/resolve` | Mutating; requires bearer auth |
| Fetch lineage for execution | `ferrumctl server inspect-lineage <exec_id>` | `GET /v1/provenance/lineage/<exec_id>` | Requires bearer auth |
| Provenance event query | `ferrumctl server inspect-provenance` | `POST /v1/provenance/query` | CLI form is intent-id-only today; HTTP form supports richer filters |
| Cancel execution | `ferrumctl server cancel-execution <id>` | `POST /v1/executions/<id>/cancel` | Mutating; pre-execute states only |
| Pause execution | `ferrumctl server pause-execution <id>` | `POST /v1/executions/<id>/pause` | Mutating; running states only |
| Resume execution | `ferrumctl server resume-execution <id>` | `POST /v1/executions/<id>/resume` | Mutating; paused state only |
| Prepare execution | `ferrumctl server prepare-execution <id>` | `POST /v1/executions/<id>/prepare` | Mutating; non-terminal states (Proposed, Authorized, Prepared, Running, AwaitingVerification) |
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
curl http://127.0.0.1:8080/v1/readyz
# Expected: 200 OK, {"status":"ready"}
```

### Step 3 — Functional probe via CLI (requires bearer auth)

Set up the environment:

```bash
export FERRUMCTL_SERVER_URL=http://127.0.0.1:8080
export FERRUMCTL_BEARER_TOKEN="$FERRUM_BEARER_TOKEN"
```

Then run the functional probe:

```bash
ferrumctl server health
# Fetches GET /v1/healthz via CLI; confirms connectivity and JSON parse
```

If the CLI health succeeds, the HTTP endpoint is reachable and responding.
If it fails, check the ferrumd logs for startup errors.

### Step 4 — Verify approvals reachable (functional probe)

```bash
# HTTP functional probe — returns empty list or live approvals
curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
# Expected: 200 OK, {"items":[...]} or {"items":[]} (empty list is normal)
# Note: omit -H flag when auth_mode=disabled
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

5. Provenance events for the intent (if intent_id is known):
   ```bash
   ferrumctl server inspect-provenance --intent-id <intent_id>
   ```
   Note: `inspect-provenance` via CLI accepts `--intent-id` only. For richer
   filtering (by execution_id, capability_id, event_kind, time range), use
   the HTTP POST endpoint directly.

---

## 5. Safe Command Catalog

All commands below are read-only. They do not mutate state.

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

### ferrumctl server inspect-execution \<execution_id\>

Fetch the execution record for a specific execution.

```bash
ferrumctl server inspect-execution 00000000-0000-0000-0000-000000000001
```

**What it checks**: GET /v1/executions/{execution_id}
**Auth**: Required (bearer). Unauthenticated requests return 401.

---

### ferrumctl server inspect-approvals

List all approvals. No pagination or filtering via CLI.

```bash
ferrumctl server inspect-approvals
```

**What it checks**: GET /v1/approvals (returns all items, no limit/filter params)
**Auth**: Required (bearer).
**Pagination/filtering**: HTTP-only. Use `GET /v1/approvals?limit=N&proposal_id=X`
directly if you need paginated or filtered results.

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

### ferrumctl server inspect-provenance (intent-id-only via CLI)

Query provenance events filtered by intent_id.

```bash
ferrumctl server inspect-provenance --intent-id <intent_id>
```

**What it checks**: POST /v1/provenance/query with intent_id filter.
**Auth**: Required (bearer).

**Current CLI limitation**: The CLI form only supports `--intent-id`. For other
filters (execution_id, capability_id, event_kind, time range), use the HTTP
endpoint directly:

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

## 6. HTTP-Only Fallback Reference

These endpoints are not exposed via the CLI in v1. Use curl or any HTTP client.

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

## 7. Do-Not-Use / Out-of-Scope

The following are not covered in this document because they are either
post-v1 or not operator-facing:

| Command / Route | Reason not covered |
|---|---|
| `POST /v1/executions/{id}/commit` | Operator-facing use is rare in single-node; compensate/rollback are the primary recovery paths |
| `ferrumctl intent create`, `ferrumctl capability mint`, etc. | Intent/capability creation CLI is not in the v1 operator surface; `inspect-capability` and `revoke-capability` are covered above |
| Adapter-backed undo (fs, sqlite, maildraft, git, http) | Skeleton implementations; no production-verified side effects in v1 |
| Multi-node, HA, read-replica configurations | Out of scope for v1 single-node |

---

## 8. Auth Header Reference

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

## 9. References

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
