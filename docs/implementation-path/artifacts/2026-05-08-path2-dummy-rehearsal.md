# Path 2 Dummy Rehearsal Evidence — 2026-05-08

## Status

**PASS (local-only dummy rehearsal):** `scripts/run_dummy_path2_rehearsal.sh --keep-output` completed all phases with `passed` status.

## Scope

This artifact records a local dummy rehearsal using generated local-only values and temporary output directories.

This is **not** operator evidence, **not** G2 evidence, **not** production readiness evidence, and **not** target-host evidence.

## Command

```bash
bash scripts/run_dummy_path2_rehearsal.sh --keep-output
```

## Output Directory

```text
/tmp/tmp.OVTXgfU4CA
```

The output directory was intentionally kept because `--keep-output` was used. Contents are local-test/dummy only and may be removed at any time.

## Phase Results

| Phase | Result | Notes |
|-------|--------|-------|
| Phase 0 — Preflight | PASS | Script syntax checks passed; canonical docs 54/58/59/63/65 not modified |
| Phase 1 — Dummy Config | PASS | Dummy config and generated dummy token written under temp output dir |
| Phase 2 — Local Probes | PASS | `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics` succeeded against local ferrumd |
| Phase 3 — Auth Smoke | PASS | `run_local_auth_smoke.sh`: Passed 7, Failed 0 |
| Phase 4 — Restore Drill | PASS | `run_local_restore_drill.sh` completed backup/restore/verify in temp environment |
| Phase 5 — D1-D6 Drills | PASS | D1-D6 cargo drills all passed; server smoke skipped because no `--server-url` was provided |
| Phase 6 — Evidence Skeleton | PASS | Dummy D1-D6 evidence skeleton generated under temp output dir |
| Phase 7 — Summary | PASS | Summary markdown/json/watermark generated |

## D1-D6 Drill Summary

```text
Total drills: 6
Passed: 6
Failed: 0
```

Observed adapter drill results:

| Drill group | Result |
|-------------|--------|
| D1 filesystem adapter rollback tests | PASS — 11 tests |
| D2 git adapter rollback tests | PASS — 9 tests |
| D3 git remote fail-closed test | PASS — 1 test |
| D4 HTTP adapter compensate tests | PASS — 22 tests |
| D5 SQLite adapter rollback tests | PASS — 10 tests |
| D6 maildraft adapter rollback tests | PASS — 7 tests |

## Auth Smoke Summary

```text
Passed: 7
Failed: 0
AUTH SMOKE: ALL CHECKS PASSED
```

Auth smoke checked public health/ready/metrics endpoints and protected `/v1/approvals` behavior with no token, wrong token, and correct token.

## Restore Drill Summary

The restore drill completed in a temporary local environment:

```text
Source store integrity check passed
Backup integrity check passed
Restored database integrity check passed
```

`sqlite3` was not available in the environment, so data comparison was skipped by the underlying restore drill script. This does not affect the dummy rehearsal's local-only/non-G2 status.

## Important Correction During Rehearsal

The first full rehearsal run completed phases but exposed a shell summary issue: Markdown backticks inside double-quoted `echo` statements attempted command substitution. The script was corrected by using single-quoted `echo` for those Markdown lines, then the full rehearsal was rerun successfully.

## Canonical Docs Not Modified

The rehearsal preflight confirmed these canonical docs were not modified:

```text
docs/implementation-path/54-operator-signoff-packet.md
docs/implementation-path/58-workload-compensation-drill-evidence-template.md
docs/implementation-path/59-pilot-readiness-evidence-packet.md
docs/implementation-path/63-path-2-target-environment-spec.md
docs/implementation-path/65-path-2-target-questionnaire.md
```

## Non-Claims

This artifact does **not** establish:

- G2 completion;
- production readiness;
- pilot authorization;
- operator signoff;
- target-host deployment evidence;
- TLS/DNS/firewall readiness;
- PostgreSQL/multi-node/HA readiness.

## Next Steps for Real Path 2

Real Path 2 remains blocked until operator/user provides real target values and executes on the target host. The next real steps remain:

1. Fill `65-path-2-target-questionnaire.md` with real values only.
2. Fill `63-path-2-target-environment-spec.md` with real values only.
3. Execute drills on the real target host.
4. Populate `59-pilot-readiness-evidence-packet.md` from real evidence.
5. Sign `54-operator-signoff-packet.md` only after G2 gates are actually satisfied.
