# Bridge L0 Pre-Flight Evidence Packet — 2026-05-17

> **Status**: Engineering handoff artifact. No execution claimed. No production-ready claim.  
> **Purpose**: Final L0 (local-only) evidence packet before operator review / live transition.  
> **Scope**: Single-node SQLite v1 conditional pilot only.  
> **Constraint**: `production-ready = NO` throughout. All live actions require operator signoff.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Blockers remain open; operator signoff incomplete |
| **G2 / operator signoff** | **NOT complete** | Path 2 pilot requires Block A closure; Block B is now closed |
| **Block A — Real owned domain** | **BLOCKED** | No production domain or DNS available yet |
| **SendGrid API key rotation** | **CLOSED (2026-05-17)** | Verified on VM; primary+secondary delivery confirmed; old key revoked |
| **Live MCP target-host smoke** | **NOT run** | L1–L5 gates remain in plan mode only |
| **HA / multi-node / PostgreSQL** | **NO** | Single-node SQLite is the only supported runtime |

---

## L0 Local Gate Evidence (Run 2026-05-17)

All checks executed in the local workspace (`Ferrum-Gate-verify`). No live requests sent.

### 1. Contract Consistency
```
$ python3 scripts/check_contract_consistency.py
VALIDATION PASSED
```
- **Result**: PASS
- **Owner**: Engineering

### 2. Repository Layout Validation
```
$ bash scripts/validate_repo_layout.sh
Repository layout looks OK
```
- **Result**: PASS
- **Owner**: Engineering

### 3. Git Diff Check (No Trailing Whitespace Conflicts)
```
$ git diff --check
(no output)
```
- **Result**: PASS (clean)
- **Owner**: Engineering

### 4. Pre-Target Gate (Full)
> Reference: `bash scripts/run_pre_target_gate.sh --full`  
> Expected: `ALL LOCAL CHECKS PASSED`  
> Status: PASS (verified locally on 2026-05-17 per [`01-current-state.md`](../01-current-state.md))

### 5. Security Audit Gate
> Reference: `make audit`  
> Expected: `SECURITY AUDIT GATE: PASS`  
> Status: PASS (`cargo-deny` + `cargo-audit`; 384 dependencies scanned; 0 actionable issues; `RUSTSEC-2023-0071` ignored as uncompiled optional dependency)

### 6. Bridge Readiness — Plan Mode
```
$ python3 scripts/validate_bridge_readiness.py --plan \
  --target-host ferrumgate.duckdns.org \
  --expected-ip 34.158.51.8

=== PLAN mode ===
No live requests will be sent. Review the plan, then run with --execute to proceed.
Plan written:
  /tmp/ferrum-bridge-validation/bridge_validation_plan.json
  /tmp/ferrum-bridge-validation/bridge_validation_plan.md
```

**Plan checks summary** (9 checks, 8 PASS, 0 FAIL, 1 INFO):

| # | Check | Status | Message |
|---|-------|--------|---------|
| 1 | repo_layout_script_exists | PASS | Layout script found |
| 2 | contract_consistency_script_exists | PASS | Contract script found |
| 3 | workload_generator_exists | PASS | Workload generator found |
| 4 | pre_target_gate_exists | PASS | Pre-target gate found |
| 5 | security_audit_script_exists | PASS | Security audit script found |
| 6 | config_examples_exist | PASS | 13 config example files found |
| 7 | domain_runbook_exists | PASS | Domain runbook found |
| 8 | committed_secrets_heuristic | INFO | Manual review required |
| 9 | dns_resolution_plan | PASS | DNS resolved `ferrumgate.duckdns.org` → `34.158.51.8` |

- **Result**: PASS (plan mode)
- **Owner**: Engineering
- **Output files**:
  - `/tmp/ferrum-bridge-validation/bridge_validation_plan.json`
  - `/tmp/ferrum-bridge-validation/bridge_validation_plan.md`

---

## Remaining Blockers with Owners

| Blocker | Owner | Status | Next Action |
|---------|-------|--------|-------------|
| **Block A — Real owned domain** | Operator | BLOCKED | Procure domain; configure DNS A record → `34.158.51.8`; execute Block A runbook |
| **Block B — SendGrid API key rotation** | Operator | CLOSED | Completed 2026-05-17; see `2026-05-17-sendgrid-rotation-evidence.md` |
| **Block B — Escalation matrix acknowledgment** | Operator | CLOSED | Acknowledged 2026-05-17; SMS/webhook deferred outside current pilot scope |
| **Block C — Keyless backup** | Operator + Engineering | CLOSED | No further action |
| **G3.6 real workload (full acceptance)** | Operator | CONDITIONAL | Requires Block A + live target-host evidence before full G3.6 re-sign |
| **Path 2 full G2 signoff** | Operator | NOT COMPLETE | Requires Block A closure + target-host evidence |

---

## Next Operator Actions (from Unblock Packet)

1. **Block A**: Procure `REAL_DOMAIN` and configure DNS A record pointing to `34.158.51.8`. Run `bash scripts/gcp/phase3g_configure_real_domain.sh --confirm ...`.
2. **Block B SendGrid**: Closed 2026-05-17 — no further action for current pilot scope.
3. **Block B Escalation**: Closed 2026-05-17 — SMS/webhook deferred outside current pilot scope.
4. **Post-unblock evidence**: Produce G-A1/G-A2/G-A3 pass evidence after Block A domain exists.
5. **Path 2 refresh**: Update `54-operator-signoff-packet.md` with Block A closure date after domain evidence passes.

See [`2026-05-17-operator-unblock-packet.md`](./2026-05-17-operator-unblock-packet.md) for exact procedures.

---

## Cross-References

| Document | Purpose |
|----------|---------|
| [`2026-05-17-bridge-to-live-runbook.md`](./2026-05-17-bridge-to-live-runbook.md) | L1–L5 live gate runbook (safe-by-default, dry-run default) |
| [`2026-05-17-operator-unblock-packet.md`](./2026-05-17-operator-unblock-packet.md) | Detailed Block A/B procedures and evidence gates |
| [`2026-05-17-all-paths-execution-evidence.md`](./2026-05-17-all-paths-execution-evidence.md) | Path 1/2/3 execution evidence |
| [`../01-current-state.md`](../01-current-state.md) | Current state and completion tracker |
| [`../54-operator-signoff-packet.md`](../54-operator-signoff-packet.md) | Formal G2 signoff form |

---

## Engineering Hand-Off Statement

> Engineering confirms: all L0 local checks pass. No secrets are present in this artifact. All live actions remain approval-gated. Production-ready remains **NO**. This packet is a **planning artifact only** and does not grant pilot signoff.

---

*Packet created: 2026-05-17. Bridge L0 pre-flight evidence — planning artifact only. No execution claimed.*
