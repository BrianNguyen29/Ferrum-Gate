# Bridge L4–L5 Live Evidence — 2026-05-17

> **Status**: Live validation evidence. No production-ready claim. No full G2 completion claimed.  
> **Purpose**: Record L4 bounded workload and L5 backup verification results after L2 remediation.  
> **Scope**: Single-node SQLite v1 conditional pilot only.  
> **Constraint**: `production-ready = NO` throughout. L4/L5 evidence does not complete G2 signoff.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Blockers remain open; operator signoff incomplete |
| **G2 / operator signoff** | **NOT complete** | Path 2 pilot requires Block A closure or conditional waiver plus operator signoff |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **L4 full workload acceptance** | **NO** | Bounded L4 only; full G3.6 real workload acceptance remains conditional |
| **HA / multi-node / PostgreSQL** | **NO** | Single-node SQLite is the only supported runtime |

---

## Live Execution Context

- **Target host**: `ferrumgate.duckdns.org`
- **Expected IP**: `34.158.51.8`
- **Execution date**: `2026-05-17`
- **Operator IP during work**: `1.55.106.164`
- **Latest commit at time of work**: `597ae61 docs: record L2 auth recovery evidence`

> **Note**: All live commands were executed by the operator or in an authorized environment. This artifact records results only; no live commands were executed during artifact creation. No bearer token values were printed, logged, or stored.

---

## SSH / Firewall Access

To enable on-VM investigation and L4/L5 execution, the SSH firewall rule was temporarily modified:

| Step | Firewall Rule | Source Ranges |
|------|---------------|---------------|
| Before work | `ferrumgate-nonprod-fw-ssh` | `["118.69.4.63/32"]` |
| During work | `ferrumgate-nonprod-fw-ssh` | `["118.69.4.63/32", "1.55.106.164/32"]` |
| After work | `ferrumgate-nonprod-fw-ssh` | `["118.69.4.63/32"]` (restored and verified) |

---

## Config Drift Remediation (Pre-L4/L5)

Before L4/L5 execution, config drift from the L2 remediation was confirmed stable:

- `/etc/ferrumgate/ferrumgate.toml` contains `[server] store_dsn = "sqlite:///var/lib/ferrumgate/ferrumgate.db"`
- Root cause: `ferrumd` config precedence reads `store_dsn` from `[server]`, env, CLI, or default. A `[store]` section with `dsn` is not parsed for this key.
- `FERRUMD_STORE_DSN` env var is unset.
- Database file exists at `/var/lib/ferrumgate/ferrumgate.db` with ownership `ferrumgate:ferrumgate` and mode `640`.
- `PRAGMA integrity_check` returns `ok`.

> See [`2026-05-17-bridge-l1-l3-live-evidence.md`](./2026-05-17-bridge-l1-l3-live-evidence.md) §L2 Recovery for full root-cause and remediation details.

---

## L4 — Bounded Workload Generator Readiness (LIVE)

### L4 First Run (Script Artifact Bug)

**Output directory**: `/tmp/ferrum-l4-bounded-20260517`

**Results**:

| Signal | Value |
|--------|-------|
| Estimated requests | ~50 |
| Target phase (20 requests) | 20/20 HTTP 200 |
| Spike phase (31 requests) | 31/31 HTTP 200 |
| Drift checks | OK — gauges `{rate_limit_per_second: 2.0, rate_limit_burst: 50.0}` |
| Post-workload readyz probes | HTTP 200 |

**Anomaly**: The script crashed while writing readyz probe Markdown due to a `KeyError: 'probe_number'`. This was caused by a mid-run record shape mismatch in the script's output serialization, **not** a service failure. Service remained healthy throughout.

**Assessment**: Treat as a **script artifact bug**, not a service failure. All actual HTTP responses from the target were successful.

---

### L4 Clean Rerun (PASS)

**Output directory**: `/tmp/ferrum-l4-bounded-rerun-20260517`

**Parameters**:

| Phase | Duration | Rate |
|-------|----------|------|
| Baseline | 3s | — |
| Target | 10s | 1 rps |
| Spike | 5s | 2 rps |
| Cooldown | 3s | — |

**Results**:

