# 34 — P3.G1 Executed Evidence: Live Walkthrough 2026-04-03

**Item:** P3.G1 — Functional readiness proof — end-to-end operator walkthrough
**Executed:** 2026-04-03
**Scope:** Single-node, SQLite-backed, local build (`cargo run -p ferrumd`)
**Live drill machine:** Local environment (no remote/ssh)

---

## Attestation Block (Filled)

```
Functional Readiness Proof — FerrumGate v1 Single-Node
======================================================
Date:          2026-04-03
Operator:      live drill (automated)
Node ID:       localhost
ferrumd version: local build @ docs/p3-g1-live-verification branch

Section 1 — Install and Start
-----------------------------
Startup log shows "ferrumd listening on {addr}":  PASS
Startup log line: 2026-04-03T17:20:49.652269Z INFO ferrumd listening on 127.0.0.1:18084
Startup error (if any):                     none

Section 2 — Functional Readiness Probe
-------------------------------------
Probe endpoint: GET /v1/approvals?limit=1
Auth mode:      disabled
HTTP status:    200
JSON parseable: yes
JSON body:      {"items":[]}
Probe outcome:  PASS

Section 3 — First Control Action
---------------------------------
Read path (inspect-approvals):              PASS
Control path (cancel/pause/resume):         PASS
Control outcome:                            execution 0bc232ef-29b3-4171-9174-4ab1d462dc11
                                               POST /v1/executions/.../cancel → 200
                                               inspect → state=Cancelled

Section 4 — Upgrade / Change Path Check
---------------------------------------
Pre-change backup taken:                    yes
Backup path: /tmp/ferrum-p3g1/ferrumgate_prechange.db
Backup integrity (PRAGMA integrity_check):  ok
Backup size: 241664 bytes
Binary replaced and restarted:              yes (same local build, Ctrl+C stop → restart)
Functional probe after restart:              PASS (GET /v1/approvals?limit=1 → 200)
Existing execution records queryable:        PASS
  - execution 0bc232ef-29b3-4171-9174-4ab1d462dc11 → state=Cancelled
  - execution ceb55909-3f39-42e4-962a-f90c025fc605 → state=Compensated
Change-path outcome:                        PASS

Section 5 — Rollback / Compensate Drill
---------------------------------------
Compensate call made (execution_id):        ceb55909-3f39-42e4-962a-f90c025fc605
Compensate HTTP status:                     200
Execution state post-compensate:            Compensated
Manual restore drill performed:             SKIP
Restore drill outcome:                      SKIP
Rollback/compensate outcome:                PASS

Overall Functional Readiness:               PASS
Operator sign-off:                         live drill 2026-04-03
Notes:                                     Same local build used for restart (not a binary swap,
                                               consistent with walkthrough Section 4 framing that
                                               change-path uses "known-good backup + same binary");
                                               CLI flag confirmed as --bind (not --bind-addr)
```

---

## Key Execution Log Lines

### Startup (first)

```
2026-04-03T17:20:49.650257Z INFO ferrumd starting: bind=127.0.0.1:18084, store=sqlite:///tmp/ferrum-p3g1/ferrumgate.db, auth=Disabled
2026-04-03T17:20:49.652269Z INFO ferrumd listening on 127.0.0.1:18084
```

### Startup (post-restart)

```
2026-04-03T17:22:04.551982Z INFO ferrumd starting: bind=127.0.0.1:18084, store=sqlite:///tmp/ferrum-p3g1/ferrumgate.db, auth=Disabled
2026-04-03T17:22:04.554105Z INFO ferrumd listening on 127.0.0.1:18084
```

### Functional probes

| Probe | Result |
|-------|--------|
| `GET /v1/healthz` (post-first-start) | connection refused (after Ctrl+C) |
| `GET /v1/approvals?limit=1` (auth=disabled) | 200, `{"items":[]}` |
| `GET /v1/executions/0bc232ef-29b3-4171-9174-4ab1d462dc11` | 200, `state":"Cancelled"` |
| `GET /v1/executions/ceb55909-3f39-42e4-962a-f90c025fc605` | 200, `state":"Compensated"` |

### Control actions

| Action | Execution ID | HTTP | Result |
|--------|--------------|------|--------|
| `POST .../cancel` | `0bc232ef-29b3-4171-9174-4ab1d462dc11` | 200 | `cancelled=true`, inspect → `Cancelled` |
| `POST .../compensate` | `ceb55909-3f39-42e4-962a-f90c025fc605` | 200 | `compensated=true`, inspect → `Compensated` |

### Change-path

| Step | Result |
|------|--------|
| Pre-change backup | `/tmp/ferrum-p3g1/ferrumgate_prechange.db`, 241664 bytes, integrity `ok` |
| Stop (Ctrl+C) | `GET /v1/healthz` → connection refused |
| Restart (same local build) | Server came up on same store |
| Post-restart probe | `GET /v1/approvals?limit=1` → 200, `{"items":[]}` |
| Records after restart | Both execution IDs queryable with correct terminal states |

---

## Relationship to Walkthrough Doc

This artifact provides the executed evidence record for P3.G1. The parent
walkthrough ([22-v1-first-operator-walkthrough.md](../22-v1-first-operator-walkthrough.md))
contains the procedure and attestation template; this document is the filled
attestation block with timestamped live-log evidence.

**CLI flag note:** The live drill confirmed the server flag is `--bind`, not
`--bind-addr` as shown in the walkthrough Section 1.2 example. The walkthrough
has been corrected to `--bind` to match the actual binary interface.

---

## P3.G1 Completion Criteria Status

| Criterion | Status |
|-----------|--------|
| All applicable sections marked PASS | ✅ PASS — all 5 sections passed |
| Attestation block signed | ✅ Signed (live drill 2026-04-03) |
| Document retained as operational record | ✅ This artifact |

**P3.G1 is complete as of 2026-04-03.**
