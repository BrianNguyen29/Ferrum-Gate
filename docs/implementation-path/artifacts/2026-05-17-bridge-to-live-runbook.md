# Bridge-to-Live Validation Toolkit / Runbook

> **Status**: Planning artifact. No execution claimed. No production-ready claim.
> **Purpose**: Safe-by-default validation checklist and runbook for transitioning from local engineering verification to live target-host operator validation.
> **Scope**: Single-node SQLite v1 conditional pilot only.
> **Constraint**: `production-ready = NO` throughout. All live actions are approval-gated and dry-run by default.

---

## Philosophy

This toolkit is designed to be **safe-by-default**:
- Every live-action command requires explicit `--confirm` or operator dashboard access.
- Planning/dry-run modes are the default.
- No secrets are embedded in repo files.
- Production-ready remains **NO** until all blockers are closed and operator signs full G2.

---

## Pre-Flight Checklist (Local — Engineering-Owned)

Run these locally before any live transition:

| # | Check | Command | Expected Result | Owner |
|---|-------|---------|-----------------|-------|
| 1 | Layout validation | `bash scripts/validate_repo_layout.sh` | "Repository layout looks OK" | Engineering |
| 2 | Contract consistency | `python3 scripts/check_contract_consistency.py` | "VALIDATION PASSED" | Engineering |
| 3 | Format check | `cargo fmt --all -- --check` | exit 0 | Engineering |
| 4 | Compile check | `cargo check --workspace` | exit 0 | Engineering |
| 5 | Lint check | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | Engineering |
| 6 | Test check | `cargo test --workspace` | exit 0, all packages pass | Engineering |
| 7 | Pre-target gate | `bash scripts/run_pre_target_gate.sh --full` | "ALL LOCAL CHECKS PASSED" | Engineering |
| 8 | Security audit | `make audit` | `SECURITY AUDIT GATE: PASS` | Engineering |
| 9 | No secrets in diff | `git diff --check` | No trailing whitespace conflicts | Engineering |
| 10 | RC evidence | `python3 scripts/generate_rc_evidence.py` | "Overall: ALL PASS" | Engineering |
| 11 | Config sanity (target-host) | Verify `/etc/ferrumgate/ferrumgate.toml` contains `[server] store_dsn` pointing to on-disk SQLite | `[server] store_dsn = "sqlite:///var/lib/ferrumgate/ferrumgate.db"` (or equivalent) | Operator |

**Config sanity note**: `ferrumd` config precedence is CLI args > env vars > config file (`[server]`) > defaults. A `[store]` section is not parsed for `store_dsn`. If `[server] store_dsn` is missing and `FERRUMD_STORE_DSN` is unset, the service will fall back to `sqlite::memory:`, which does not survive restarts and will cause missing-table errors (e.g., `no such table: approvals`).

**All 11 must pass before live transition is considered.**

---

## Live Target-Host Gate Structure

Live validation is organized into **five gates** (G-L1 through G-L5). Each gate has:
- A dry-run / planning command
- A live / execute command (requires operator signoff)
- Evidence criteria
- Abort criteria

### Gate L1 — Target Reachability & TLS

**Dry-run / plan (default):**
```bash
# Planning only — does not hit live target
python3 scripts/validate_bridge_readiness.py --plan \
  --target-host ferrumgate.duckdns.org \
  --expected-ip 34.158.51.8
```

**Live execution (requires operator signoff):**
```bash
# Operator must confirm target-host access before running
export TARGET_HOST="<REAL_DOMAIN>"  # e.g., ferrumgate.duckdns.org (temp) or real domain
export EXPECTED_IP="34.158.51.8"
python3 scripts/validate_bridge_readiness.py --execute \
  --target-host "$TARGET_HOST" \
  --expected-ip "$EXPECTED_IP"
```