| Signal | Value |
|--------|-------|
| Estimated requests | ~20 |
| Target phase | 10/10 HTTP 200 |
| Spike phase | 10/10 HTTP 200 |
| Drift checks | OK — gauges `{rate_limit_per_second: 2.0, rate_limit_burst: 50.0}` |
| Post-workload readyz probes | 3/3 HTTP 200 |
| Post-workload deep readyz | 3/3 HTTP 200 |
| Exit code | 0 |

**Output files**:
- `workload_plan.json` / `workload_plan.md`
- `workload_results.json` / `workload_results.md`
- `readyz_probe_*.json` / `readyz_probe_*.md`

- **L4 overall**: **PASS**
- **Owner**: Engineering / Operator

---

## L5 — Backup & Restore Verification (LIVE)

### Runbook Drift Correction

The runbook (`2026-05-17-bridge-to-live-runbook.md`) initially documented:

```bash
# INCORRECT — do not use
sudo ferrumctl backup verify --store-path /var/lib/ferrumgate/ferrumgate.db
```

Two issues were found during live execution:
1. `ferrumctl` is not in the default `PATH` for the `ferrumgate` user; the binary is at `/opt/ferrumgate/ferrumctl`.
2. The actual CLI flag is `--db-path`, not `--store-path`.

**Corrected command**:
```bash
sudo -u ferrumgate /opt/ferrumgate/ferrumctl backup verify --db-path /var/lib/ferrumgate/ferrumgate.db
```

### Live Execution Results

**Command executed**:
```bash
sudo -u ferrumgate /opt/ferrumgate/ferrumctl backup verify --db-path /var/lib/ferrumgate/ferrumgate.db
```

**Results**:

| Signal | Value |
|--------|-------|
| `backup verify` output | `OK` |
| Integrity check message | `Database integrity check passed: /var/lib/ferrumgate/ferrumgate.db` |
| Exit code | 0 |
| `ferrumgate.service` status | active |
| Backup timer status | active |
| Latest backup observed | `/var/lib/ferrumgate/backups/ferrumgate_20260513_163232.db` |
| Offsite sync script | present |

- **L5 overall**: **PASS**
- **Owner**: Engineering / Operator

---

## Combined L4/L5 Summary

| Level | Checks | Status |
|-------|--------|--------|
| L4 — Bounded Workload Generator | Target + spike HTTP 200, drift OK, readyz 3/3 | **PASS** |
| L5 — Backup Verification | `ferrumctl backup verify` OK, integrity passed, timer active, backup present, offsite script present | **PASS** |

---

## Remaining Blockers with Owners

| Blocker | Owner | Status | Next Action |
|---------|-------|--------|-------------|
| **Block A — Real owned domain** | Operator | WAIVED/CONDITIONAL | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **Path 2 full G2 signoff** | Operator | NOT COMPLETE | Requires Block A closure or conditional waiver + operator signoff |
| **Production-ready claim** | — | **NO** | Requires all G2/G3 gates + operator signoff + live validation + real domain |

---

## Cross-References

| Document | Purpose |
|----------|---------|
| [`2026-05-17-bridge-to-live-runbook.md`](./2026-05-17-bridge-to-live-runbook.md) | L1–L5 live gate runbook (safe-by-default, dry-run default) |
| [`2026-05-17-bridge-l1-l3-live-evidence.md`](./2026-05-17-bridge-l1-l3-live-evidence.md) | Bridge L1–L3 live evidence (L1/L3 PASS, L2 PASS after remediation) |
| [`2026-05-17-all-paths-execution-evidence.md`](./2026-05-17-all-paths-execution-evidence.md) | Path 1/2/3 execution evidence |
| [`../01-current-state.md`](../01-current-state.md) | Current state and completion tracker |
| [`../54-operator-signoff-packet.md`](../54-operator-signoff-packet.md) | Formal G2 signoff form |

---

## Operator / Engineering Review Statement

> This artifact accurately records live L4 and L5 validation results as of 2026-05-17. L4 bounded workload passed on clean rerun after a script artifact bug on the first run. L5 backup verification passed after correcting runbook command drift (`--store-path` → `--db-path`, `ferrumctl` → `/opt/ferrumgate/ferrumctl`). SSH firewall was restored after work. No secrets or token values are present in this artifact. Production-ready remains **NO**. Full G2 operator signoff remains **NOT COMPLETE**.

---

*Artifact created: 2026-05-17. Bridge L4–L5 live evidence — records observed results only. No production-ready claim.*
