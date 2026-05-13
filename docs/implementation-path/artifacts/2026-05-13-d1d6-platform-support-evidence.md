# D1–D6 Platform Support Evidence

> **Date**: 2026-05-13  
> **Artifact**: `docs/implementation-path/artifacts/2026-05-13-d1d6-platform-support-evidence.md`  
> **Status**: Engineering evidence — adapter wiring, API plan mode, and local checks recorded. B1 remains open. No production-ready claim.  
> **Scope**: D1–D6 adapter integration in `ferrumd`, API drill plan mode, OpenAPI coverage, runbook lifecycle overview, and local validation checks.

---

## 1. Summary

This artifact records the engineering implementation that improves D1–D6 platform support in `ferrumd` and the surrounding tooling. It does **not** claim closure of B1 (target-host D1–D6 evidence), G3.6 full acceptance, or production readiness.

Key improvements recorded:
- D4/D5/D6 adapter wiring is present in `ferrumd` (`main.rs` + `Cargo.toml`)
- API drill plan mode (`--api-drills --plan`) added to `scripts/run_d1_d6_drills.py` with token-safe output
- OpenAPI `execute` and `verify` endpoint coverage added to `openapi/ferrumgate-control-api.v1.yaml`
- Runbook lifecycle overview corrected in `62-path-2-operator-runbook.md`
- Local checks passed: `cargo check --package ferrumd`, `cargo fmt`, Python syntax, YAML parse, API plan token safety

**Remaining limitation**: API plan mode generates compile/execute payload scaffolding only. Full automated lifecycle (intent-compile → proposal-evaluate → capability-mint → authorize → prepare → execute → verify → compensate) is not complete. B1 not executed.

---

## 2. Evidence Facts

### 2.1 Code/Doc Changes Present in Worktree

| File | Change | Adapter / Area |
|------|--------|----------------|
| `bins/ferrumd/Cargo.toml` | Dependencies for `ferrum-adapter-http`, `ferrum-adapter-sqlite`, `ferrum-adapter-maildraft` | D4, D5, D6 |
| `bins/ferrumd/src/main.rs` | `register_http_adapter`, `register_sqlite_adapter`, `register_maildraft_adapter` wired; plannable adapters registered for rollback service | D4, D5, D6 |
| `crates/ferrum-adapter-http/src/lib.rs` | HTTP adapter with prepare→execute→verify→compensate/rollback, idempotency key, retry config, pool config, `http.replay_v1` compensation | D4 |
| `crates/ferrum-adapter-sqlite/src/lib.rs` | SQLite adapter with SAVEPOINT-based DML rollback, schema-capture DDL rollback, `SqlRowCountRange` verify check | D5 |
| `openapi/ferrumgate-control-api.v1.yaml` | `/v1/executions/{id}/execute` and `/v1/executions/{id}/verify` endpoints documented with request/response schemas | API coverage |
| `scripts/run_d1_d6_drills.py` | `--api-drills`, `--plan`, `--bearer-token` CLI args; `API_DRILL_TEMPLATES` for D1–D6; token-safe plan output using `$FERRUM_BEARER_TOKEN` placeholder | Drill tooling |
| `docs/implementation-path/62-path-2-operator-runbook.md` | Lifecycle overview table updated with exact API paths; disclaimer that Phase 3 drill examples may need adaptation | Runbook |

### 2.2 Local Validation Checks

| Check | Command | Result |
|-------|---------|--------|
| Rust compilation (ferrumd) | `cargo check --package ferrumd` | **PASSED** — `Finished dev profile` |
| Rust formatting | `cargo fmt --all -- --check` | **PASSED** — no output (no violations) |
| Python syntax | `python3 -m py_compile scripts/run_d1_d6_drills.py` | **PASSED** — no output |
| OpenAPI YAML parse | `python3 -c "import yaml; yaml.safe_load(open('openapi/...')); print('YAML_OK')"` | **PASSED** — `YAML_OK` |
| API plan execution | `python3 scripts/run_d1_d6_drills.py --api-drills --plan --server-url https://ferrumgate.duckdns.org --output-dir /tmp/ferrum-api-drill-plan-verify2 --bearer-token dummy_secret_token_12345` | **EXIT 0** |
| Token placeholder count | `grep -c '$FERRUM_BEARER_TOKEN' /tmp/.../api_drill_commands.md` | **6** |
| Secret leak check | `grep -c 'dummy_secret_token_12345' /tmp/.../api_drill_commands.md` | **0** |

### 2.3 API Plan Mode Behavior

- Plan mode explicitly skips server smoke / live probes
- Generated markdown includes curl scaffolding for `POST /v1/intents/compile` per drill
- Execute payloads provided as JSON blocks for manual use after authorize+prepare
- No HTTP requests sent to the server in plan mode

---

## 3. Limitations (Explicit)

| # | Limitation | Impact |
|---|------------|--------|
| L1 | API plan mode generates execute-phase payload scaffolding only. It does **not** automate the full governance lifecycle (compile → evaluate → mint → authorize → prepare → execute → verify → compensate). | Operator must still perform preceding governance steps manually or via `ferrumctl`. |
| L2 | B1 (target-host D1–D6 evidence) is **not executed** by this artifact. | `58-workload-compensation-drill-evidence-template.md` remains unfilled; operator must run drills on target host. |
| L3 | G3.6 full acceptance **not claimed**. | Compile-only and full-duration compile-only sequences exist; adapter-mixed real workload not performed. |
| L4 | No production-ready, PostgreSQL, or HA claim is made. | Single-node SQLite scope only. |
| L5 | The runbook Phase 3 drill examples use illustrative shapes (`/v1/intents`, `/v1/proposals`, `/v1/approvals`) that may not match exact API paths. | Operators must adapt curl commands to actual endpoints per `openapi/ferrumgate-control-api.v1.yaml` and the lifecycle table in `62-path-2-operator-runbook.md`. |

---

## 4. Cross-References

| Document | Relationship |
|----------|-------------|
| `62-path-2-operator-runbook.md` | Operator command sequences; lifecycle overview table updated with exact API paths |
| `115-sqlite-path2-target-host-checklist.md` | B1–B5, B8 blocker checklist; B1 remains open |
| `116-g36-monitoring-execution-plan.md` | G3.6 real workload plan; full acceptance not achieved |
| `112-post-p5c-completion-execution-plan.md` | Master plan; Track 4 (B1) status updated to "platform support improved, B1 remains not executed" |
| `scripts/run_d1_d6_drills.py` | Drill runner with API plan mode |
| `openapi/ferrumgate-control-api.v1.yaml` | Execute/verify endpoint coverage |

---

## 5. Verification Self-Check

- [x] No secret values recorded in this artifact
- [x] No bearer token values recorded
- [x] No production-ready claim
- [x] No B1 closure claim
- [x] No G3.6 full acceptance claim
- [x] No PostgreSQL/HA claim
- [x] All referenced commands were actually executed
- [x] Token safety verified: `$FERRUM_BEARER_TOKEN` placeholder used; dummy token not leaked

---

*Artifact created: 2026-05-13. Engineering evidence only. B1 remains open. Operator review required before use.*