**Evidence criteria:**
- DNS resolves to expected IP
- HTTPS port 443 reachable
- TLS certificate valid (or nip.io/Let's Encrypt temporary accepted)
- HTTP→HTTPS redirect returns 308

**Abort criteria:**
- DNS does NOT resolve to expected IP
- TLS certificate expired or mismatched
- Port 443 not reachable

---

### Gate L2 — Authentication & Authorization

**Dry-run / plan (default):**
```bash
# Review auth configuration only
python3 scripts/validate_bridge_readiness.py --plan \
  --target-host ferrumgate.duckdns.org \
  --check-auth-config
```

**Live execution (requires operator signoff + bearer token):**
```bash
export FERRUM_BEARER_TOKEN="<operator-generated-token>"  # never commit this value
export TARGET_HOST="<REAL_DOMAIN>"
python3 scripts/validate_bridge_readiness.py --execute \
  --target-host "$TARGET_HOST" \
  --check-auth-live
```

**Evidence criteria:**
- No-token request to `/v1/approvals` returns HTTP 401
- Valid-token request to `/v1/approvals` returns HTTP 200
- Token is not logged or stored in output

**Abort criteria:**
- No-token request returns 200 (auth bypass)
- Valid-token request returns 401 (misconfiguration)
- Token appears in logs or output

---

### Gate L3 — Health & Readiness Probe

**Dry-run / plan (default):**
```bash
# Generate probe plan only
python3 scripts/validate_bridge_readiness.py --plan \
  --target-host ferrumgate.duckdns.org \
  --check-readiness-plan
```

**Live execution (requires operator signoff):**
```bash
export TARGET_HOST="<REAL_DOMAIN>"
export FERRUM_BEARER_TOKEN="<operator-generated-token>"
python3 scripts/validate_bridge_readiness.py --execute \
  --target-host "$TARGET_HOST" \
  --check-readiness-live
```

**Evidence criteria:**
- `/v1/healthz` returns HTTP 200 (shallow)
- `/v1/readyz` returns HTTP 200 (shallow)
- `/v1/readyz/deep` returns HTTP 200 and JSON with `store_healthy: true`
- `/v1/metrics` returns Prometheus text with required counters present

**Abort criteria:**
- `/v1/readyz/deep` returns 503
- Required metrics counters missing
- Store health gauge shows `ferrumgate_store_health_up 0`

---

### Gate L4 — Workload Generator Readiness

**Dry-run / plan (default):**
```bash
# Generate workload plan only — no live requests
python3 scripts/run_real_workload_generator.py --plan \
  --server-url "https://<REAL_DOMAIN>" \
  --output-dir /tmp/ferrum-g36-plan
```

**Live execution (requires operator signoff):**
```bash
export FERRUM_BEARER_TOKEN="<operator-generated-token>"
python3 scripts/run_real_workload_generator.py --execute \
  --server-url "https://<REAL_DOMAIN>" \
  --output-dir /tmp/ferrum-g36-live \
  --readyz-probes 5 \
  --readyz-interval 10
```

**Evidence criteria:**
- Workload plan JSON and Markdown generated
- If executed: all phases complete without abort
- If executed: `readyz/deep` success rate ≥ 99%
- If executed: queue depth ≤ 100 sustained
- If executed: drift checks show rate-limit gauges within expected bounds

**Abort criteria:**
- Load generator aborts due to config drift
- `readyz/deep` success rate < 95%
- Queue backlog > 100 sustained
- Target returns non-2xx HTTP codes during workload (exclude script output bugs)

**Known script artifact (2026-05-17)**: The workload generator may crash during Markdown serialization of readyz probes with `KeyError: 'probe_number'`. This is a script-side output bug, not a service failure. Verify by checking actual HTTP response codes and `readyz/deep` status independently.

---

### Gate L5 — Backup & Restore Verification

**Dry-run / plan (default):**
```bash
# Review backup configuration only
python3 scripts/validate_bridge_readiness.py --plan \
  --check-backup-config
```

**Live execution (requires operator signoff + target-host SSH):**
```bash
# SSH to target host
export VM_NAME="ferrumgate-nonprod"
export ZONE="asia-southeast1-a"
gcloud compute ssh "$VM_NAME" --zone="$ZONE" -- \
  "sudo -u ferrumgate /opt/ferrumgate/ferrumctl backup verify --db-path /var/lib/ferrumgate/ferrumgate.db"
```

> **Runbook drift note (2026-05-17)**: The original runbook used `--store-path` and `ferrumctl` without full path. The actual CLI flag is `--db-path`, and the binary is at `/opt/ferrumgate/ferrumctl`. The command above reflects the corrected form verified live.

**Evidence criteria:**
- `ferrumctl backup verify` returns exit 0
- Most recent backup file exists and is within RPO window
- systemd timer or cron is active

**Abort criteria:**
- Backup verify fails
- No backup file within RPO window
- Scheduler not active

---

## Remaining Blockers with Owners

| Blocker | Owner | Status | Next Action |
|---------|-------|--------|-------------|
| **Block A — Real owned domain** | Operator | WAIVED/CONDITIONAL | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **Block B — SendGrid API key rotation** | Operator | CLOSED | Completed 2026-05-17; see `2026-05-17-sendgrid-rotation-evidence.md` |
| **Block B — Escalation matrix acknowledgment** | Operator | CLOSED | Acknowledged 2026-05-17; SMS/webhook deferred outside current pilot scope |
| **Block C — Keyless backup** | Operator + Engineering | CLOSED | No further action |
| **G3.6 real workload (full acceptance)** | Operator | CONDITIONAL | Bounded L4 passed; full sustained workload acceptance remains deferred |
| **Path 2 full G2 signoff** | Operator | NOT COMPLETE | Requires Block A closure or conditional waiver + operator signoff |

---

## Approval-Gated Execution Flow

```text
Local pre-flight (L0) ──► Plan generation (dry-run) ──► Operator review ──► Operator signoff ──► Live execution (L1-L5)
        │                         │                           │                    │
        ▼                         ▼                           ▼                    ▼
   Any check FAIL           Any plan issue            Operator rejects      Any gate FAIL
   → STOP, fix locally      → STOP, fix plan          → STOP, remain        → STOP, rollback
                                                         in plan mode        and investigate
```

**Rules:**
1. Never skip dry-run/planning phase.
2. Never execute live without operator signoff.
3. Never commit secrets or tokens.
4. If any gate fails, stop and investigate before proceeding.
5. Production-ready remains NO until all gates pass AND operator signs full G2.

---

## Cross-References

| Document | Purpose |
|----------|---------|
| [`2026-05-17-operator-unblock-packet.md`](./2026-05-17-operator-unblock-packet.md) | Detailed Block A/B procedures and evidence gates |
| [`2026-05-17-bridge-l1-l3-live-evidence.md`](./2026-05-17-bridge-l1-l3-live-evidence.md) | Bridge L1–L3 live validation evidence (L1/L3 PASS, L2 PASS after remediation) |
| [`2026-05-17-bridge-l4-l5-live-evidence.md`](./2026-05-17-bridge-l4-l5-live-evidence.md) | Bridge L4–L5 live validation evidence (L4 bounded PASS, L5 backup verify PASS) |
| [`67-production-readiness-roadmap.md`](../67-production-readiness-roadmap.md) | Priority context and blocker ownership |
| [`122-completion-roadmap-and-hardening-tracker.md`](../122-completion-roadmap-and-hardening-tracker.md) | 10-item completion tracker |
| [`54-operator-signoff-packet.md`](../54-operator-signoff-packet.md) | Formal G2 signoff form |
| [`scripts/run_real_workload_generator.py`](../../../scripts/run_real_workload_generator.py) | G3.6 workload generator (safe-by-default, plan mode default) |
| [`scripts/validate_bridge_readiness.py`](../../../scripts/validate_bridge_readiness.py) | Bridge readiness validation script (this toolkit's companion) |

---

## Non-Claims

- **NOT production-ready**: This toolkit does not make FerrumGate production-ready.
- **NOT a substitute for operator judgment**: All live actions require explicit operator signoff.
- **NOT a guarantee of target-host success**: Local passes do not imply live target-host passes.
- **NOT PostgreSQL/HA/multi-node**: Single-node SQLite only.

---

*Toolkit created: 2026-05-17. Bridge-to-live validation runbook — planning artifact only. No execution claimed.*
