# DOC-3, ferrumctl, MCP, and Fresh-User Usability Evidence — 2026-05-19

## Status

- **Scope**: DOC-3 doc review + local ferrumctl validation + local MCP validation + usability status.
- **Verdict**: DOC-3 ✅ COMPLETE (after edits). ferrumctl ✅ LOCALLY VALIDATED (7/7 commands pass after bugfix). MCP ✅ LOCALLY VALIDATED (all tested tools pass after bugfix). Engineering local quickstart validation ✅ COMPLETE after docs corrections.
- **Production-ready**: NO.
- **Full quickstart end-to-end**: LOCALLY VALIDATED — API/curl, ferrumctl, and MCP locally validated by engineering; independent external fresh-user validation is not claimed.
- **Target-host / cloud**: NOT CLAIMED.
- **Block A**: WAIVED/CONDITIONAL — DuckDNS accepted for single-node SQLite pilot only; real owned domain required for production-ready/full G2.

---

## DOC-3 — Docs state production-ready limitations correctly

### Review findings

| Doc | Issue | Correction applied |
|-----|-------|-------------------|
| `docs/guides/hosted-deployment.md` | DEP-4 stated "validated locally only" and "DEP-4 remains open" | Updated to: target-host systemd runtime validated on `ferrumgate-nonprod` with evidence `2026-05-19-dep4-target-host-systemd-evidence.md`; service `ferrumgate.service`; **not production-ready** |
| `docs/guides/hosted-deployment.md` | Missing Block A / DuckDNS context | Added: DuckDNS accepted only for single-node SQLite pilot; real owned domain required for production-ready or full G2 closure; Block A remains WAIVED/CONDITIONAL |

### Evidence for DEP-4 correction

- Evidence artifact: `docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md`
- Target host: `ferrumgate-nonprod`
- Service name: `ferrumgate.service`
- Status: target-host runtime validated; **not** a production-ready claim

### Block A / DuckDNS context

- DuckDNS was accepted by the operator on 2026-05-17 for single-node SQLite pilot only.
- A real owned domain and DNS configuration are still required for production-ready status or full G2 closure.
- Block A remains **WAIVED/CONDITIONAL**.

Result: ✅ DOC-3 COMPLETE after edits.

---

## ferrumctl Local Validation

### Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Local development workstation |
| Binary | `./target/release/ferrumctl` |
| Server | `target/release/ferrumd --config configs/ferrumgate.dev.toml` |
| Bind address | `127.0.0.1:18080` |
| Store DSN | `sqlite::memory:` |
| Auth mode | `disabled` |

### Build

Command:

```bash
cargo build --release --package ferrumd --package ferrumctl --package ferrum-integrations-mcp
```

Observed result: Release profile finished in 3m34s.

Result: ✅ PASS.

### Test execution created via API

An execution was created via the local API for CLI lineage inspection:

- Elapsed: 0.367s
- Intent id: `de5b97bd-295c-44c4-8b93-4aecb0eaaa57`
- Proposal id: `8c809649-2e48-43b2-a2f0-0e84a10d93c5`
- Capability id: `2d7f470a-057f-4ec7-9b7a-c4bc859a3696`
- Execution id: `237ad16e-5c79-41c6-97dc-6d8dd8d38677`
- Path: `/tmp/ferrumctl-mcp-demo.txt`

### Validated ferrumctl commands

#### 1. server health

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server health
```

Observed result: `{"status":"ok"}`

Result: ✅ PASS.

#### 2. server readiness (shallow)

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server readiness
```

Observed result: `{"status":"ready"}`

Result: ✅ PASS.

#### 3. server readiness --deep

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server readiness --deep
```

Observed result: `{"status":"ok"}`

Result: ✅ PASS.

#### 4. server list-intents

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server list-intents --limit 5 --format json
```

Observed result: Returned one item with intent `de5b97bd-295c-44c4-8b93-4aecb0eaaa57` and `exec_state: "Committed"`.

Result: ✅ PASS.

#### 5. server inspect-lineage

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server inspect-lineage 237ad16e-5c79-41c6-97dc-6d8dd8d38677 --format json
```

Observed result: Returned events including `ActionProposalSubmitted`, `SideEffectPrepared`, `ToolCallPrepared`, `ToolCallExecuted`, `SideEffectVerified`, `SideEffectCommitted`.

Result: ✅ PASS.

#### 6. server metrics

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server metrics
```

Observed result: Returned Prometheus text including `ferrumgate_store_health_up 1`.

Result: ✅ PASS.

### ferrumctl bugfix regression

Rust fixes were applied and package tests passed:

```bash
cargo test --package ferrumctl
```

Original bugfix regression result: 53 passed. Later POL-1, UX-1/UX-2/UX-3/UX-6, and UX-5 CLI additions increased the ferrumctl package test count to 71 passed; the original regression remains valid.

#### Regression execution

A new execution was created for regression validation:

- Elapsed: 0.33s
- Intent id: `011f9304-3ed4-4742-b6e3-b767aa378b78`
- Proposal id: `ee34ca92-5b18-4509-8ddd-ddc31531ff9c`
- Capability id: `c7983af9-2da5-4f0b-a886-faeca9a24849`
- Execution id: `95afd0bc-58de-4b95-bb08-b5c3efce7d40`
- Path: `/tmp/ferrum-bugfix-regression.txt`

#### 7. server inspect-execution (post-bugfix)

```bash
./target/release/ferrumctl --server-url http://127.0.0.1:18080 server inspect-execution 95afd0bc-58de-4b95-bb08-b5c3efce7d40
```

