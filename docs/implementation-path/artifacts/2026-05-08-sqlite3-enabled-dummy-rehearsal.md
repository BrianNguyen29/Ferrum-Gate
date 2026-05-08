# sqlite3-Enabled Dummy Rehearsal — 2026-05-08

> **Status**: LOCAL-TEST/DUMMY ONLY.
> **Purpose**: Record a fresh dummy Path 2 rehearsal after installing local `sqlite3`.
> **Scope**: Local development environment only; no target host, no operator signoff, no G2 completion, no production-ready claim.

---

## Summary

`sqlite3` was installed locally and the dummy Path 2 rehearsal was rerun successfully. The restore drill now included data comparison rather than only integrity checks.

Final rehearsal result:

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
/tmp/tmp.h3RikwdWwD
```

---

## sqlite3 Installation Evidence

Initial availability check:

```bash
which sqlite3
```

Result: no output; `sqlite3` was not installed.

Package index update attempt:

```bash
sudo apt-get update
```

Result: failed because an existing apt source returned:

```text
E: The repository 'https://github.com stable Release' does not have a Release file.
```

This did not block direct package installation from the available Ubuntu package cache/repository.

Install command:

```bash
sudo apt-get install -y sqlite3
```

Result: succeeded.

Version check:

```bash
sqlite3 --version
```

Result:

```text
3.45.1 2024-01-30 ... (64-bit)
```

Security note: no sudo password, bearer token value, or real secret is recorded in this artifact.

---

## Rehearsal Command

```bash
bash scripts/run_dummy_path2_rehearsal.sh --keep-output
```

---

## Phase Details

### Phase 0 — Preflight

Result: PASS

- script syntax valid
- canonical docs checked as not modified:
  - `docs/implementation-path/54-operator-signoff-packet.md`
  - `docs/implementation-path/58-workload-compensation-drill-evidence-template.md`
  - `docs/implementation-path/59-pilot-readiness-evidence-packet.md`
  - `docs/implementation-path/63-path-2-target-environment-spec.md`
  - `docs/implementation-path/65-path-2-target-questionnaire.md`
- dummy values doc exists
- dummy bundle template exists

### Phase 1 — Dummy Config

Result: PASS

- dummy config created at `/tmp/tmp.h3RikwdWwD/00-config/dummy-ferrumgate.toml`
- dummy token saved at `/tmp/tmp.h3RikwdWwD/00-config/dummy-token.txt` with `600` permissions
- token value not recorded

### Phase 2 — Local Probes

Result: PASS

| Probe | Result |
| --- | --- |
| `/v1/healthz` | 200 |
| `/v1/readyz` | 200 |
| `/v1/readyz/deep` | 200 |
| `/v1/metrics` | captured |

### Phase 3 — Auth Smoke

Result: PASS

```text
Passed: 7
Failed: 0
AUTH SMOKE: ALL CHECKS PASSED
```

### Phase 4 — Restore Drill with sqlite3 Data Comparison

Result: PASS

`sqlite3` was available:

```text
/usr/bin/sqlite3
```

Restore drill checks:

| Check | Result |
| --- | --- |
| source store integrity | PASS |
| backup integrity | PASS |
| restore completed | PASS |
| restored database integrity | PASS |
| data comparison | PASS |

Data comparison result:

```text
[PASS] Data match: original and restored databases are identical
```

### Phase 5 — D1–D6 Drills

Result: PASS

```text
Total drills: 6
Passed: 6
Failed: 0
```

Breakdown:

| Drill | Scope | Tests |
| --- | --- | ---: |
| D1 | filesystem adapter rollback | 11 |
| D2 | git adapter rollback | 9 |
| D3 | git remote fail-closed | 1 |
| D4 | HTTP adapter compensation | 22 |
| D5 | SQLite adapter rollback | 10 |
| D6 | maildraft adapter rollback | 7 |

### Phase 6 — Evidence Skeleton

Result: PASS

- generated local-only D1–D6 evidence skeleton

### Phase 7 — Summary

Result: PASS

- generated rehearsal summary markdown
- generated rehearsal summary JSON
- generated rehearsal watermark

---

## Explicit Non-Claims

- This is not G2 evidence.
- This is not target-host evidence.
- This is not operator signoff.
- This does not authorize a production pilot.
- This does not make FerrumGate production-ready.
- Dummy/local outputs must not be copied into docs `54`, `59`, `63`, or `65` as real evidence.

FerrumGate remains **RC-ready / conditional single-node SQLite** until real target evidence and operator signoff exist.
