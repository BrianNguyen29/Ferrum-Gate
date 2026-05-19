# Quickstart Local Validation Evidence — 2026-05-19

## Status

- **Scope**: DOC-1 / DOC-2 local quickstart validation.
- **Verdict**: ✅ PASS for core 4-step curl sequence.
- **Production-ready**: NO.
- **Full quickstart end-to-end**: NOT COMPLETE (execution pipeline, ferrumctl, MCP remain scaffold).
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
| 3 | `/v1/proposals/{proposal_id}/evaluate` | `POST` | HTTP 200 decision `Allow` (with advisory warning) |
| 4 | `/v1/capabilities/mint` | `POST` | HTTP 200 with lease |

## DOC-1 — Core quickstart sequence timing

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
  -d '{"principal_id":"a8d023fb-a81b-4bc1-8397-5ba921e8a233","title":"demo-write","goal":"write a demo file","agent_plan_summary":"write /tmp/ferrum-demo.txt","trusted_context":{},"raw_inputs":[],"requested_resource_scope":[],"metadata":{}}'
```

Observed result:

- HTTP status: `200`
- Response contained sanitized `intent_id`: `a8d023fb-a81b-4bc1-8397-5ba921e8a233`

Result: ✅ PASS.

### Step 3 — Evaluate proposal

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/proposals/6a8c9b36-f554-4bf1-957a-f3e06de84ca8/evaluate \
  -H "Content-Type: application/json" \
  -d '{"proposal_id":"6a8c9b36-f554-4bf1-957a-f3e06de84ca8","intent_id":"a8d023fb-a81b-4bc1-8397-5ba921e8a233","step_index":1,"title":"demo proposal","tool_name":"fs.write","server_name":"fs","raw_arguments":{"path":"/tmp/ferrum-demo.txt","content":"hello"},"expected_effect":"write file","estimated_risk":"Low","requested_rollback_class":"R0NativeReversible","taint_inputs":[],"metadata":{},"created_at":"2026-05-19T00:00:00Z"}'
```

Observed result:

- HTTP status: `200`
- Decision: `Allow`
- Warning: `advisory mismatch: inferred effect FileMutation is not in allowed outcomes`

Result: ✅ PASS.

### Step 4 — Mint capability

Command:

```bash
curl -X POST http://127.0.0.1:18080/v1/capabilities/mint \
  -H "Content-Type: application/json" \
  -d '{"intent_id":"a8d023fb-a81b-4bc1-8397-5ba921e8a233","proposal_id":"6a8c9b36-f554-4bf1-957a-f3e06de84ca8","tool_binding":{"server_name":"fs","tool_name":"fs.write"},"resource_bindings":[{"kind":"File","path":"/tmp/ferrum-demo.txt","mode":"Write"}],"argument_constraints":[],"taint_budget":{"max_taint_score":0,"allow_external_tool_output":false,"allow_external_metadata":false,"allow_untrusted_text":false},"approval_binding":null,"requested_ttl_secs":300,"metadata":{}}'
```

Observed result:

- HTTP status: `200`
- Response contained a capability lease.

Result: ✅ PASS.

### Timing

Total elapsed time for the validated 4-step sequence: **0.27 s**.

This is consistent with the DOC-1 "<30 min" target for the core sequence only. It does NOT validate the full quickstart (which includes execution pipeline, ferrumctl, and MCP steps). It does NOT constitute a fresh-user test. DOC-1 acceptance criterion remains OPEN.

## DOC-2 — No secrets required

All validated steps ran with `auth_mode=disabled`. No bearer token, API key, or other secret was required.

- `FERRUMD_BEARER_TOKEN`: not set
- `curl` commands: no `-H "Authorization: ..."` header used
- Response bodies: no secrets or live tokens present

Result: ✅ PASS for the validated 4-step sequence ONLY. DOC-2 acceptance criterion remains OPEN because ferrumctl and MCP paths are not validated.

## Cleanup

After capturing evidence, the local server was stopped.

## Non-claims

- **NOT production-ready**: This is a local demo with auth disabled and in-memory storage.
- **NOT a full quickstart validation**: Only the 4-endpoint curl sequence was validated. The extended flow (authorize, prepare, execute, verify, lineage, ferrumctl, MCP) remains scaffold.
- **NOT tested by a new user**: This was an engineering validation run, not a fresh-user usability test.
- **NOT target-host validated**: Ran on a local workstation against loopback only.
- **NOT a Block A closure**: No real domain or DNS was used. Block A remains WAIVED/CONDITIONAL.
- **NOT a G2 claim**: This evidence does not assert full G2 completion.
- **No secrets printed**: All IDs shown are sanitized placeholders. No live tokens, passwords, or keys appear in this artifact.

## Related docs

- [`docs/guides/quickstart.md`](../../guides/quickstart.md) — Updated quickstart guide
- [`docs/production-readiness-v2/07-product-docs-plan.md`](../../production-readiness-v2/07-product-docs-plan.md) — Product docs roadmap
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — Evidence checklist
