# 68 — Path 2 Operator Handoff Packet (Quick-Reference)

> **Status**: Documentation-only. Concise operator quick-reference.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. No production-ready claim.
> **Constraint**: Do not sign G2, do not claim pilot accepted, do not execute on target host.
> **Phase 3**: Blocked until G2/operator evidence complete.

---

## Purpose

This is the concise operator quick-reference for Path 2. For full detail, see linked docs.

**This packet does NOT:**
- Complete any G2 gate
- Authorize any pilot
- Claim production-ready
- Replace operator signoff

---

## Required Operator Actions (Blockers to Resolve)

Before Phase B execution, operator must provide:

| Blocker | Resolution | Doc |
|---------|------------|-----|
| Target host access | Operator/infrastructure provides SSH | 65 |
| Bearer token | `openssl rand -hex 32` | 65 §F |
| Doc 65 completed | All PROVIDE fields filled | 65 |
| TLS certificates | Operator/security provides | 65 §C |
| DNS A record | Operator/network confirms | 65 §C |

---

## Values Operator Must Provide

From [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md):

| Category | Key Fields |
|----------|------------|
| **Host** | FQDN/IP, SSH user, OS, firewall ports (80/443/8080) |
| **Storage** | Store path (e.g. `/var/lib/ferrumgate/ferrumgate.db`), backup directory |
| **Auth** | Bearer token (generated, not hardcoded), token env file path |
| **Proxy** | Domain, TLS cert/key paths, nginx config adapted |
| **Monitoring** | Prometheus available (yes/no), alert contact |
| **Backup** | Schedule, retention policy, scheduler method (cron/systemd/CI) |

**No real secrets in documentation.** Use placeholders.

---

## Command / Runbook Sequence

Full commands in [`62-path-2-operator-runbook.md`](./62-path-2-operator-runbook.md).

### Phase 0 — Preflight

```bash
# Verify binaries
which ferrumd && which ferrumctl

# Verify Python for evidence skeleton
python3 --version
```

### Phase 1 — Config Adaptation

```bash
# Copy and adapt non-prod config
cp configs/examples/nonprod-ferrumgate.toml /path/to/<nonprod-url>-ferrumgate.toml
# Update: bind_addr, store.path, auth_mode="Bearer"
```

### Phase 2 — Server Startup + Probes

```bash
# Start ferrumd
ferrumd --config /path/to/<nonprod-url>-ferrumgate.toml &

# Probe sequence
curl http://<target-host>:8080/v1/healthz        # expect 200
curl http://<target-host>:8080/v1/readyz         # expect 200
curl http://<target-host>:8080/v1/readyz/deep   # expect 200
curl http://<target-host>:8080/v1/metrics        # expect 200 + prometheus format
curl http://<target-host>:8080/v1/approvals       # expect 401 (no auth)
```

### Phase 3 — D1–D6 Compensation Drills

```bash
# Drills require bearer auth
FERRUM_TOKEN="${FERRUM_BEARER_TOKEN}"
FERRUM_BASE="http://<target-host>:8080"

# Full drill commands in doc 62 §Phase 3
# D1: FileWrite/FileDelete
# D2: GitCommit
# D3: Git Remote Push (fail-closed) — use non-prod remote only
# D4: HTTP POST Replay
# D5: SQLite DML
# D6: Maildraft
```

### Phase 5 — Restore Drill

```bash
STORE_PATH="<store-path>"
BACKUP_DIR="<backup-dir>"

# Backup
ferrumctl backup create --db-path "$STORE_PATH" --output-dir "$BACKUP_DIR"

# Verify
ferrumctl backup verify --db-path "$STORE_PATH"

# Stop ferrumd, restore, restart, verify
# Full commands in doc 62 §Phase 5
```

---

## Evidence to Collect

| Phase | Evidence | Path |
|-------|----------|------|
| Probes | Probe output | `/tmp/readyz_deep_output.txt` |
| Drills | D1–D6 logs | `/tmp/d{1,2,3,4,5,6}_drill_output.txt` |
| Skeleton | D1–D6 evidence skeleton | `/tmp/d1_d6_skeleton.md` |
| Restore | Restore drill log | `/tmp/restore_drill_output.txt` |
| G2 evidence | Doc 59 G2.1–G2.8 | `59-pilot-readiness-evidence-packet.md` |
| Signoff | Doc 54 | `54-operator-signoff-packet.md` |

### Evidence Skeleton Generation

```bash
cat /tmp/d*_drill_output.txt \
    | python3 scripts/generate_evidence_skeleton.py --type d1-d6 \
    > /tmp/d1_d6_skeleton.md
```

---

## Pass / Fail Criteria

### Probes (Phase 2)

| Check | Expected | Fail action |
|-------|----------|-------------|
| `/v1/healthz` | 200 | Investigate |
| `/v1/readyz` | 200 | Investigate |
| `/v1/readyz/deep` | 200 | Do not proceed |
| `/v1/approvals` (no auth) | 401 | Auth misconfigured |
| `/v1/metrics` | 200 + prometheus | Investigate |

