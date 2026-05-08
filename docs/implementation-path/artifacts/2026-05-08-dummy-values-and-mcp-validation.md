# Dummy Values + MCP Validation — 2026-05-08

> **Status**: LOCAL-TEST/DUMMY ONLY.
> **Purpose**: Record a fresh local validation run using dummy values and MCP smoke tests.
> **Scope**: No target host, no operator signoff, no G2 completion, no production-ready claim.

---

## Summary

Fresh local validation completed successfully for:

- dummy Path 2 rehearsal with generated local-only values
- MCP crate tests
- MCP live-local lifecycle smoke

This confirms local tooling and MCP behavior are working within their declared local scope.
It does **not** establish target-host readiness or production readiness.

---

## Dummy Value Sources

Dummy values already exist in the repo and remain the source of truth for local rehearsal:

- `docs/implementation-path/69-local-dummy-target-values.md`
- `docs/implementation-path/71-path-2-target-values-intake-packet.md` for real-value intake mapping
- `docs/implementation-path/path2-dummy-rehearsal-bundle/` for local-only bundle structure
- `scripts/run_dummy_path2_rehearsal.sh` for fresh generated dummy config/token/probe evidence

The fresh run generated runtime dummy values in a temporary directory only:

```text
/tmp/tmp.glyIRtAQ3o
```

The generated bearer token stayed in:

```text
/tmp/tmp.glyIRtAQ3o/00-config/dummy-token.txt
```

No real secret was committed. Canonical docs `54`, `58`, `59`, `63`, and `65` were checked and not modified.

---

## Commands Run

### MCP crate tests

```bash
cargo test -p ferrum-integrations-mcp
```

Result:

```text
lib tests: 189 passed, 0 failed
bin tests: 8 passed, 0 failed
doctests: 0 passed, 0 failed, 2 ignored
```

The two ignored doctests are internal/private helper examples and are intentionally non-executable examples.

### MCP lifecycle smoke

```bash
bash scripts/run_mcp_lifecycle_smoke.sh
```

Result:

```text
Passed: 13
Failed: 0
MCP LIFECYCLE SMOKE: ALL CHECKS PASSED
```

Observed lifecycle smoke details:

- MCP initialize: PASS
- `tools/list`: PASS, 17 tools
- blocked `ferrum_gate_approve_intent`: PASS, returns `-32001`
- blocked `ferrum_gate_reject_intent`: PASS, returns `-32001`
- read-only `ferrum_gate_health`: PASS
- all 8 lifecycle tools present: PASS
- unknown tool error handling: PASS, returns `-32601`
- ping: PASS
- `ferrum_gate_submit_intent`: PASS, generated intent id `b106fceb-d4b8-471a-aebe-d1b009b453f9`
- `ferrum_gate_evaluate_intent`: PASS
- `ferrum_gate_mint_capability`: PASS
- `ferrum_gate_list_intents`: PASS

### Dummy Path 2 rehearsal

```bash
bash scripts/run_dummy_path2_rehearsal.sh --keep-output
```

Result:

```text
phase0: passed
phase1: passed
phase2: passed
phase3: passed
phase4: passed
phase5: passed
phase6: passed
phase7: passed
```

Output directory:

```text
/tmp/tmp.glyIRtAQ3o
```

---

## Dummy Rehearsal Details

### Phase 0 — Preflight

Result: PASS

- script syntax valid
- canonical docs `54`, `58`, `59`, `63`, `65` not modified
- dummy values doc exists
- dummy bundle template exists

### Phase 1 — Dummy Config

Result: PASS

- dummy config created at `/tmp/tmp.glyIRtAQ3o/00-config/dummy-ferrumgate.toml`
- dummy token created with `600` permissions

### Phase 2 — Local Probes

Result: PASS

- `/v1/healthz`: 200
- `/v1/readyz`: 200
- `/v1/readyz/deep`: 200
- `/v1/metrics`: captured

### Phase 3 — Auth Smoke

Result: PASS

```text
Passed: 7
Failed: 0
AUTH SMOKE: ALL CHECKS PASSED
```

### Phase 4 — Restore Drill

Result: PASS

- source store integrity: PASS
- backup integrity: PASS
- restore completed: PASS
- restored database integrity: PASS
- sqlite3 data comparison skipped because sqlite3 was unavailable

### Phase 5 — D1–D6 Drills

Result: PASS

```text
Total drills: 6
Passed: 6
Failed: 0
```

Breakdown:

- D1 filesystem adapter: PASS, 11 tests
- D2 git adapter: PASS, 9 tests
- D3 git remote fail-closed: PASS, 1 test
- D4 HTTP adapter: PASS, 22 tests
- D5 SQLite adapter: PASS, 10 tests
- D6 maildraft adapter: PASS, 7 tests

### Phase 6 — Evidence Skeleton

Result: PASS

- generated local-only D1–D6 evidence skeleton

### Phase 7 — Summary

Result: PASS

- generated rehearsal summary markdown
- generated rehearsal summary JSON
- generated rehearsal watermark

---

## MCP Functional Assessment

Based on the fresh test/smoke run, MCP is working for the implemented local scope:

| Area | Status | Notes |
| --- | --- | --- |
| stdio JSON-RPC transport | Working | initialize and ping pass |
| tool registry | Working | 17 tools listed |
| read-only health/list dispatch | Working | health and list-intents pass |
| lifecycle dispatch | Working | submit/evaluate/mint/list pass in live-local smoke |
| blocked approve/reject behavior | Working as designed | both return `-32001` |
| DLP/redaction tests | Working | covered by MCP crate tests |
| output sanitization tests | Working | covered by MCP crate tests |
| full pipeline mock tests | Working | covered by MCP crate tests |

Known limitations:

- approve/reject MCP tools remain intentionally blocked until backend mutation endpoints exist
- live smoke is local dev-mode, not target-host evidence
- no production performance benchmark was run
- no real target rollback/compensation evidence was collected
- direct MCP provenance emission remains forbidden; provenance stays gateway-owned

---

## Explicit Non-Claims

- No G2 gate is complete.
- No operator signoff exists.
- No production pilot is authorized.
- No real target values were supplied.
- No real target host was used.
- Dummy output is not operator evidence.
- FerrumGate remains RC-ready / conditional single-node SQLite.

---

## Recommended Next Steps

1. If you want more realistic local dummy testing, install `sqlite3` and rerun the dummy rehearsal so the restore drill can include data comparison.
2. Keep using `docs/implementation-path/71-path-2-target-values-intake-packet.md` to collect real target values.
3. Do not fill docs `54`, `59`, `63`, or `65` with dummy values.
4. Treat MCP as locally operational for implemented scope, but not production/G2-certified.
