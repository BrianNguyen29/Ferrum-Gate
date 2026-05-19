# Quickstart Local Validation Evidence — 2026-05-19 (Extended)

## Status

- **Scope**: DOC-1 / DOC-2 local quickstart validation — extended API/curl flow.
- **Verdict**: ✅ PASS for local API/curl flow through lineage endpoint.
- **Production-ready**: NO.
- **Full quickstart end-to-end**: PARTIAL — curl/API path validated; ferrumctl and MCP remain scaffold.
- **Fresh-user test**: NOT PERFORMED.
- **Target-host / cloud**: NOT CLAIMED.
- **Block A**: WAIVED/CONDITIONAL — no real owned domain or DNS available.

This artifact records a local validation run of the FerrumGate quickstart curl sequence against `target/release/ferrumd` with in-memory SQLite, disabled authentication, and loopback-only binding.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Local development workstation |
| Binary | `./target/release/ferrumd` |
| Config | `configs/ferrumgate.dev.toml` |
| Bind address | `127.0.0.1:18080` (via `FERRUMD_BIND_ADDR`) |
| Store DSN | `sqlite::memory:` (in-memory) |
| Auth mode | `disabled` |

## Validated endpoint sequence

| Step | Endpoint | Method | Expected status |
|------|----------|--------|-----------------|
| 1 | `/v1/healthz` | `GET` | HTTP 200 `{"status":"ok"}` |
| 2 | `/v1/intents/compile` | `POST` | HTTP 200 with sanitized `intent_id` |
| 3 | `/v1/proposals/{proposal_id}/evaluate` | `POST` | HTTP 200 decision `Allow` |
| 4 | `/v1/capabilities/mint` | `POST` | HTTP 200 with lease |
| 5 | `/v1/executions/authorize` | `POST` | HTTP 200 |
| 6 | `/v1/executions/{execution_id}/prepare` | `POST` | HTTP 200 `prepared: true` |
| 7 | `/v1/executions/{execution_id}/execute` | `POST` | HTTP 200 `executed: true` |
| 8 | `/v1/executions/{execution_id}/verify` | `POST` | HTTP 200 `verified: true` |
| 9 | `/v1/executions/{execution_id}/evaluate-outcome` | `POST` | HTTP 200 `aligned: true` |
| 10 | `/v1/provenance/lineage/{execution_id}` | `GET` | HTTP 200 with `events` and `execution_id` |

## Docs correction discovered during validation

### Initial attempt — HTTP 403 on authorize

The first extended validation attempt used an empty `requested_resource_scope` in the compile request:

```json
{"requested_resource_scope": []}
```

This caused step 5 (authorize) to return:

- HTTP status: `403`
- Decision: `PolicyDenied`
- Message: `resource scope is empty but capability has resource bindings`

### Corrected request

The compile request must include a `requested_resource_scope` that matches the capability's resource bindings:

```json
{"requested_resource_scope": [{"kind":"FilesystemPath","path":"/tmp/ferrum-demo-extended.txt","mode":"Write"}]}
```

After this correction, the full flow validated successfully.

## Corrected extended validation

### Start server

Command:

```bash
FERRUMD_BIND_ADDR=127.0.0.1:18080 ./target/release/ferrumd --config configs/ferrumgate.dev.toml
```

Observed log lines (sanitized):

```text
auth_mode=disabled
store_dsn=sqlite::memory:
listening on 127.0.0.1:18080
```

### Step 1 — Health check

Command:

```bash
curl -s -w "\n%{http_code}\n" http://127.0.0.1:18080/v1/healthz
```

Observed result:

- HTTP status: `200`
- Response body: `{"status":"ok"}`

Result: ✅ PASS.

### Step 2 — Compile intent

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/intents/compile \
  -H "Content-Type: application/json" \
  -d '{"principal_id":"d228bf73-c4f5-467a-b47b-53110dca7270","title":"demo-write","goal":"write a demo file","agent_plan_summary":"write /tmp/ferrum-demo-extended.txt","trusted_context":{},"raw_inputs":[],"requested_resource_scope":[{"kind":"FilesystemPath","path":"/tmp/ferrum-demo-extended.txt","mode":"Write"}],"metadata":{}}'
```

Observed result:

- HTTP status: `200`
- Warnings: `[]`
- Response contained sanitized `intent_id`: `d228bf73-c4f5-467a-b47b-53110dca7270`

Result: ✅ PASS.

### Step 3 — Evaluate proposal

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/proposals/5af85ef6-5d79-4da4-9866-299797ed4f15/evaluate \
  -H "Content-Type: application/json" \
  -d '{"proposal_id":"5af85ef6-5d79-4da4-9866-299797ed4f15","intent_id":"d228bf73-c4f5-467a-b47b-53110dca7270","step_index":1,"title":"demo proposal","tool_name":"fs.write","server_name":"fs","raw_arguments":{"path":"/tmp/ferrum-demo-extended.txt","content":"hello extended"},"expected_effect":"write file","estimated_risk":"Low","requested_rollback_class":"R0NativeReversible","taint_inputs":[],"metadata":{},"created_at":"2026-05-19T00:00:00Z"}'
```

Observed result:

- HTTP status: `200`
- Decision: `Allow`
- Reason: `proposal passed default scaffold policy`
- Warning: `advisory mismatch: inferred effect FileMutation is not in allowed outcomes`