### Restore Drill (Phase 5)

| Criterion | Expected |
|-----------|----------|
| `ferrumctl backup verify` pre-restore | OK |
| Pre-restore copy created | true |
| `ferrumctl backup restore` completes | true |
| `ferrumctl backup verify` post-restore | OK |
| `readyz/deep` returns 200 after restart | true |

### Compensation Drills (Phase 3)

| Drill | recovered | Acceptable? |
|-------|-----------|-------------|
| D1.1 FileWrite | true | yes |
| D1.2 FileDelete | true | yes |
| D2.1 GitCommit | true | yes |
| D3.1 Git Remote Push | true | yes |
| D3.2 Git Remote Push fail-closed | `recovered: false` + `failure_reason` | yes |
| D4.1 HTTP POST replay | true | yes |
| D4.2 HTTP fail-closed | `recovered: false` | yes |
| D5 SQLite DML | true | yes |
| D6 Maildraft | true | yes |

**Stop condition**: Any drill `recovered: false` with no valid `failure_reason` → abort pilot.

---

## G2 Gates (Operator-Owned, Pending Signoff)

| Gate | Evidence Required | Signoff Doc |
|------|-------------------|-------------|
| G2.1 Workload Model | Write workload ≤300 writes/s | 59 §G2.1 |
| G2.2 Auth/TLS Config | Bearer auth + TLS proxy confirmed | 59 §G2.2 |
| G2.3 Backup Schedule | External scheduler operational | 59 §G2.3 |
| G2.4 Restore Drill | `PRAGMA integrity_check` passes | 59 §G2.4 |
| G2.5 RPO/RTO Acceptance | Backup/restore objectives accepted | 59 §G2.5 |
| G2.6 Production Eval | All dimensions SATISFIED or CONDITIONAL | 59 §G2.6 |
| G2.7 Accepted-Risk Review | Weak Spots 1–4 reviewed | 59 §G2.7 |
| G2.8 Compensate Noop | Noop risk accepted for target adapters | 59 §G2.8 |

**Only after all G2 gates signed may operator sign doc 54 pilot acceptance.**

---

## Explicit Non-Claims

- **No G2 complete**: G2.1–G2.8 remain pending until operator signs doc 59
- **No pilot authorized**: Pilot not authorized until doc 54 signed
- **No production-ready**: FerrumGate v1 remains RC-ready/conditional
- **No PostgreSQL**: PostgreSQL/multi-node/HA not in Phase 1 scope
- **Operator signatures filled for conditional pilot only**: BrianNguyen signed G2.1–G2.8 and doc 54 on 09/05/2026 for conditional single-node SQLite pilot scope. Signature fields are NOT blank for that scoped acceptance. Full production-ready signoff remains pending.
- **Phase 3 blocked**: Until G2/operator evidence complete

---

## Phase 3 Decision Gate

After Phase B complete, operator decides:

| Decision | Criteria | Next Action |
|----------|----------|-------------|
| Proceed to Phase 3 | Write rate >300 writes/s OR operator prefers PostgreSQL | Engineering initiates Phase P1 per ADR-50 |
| Continue Path 2 | Single-node SQLite acceptable | Bounded production use; Phase 3 deferred |
| Abort pilot | Any abort trigger fires | Investigate, fix, re-evaluate |

---

## Quick Links

| Doc | Purpose |
|-----|---------|
| [65-path-2-target-questionnaire.md](./65-path-2-target-questionnaire.md) | Required operator inputs |
| [62-path-2-operator-runbook.md](./62-path-2-operator-runbook.md) | Full command sequences |
| [63-path-2-target-environment-spec.md](./63-path-2-target-environment-spec.md) | Target spec template |
| [59-pilot-readiness-evidence-packet.md](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence |
| [54-operator-signoff-packet.md](./54-operator-signoff-packet.md) | Pilot acceptance signoff |
| [61-path-2-execution-plan.md](./61-path-2-execution-plan.md) | Execution checklist |
| [66-path-2-operator-handoff.md](./66-path-2-operator-handoff.md) | Full handoff narrative |
| [69-local-dummy-target-values.md](./69-local-dummy-target-values.md) | **Optional**: Local-only dummy values for rehearsal (NOT operator evidence) |
| [70-security-hardening-local-only-plan.md](./70-security-hardening-local-only-plan.md) | Security hardening proposals, local-only audit commands, token rotation procedure |
| [path2-evidence-bundle-template](./path2-evidence-bundle-template/README.md) | **Blank template**: Copy to dated target-run dir before collecting real evidence |

---

*FerrumGate v1 RC-ready/conditional. Phase A complete does not authorize pilot. No production-ready claim. All G2 gates pending operator signoff.*
