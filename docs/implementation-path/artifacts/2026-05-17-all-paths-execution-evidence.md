# 2026-05-17 — All Paths Execution Evidence

> **Status**: Evidence-only. No production-ready claim. No operator signoff completion claimed.
> **Scope**: Paths 1/2/3 execution as far as reasonably completed on 2026-05-17.
> **Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate`

---

## Executive Summary

| Path | Status | Evidence | Blockers |
|---|---|---|---|
| **Path 1 — RC Tag** | ✅ Executed | `v0.1.0-rc.2` cut at `e229f76`; GitHub prerelease published; G1 gates fresh-pass | None for RC scope |
| **Path 2 — Safe Probes** | ✅ Safe probes executed | `check_pilot_readiness.py` shallow/deep/metrics PASS against duckdns; bridge L1/L3 live PASS; L2 PASS after root-cause remediation (missing `store_dsn` → in-memory SQLite, fixed on VM); **does NOT complete G2/operator signoff** | Block A WAIVED/CONDITIONAL for single-node SQLite pilot only; SendGrid rotation CLOSED 2026-05-17; escalation acknowledgment CLOSED 2026-05-17 |
| **Path 3 — Local Prep** | ✅ Local plan generated; MCP smoke passed | Workload plan 3360 requests (no live traffic); local MCP lifecycle smoke 15 passed | T3.4+ target-host deployment/live validation pending |
| **Production-ready** | ❌ NO | — | All of the above |

---

## Path 1 — RC Release (v0.1.0-rc.2)

### Tag Evidence

| Field | Value |
|---|---|
| Tag | `v0.1.0-rc.2` |
| Target commit | `e229f767fccf86fd441b7ca2ac5e9756b07a254b` |
| Commit message | `docs: prepare v0.1.0-rc.2 release` |
| GitHub prerelease URL | `https://github.com/BrianNguyen29/Ferrum-Gate/releases/tag/v0.1.0-rc.2` |
| Is prerelease | `true` |

### G1 Gates (Fresh — Executed Immediately Before Tag)

| Gate | Command | Result |
|---|---|---|
| G1.1 | `cargo check --workspace` | PASS |
| G1.2 | `cargo fmt --all -- --check` | PASS |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| G1.4 | `cargo test --workspace` | PASS |
| G1.5 | `python3 scripts/generate_rc_evidence.py` | Overall ALL PASS |
| G1.6 | `bash scripts/validate_repo_layout.sh` | Repository layout looks OK |
| G1.7 | `python3 scripts/check_contract_consistency.py` | VALIDATION PASSED |
| G1.8 | `bash scripts/run_pre_target_gate.sh --full` | ALL LOCAL CHECKS PASSED |

> **Note**: G1 gates were run fresh immediately before tagging. No code changes were made after gate execution.

---

## Path 2 — Safe Pilot Probes (Against DuckDNS Target)

### Probe Execution

| Field | Value |
|---|---|
| Command | `python3 scripts/check_pilot_readiness.py --server-url https://ferrumgate.duckdns.org --skip-functional` |
| Target | `https://ferrumgate.duckdns.org` |
| Functional probes | SKIPPED (`--skip-functional`) |

### Results

| Probe | Status |
|---|---|
| `shallow_readiness` | PASS |
| `deep_readiness` | PASS |
| `functional_readiness` | SKIPPED |
| `metrics_endpoint` | PASS |
| **Overall** | **ALL PROBES PASSED** |

### What This Does NOT Mean

- **Does NOT complete G2**. Operator signoff (G2.1–G2.8) remains pending.
- **Does NOT verify production workload fit**.
- **Does NOT verify backup/restore, compensation drills, or TLS/reverse-proxy configuration**.
- No bearer token or secrets were printed in probe output.

---

## Path 3 — Local Plan + MCP Smoke

### Workload Plan Generation

| Field | Value |
|---|---|
| Command | `python3 scripts/run_real_workload_generator.py --plan --server-url https://ferrumgate.duckdns.org --output-dir /tmp/opencode/ferrum-t3-plan-20260517` |
| Output files | `/tmp/opencode/ferrum-t3-plan-20260517/workload_plan.json`, `.md` |
| Estimated total requests | 3360 |
| Live requests sent | **0** ( `--plan` mode only) |

### MCP Lifecycle Smoke

| Field | Value |
|---|---|
| Command | `bash scripts/run_mcp_lifecycle_smoke.sh --help` |
| Note | Script ignores `--help` and runs smoke locally |
| Results | 15 passed, 0 failed |
| Overall | **MCP LIFECYCLE SMOKE ALL CHECKS PASSED** |

### What This Does NOT Mean

- **Does NOT validate target-host performance under real load**.
- **Does NOT complete T3.4+ target-host deployment**.
- Local smoke only; no production-ready claim.

---

## Remaining Blockers (Explicit)

| Blocker | Status | Owner | Notes |
|---|---|---|---|
| **Block A — Real owned domain / DNS** | WAIVED/CONDITIONAL | Operator | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **SendGrid API key rotation** | CLOSED (2026-05-17) | Operator | Verified on VM; primary+secondary delivery confirmed; old key revoked |
| **Escalation matrix operator acknowledgment** | CLOSED (2026-05-17) | Operator | Primary+secondary paths formally acknowledged; SMS/webhook deferred outside current pilot scope |
| **T3.4+ target-host deployment / live validation** | Pending | Engineering | Workload plan generated; live execution gated by Block A WAIVED/CONDITIONAL status |
| **Live target-host MCP smoke / load test** | Pending | Engineering | Requires real domain + operator signoff |
| **Production-ready claim** | **NO** | — | Requires all G2/G3 gates + operator signoff + live validation |

---

## Cross-References

| Document | Purpose |
|---|---|
| [`31-release-paths-todo.md`](./31-release-paths-todo.md) | Path 1/2/3 decision framework |
| [`53-rc-tag-checklist.md`](./53-rc-tag-checklist.md) | RC tag checklist |
| [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) | Operator signoff template (G2) |
| [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | Pilot readiness evidence template |
| [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Path 2 ordered execution checklist |
| [`2026-05-17-bridge-l1-l3-live-evidence.md`](./2026-05-17-bridge-l1-l3-live-evidence.md) | Bridge L1–L3 live validation evidence (L1/L3 PASS, L2 PASS after root-cause remediation; historical initial state was partial/blocked) |
| [`67-production-readiness-roadmap.md`](./67-production-readiness-roadmap.md) | Production readiness roadmap |
| [`122-completion-roadmap-and-hardening-tracker.md`](./122-completion-roadmap-and-hardening-tracker.md) | Completion tracker |

---

*Artifact generated: 2026-05-17. Evidence-only — no production-ready claim.*