Observed result: JSON response containing `execution_id`, `proposal_id`, `intent_id`, `capability_id`, `rollback_contract_id`, `decision`, `state`, `result_digest`. No decode error.

Result: ✅ PASS — `inspect-execution` bug fixed.

### ferrumctl summary

- **7 of 7 commands pass** locally after bugfix.
- ferrumctl is **locally validated**. Target-host validation is NOT claimed.

---

## MCP Local Validation

### Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Local development workstation |
| Binary | `./target/release/ferrum-mcp-server` |
| Server | `target/release/ferrumd --config configs/ferrumgate.dev.toml` |
| Gateway URL | `http://127.0.0.1:18080` |
| Auth mode | `disabled` (local) |

### Connection tests

#### ping

Observed result: `{success:true}`

Result: ✅ PASS.

#### initialize

Observed result: protocol_version `2024-11-05`, server `ferrum-integrations-mcp` version `0.1.0`.

Result: ✅ PASS.

#### tools/list

Observed result: Returned tools array.

Result: ✅ PASS.

#### ferrum_gate_health

Observed result: `{"status":"ok"}`

Result: ✅ PASS.

#### ferrum_gate_readyz_deep

Observed result: healthy `true`, status `ok`.

Result: ✅ PASS.

### Auth test — mutating tool without token

Command: `ferrum_gate_submit_intent` without bearer token.

Observed result: JSON-RPC error `-32002`, message `Mutating tool 'ferrum_gate_submit_intent' requires configured bearer token`.

Result: ✅ PASS — mutating tools fail closed without auth.

### Lifecycle flow with dummy local token

With `FERRUM_GATEWAY_BEARER_TOKEN=local-disabled-auth-token`:

1. **submit_intent**: Succeeded (warnings `[]`).
2. **evaluate_intent**: Decision `Allow` with advisory mismatch warning.
3. **mint_capability**: Capability `de1da6b0-1e47-4154-acc7-26c98b547d0e`.
4. **authorize_execution**: Execution `cf1a1e45-bdb1-41dd-bb86-8292135d9ef7`.
5. **prepare_execution**: `true`.
6. **execute_prepared**: `true`.
7. **verify_execution**: `true`.

Result: ✅ PASS — full lifecycle validated locally.

### Read query tests

- `ferrum_gate_get_execution` for existing execution: ✅ PASS.
- `ferrum_gate_list_intents`: ✅ PASS.

### MCP bugfix regression

Rust fixes were applied and package tests passed:

```bash
cargo test --package ferrum-integrations-mcp
```

Result: 236 lib + 8 bin passed.

#### query_lineage (post-bugfix)

With valid execution id:

Observed result: JSON-RPC success with content text containing `events` and `execution_id`.

Result: ✅ PASS — `query_lineage` returns lineage successfully.

With missing execution id:

Observed result: JSON-RPC error `-32602`, message `Missing required argument: execution_id`.

Result: ✅ PASS — proper validation error for missing required argument.

### MCP summary

- **All tested tools pass** locally after bugfix: connection, auth, lifecycle, read queries, and `query_lineage`.
- MCP is **locally validated**. Target-host validation is NOT claimed.

---

## Fresh-User Usability

### Status: BLOCKED

No independent fresh user was available to perform the quickstart. The validation was performed by engineering, not by someone outside the project team.

- Engineering simulation elapsed: API/curl 0.384s, ferrumctl test execution 0.367s.
- This is **not** a fresh-user test.

### Impact on DOC-1

DOC-1 acceptance criterion remains **OPEN**:

- API/curl flow timing is validated (0.384s).
- ferrumctl timing is validated for the tested commands.
- MCP timing is validated for the tested lifecycle and read queries.
- **Fresh-user test has NOT been performed.**

---

## DOC-2 note

- API/curl and ferrumctl validated paths require no live secrets (`auth_mode=disabled`).
- MCP mutating validation used documented dummy placeholder token `local-disabled-auth-token` because the MCP server has its own auth gate.
- All local demo paths (API/curl, ferrumctl, MCP) now pass after bugfix.
- DOC-2 acceptance criterion is **LOCALLY COMPLETE** for the local demo scope. Target-host validation is NOT claimed.

## Non-claims

- **NOT production-ready**: All validations are local only with auth disabled and in-memory SQLite.
- **NOT target-host validated**: ferrumctl and MCP were tested against loopback only.
- **NOT a full quickstart validation**: Local API/curl, ferrumctl, and MCP paths are validated. Target-host and fresh-user validation are NOT claimed.
- **NOT a fresh-user test**: Validated by engineering, not an independent user.
- **NOT a Block A closure**: Block A remains WAIVED/CONDITIONAL. DuckDNS is pilot-only.
- **NOT a G2 claim**: This evidence does not assert full G2 completion.
- **No secrets printed**: All IDs are sanitized. The dummy token `local-disabled-auth-token` is a documented placeholder, not a live secret.

## Related docs

- [`docs/guides/quickstart.md`](../../guides/quickstart.md) — Updated quickstart guide
- [`docs/guides/mcp-integration.md`](../../guides/mcp-integration.md) — MCP guide with validation status
- [`docs/guides/hosted-deployment.md`](../../guides/hosted-deployment.md) — Hosted deployment with corrected DEP-4 status
- [`docs/production-readiness-v2/07-product-docs-plan.md`](../../production-readiness-v2/07-product-docs-plan.md) — Product docs roadmap
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — Evidence checklist