Result: ✅ PASS.

### Step 4 — Mint capability

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/capabilities/mint \
  -H "Content-Type: application/json" \
  -d '{"intent_id":"d228bf73-c4f5-467a-b47b-53110dca7270","proposal_id":"5af85ef6-5d79-4da4-9866-299797ed4f15","tool_binding":{"server_name":"fs","tool_name":"fs.write"},"resource_bindings":[{"kind":"File","path":"/tmp/ferrum-demo-extended.txt","mode":"Write"}],"argument_constraints":[],"taint_budget":{"max_taint_score":10,"allow_external_tool_output":true,"allow_external_metadata":true,"allow_untrusted_text":true},"approval_binding":null,"requested_ttl_secs":60,"metadata":{}}'
```

Observed result:

- HTTP status: `200`
- Warnings: `[]`
- Response contained capability id: `85c9cdbb-1f3d-4a22-b1e9-c60b9cef9309`

Result: ✅ PASS.

### Step 5 — Authorize execution

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/executions/authorize \
  -H "Content-Type: application/json" \
  -d '{"proposal_id":"5af85ef6-5d79-4da4-9866-299797ed4f15","capability_id":"85c9cdbb-1f3d-4a22-b1e9-c60b9cef9309","dry_run":false}'
```

Observed result:

- HTTP status: `200`
- Warnings: `[]`
- Response contained execution id: `0b85c3ad-79dd-441e-9423-1141cc90f898`

Result: ✅ PASS.

### Step 6 — Prepare

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/executions/0b85c3ad-79dd-441e-9423-1141cc90f898/prepare \
  -H "Content-Type: application/json"
```

Observed result:

- HTTP status: `200`
- `prepared`: `true`
- Warnings: `[]`

Result: ✅ PASS.

### Step 7 — Execute

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/executions/0b85c3ad-79dd-441e-9423-1141cc90f898/execute \
  -H "Content-Type: application/json" \
  -d '{"payload":{"content":"hello extended"}}'
```

Observed result:

- HTTP status: `200`
- `executed`: `true`
- Warnings: `[]`

Result: ✅ PASS.

### Step 8 — Verify

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/executions/0b85c3ad-79dd-441e-9423-1141cc90f898/verify \
  -H "Content-Type: application/json"
```

Observed result:

- HTTP status: `200`
- `verified`: `true`
- Warnings: `[]`

Result: ✅ PASS.

### Step 9 — Evaluate outcome

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/executions/0b85c3ad-79dd-441e-9423-1141cc90f898/evaluate-outcome \
  -H "Content-Type: application/json" \
  -d '{"execution_id":"0b85c3ad-79dd-441e-9423-1141cc90f898","actual_effect":"FileMutation","description":"local extended quickstart wrote /tmp/ferrum-demo-extended.txt","result_digest":null,"adapter_success":true,"adapter_metadata":{}}'
```

Observed result:

- HTTP status: `200`
- `aligned`: `true`
- Reason: `outcome matches intent expectations`
- Warning: `advisory mismatch: inferred effect FileMutation is not in allowed outcomes`

Result: ✅ PASS.

### Step 10 — Query lineage

Command:

```bash
curl http://127.0.0.1:18080/v1/provenance/lineage/0b85c3ad-79dd-441e-9423-1141cc90f898
```

Observed result:

- HTTP status: `200`
- Response contained keys: `events`, `execution_id`

Result: ✅ PASS.

### Timing

Total elapsed time for the corrected extended flow (steps 1–10, excluding build/start): **0.384 s**.

This is consistent with the DOC-1 "<30 min" target for the API/curl flow. It does NOT validate ferrumctl or MCP paths. It does NOT constitute a fresh-user test. DOC-1 acceptance criterion remains OPEN.

## DOC-2 — No secrets required

All validated steps ran with `auth_mode=disabled`. No bearer token, API key, or other secret was required.

- `FERRUMD_BEARER_TOKEN`: not set
- `curl` commands: no `-H "Authorization: ..."` header used
- Response bodies: no secrets or live tokens present

Result: ✅ PASS for the validated API/curl flow ONLY. DOC-2 acceptance criterion remains OPEN because ferrumctl and MCP paths are not validated.

## Cleanup

After capturing evidence, the local server was stopped.

## Non-claims

- **NOT production-ready**: This is a local demo with auth disabled and in-memory storage.
- **NOT a full quickstart validation**: The API/curl flow is validated. ferrumctl and MCP paths remain scaffold.
- **NOT tested by a new user**: This was an engineering validation run, not a fresh-user usability test.
- **NOT target-host validated**: Ran on a local workstation against loopback only.
- **NOT a Block A closure**: No real domain or DNS was used. Block A remains WAIVED/CONDITIONAL.
- **NOT a G2 claim**: This evidence does not assert full G2 completion.
- **No secrets printed**: All IDs shown are sanitized placeholders. No live tokens, passwords, or keys appear in this artifact.

## Related docs

- [`docs/guides/quickstart.md`](../../guides/quickstart.md) — Updated quickstart guide
- [`docs/production-readiness-v2/07-product-docs-plan.md`](../../production-readiness-v2/07-product-docs-plan.md) — Product docs roadmap
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — Evidence checklist
