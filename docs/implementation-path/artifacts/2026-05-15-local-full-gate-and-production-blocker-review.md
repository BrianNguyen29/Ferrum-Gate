# 2026-05-15 — Local Full Gate and Production Blocker Review

> **Status**: Audit-trail artifact. No execution claimed. No production-ready claim.
> **Purpose**: Record CI cost preference, local full gate evidence, and production-readiness blocker review.
> **Scope**: Documentation only. No live infra changes. No secrets.
> **Constraint**: `production-ready = NO` throughout.

---

## 1. CI Cost Preference

- **Decision**: GitHub-hosted CI was not triggered/used for this private repository due to GitHub Actions minutes cost preference.
- **Rationale**: GitHub-hosted CI for private repositories consumes Actions minutes; the operator prefers a local validation / self-hosted / manual-only approach.
- **Accepted approach**: Local `run_pre_target_gate.sh --full`, plus manual `cargo check`, `cargo clippy`, `cargo test`, and layout/contract validation.
- **Future option**: Self-hosted runner or manual-only CI pipeline can be evaluated later if cost model changes.
- **Reference**: `docs/implementation-path/70-security-hardening-local-only-plan.md` for local-only security audit commands.

---

## 2. Local Full Gate Evidence

- **Command executed**: `bash scripts/run_pre_target_gate.sh --full`
- **Run by**: Orchestrator
- **Date**: 2026-05-15
- **Result**: `ALL LOCAL CHECKS PASSED`
- **Checks included**:
  - `cargo fmt --all -- --check`
  - `cargo check --workspace`
  - `ferrumctl` smoke (binary functional)
  - Config examples validation
  - Local restore drill (temp SQLite)
  - Evidence skeleton generator valid
  - Required Path 2 docs present
  - Required config examples present
  - Local bearer-auth smoke
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
- **Evidence quality**: Local-only; does not constitute target-host or production signoff.

---

## 3. Production Blocker Review

> **Scope**: Single-node SQLite v1 conditional pilot only.
> **Status**: B1/B2/B3/B4/B5 closed/accepted/delegated-closed. New active blockers identified below.
> **No production-ready claim**.

### 3.1 Active Operator-Action Blockers (Sequenced)

| # | Blocker | Owner | Evidence Gate | Sequencing |
|---|---------|-------|---------------|------------|
| 1 | **Real owned domain** | Operator | DuckDNS is not a production-owned domain; operator must procure and configure a production domain | P0 — before any external exposure |
| 2 | **Off-VM alerting / external notification** | Operator | Alerting must reach an off-VM channel (email/SMS/pager) with confirmed delivery | P0 — before unattended operation |
| 3 | **Keyless backup / VM OAuth scope blocker** | Operator | Backup storage must not rely on VM-instance OAuth scopes; use service-account key or workload identity with scoped storage permissions; **or** operator explicitly accepts key-based backup risk | P0 — before production data volume |

### 3.2 Settled Decisions (Not Blockers)

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **PostgreSQL production deployment** | **NO** | Remains deferred unless explicitly selected by operator; Path 3 scope |
| **HA / multi-node** | **NO / out of v1 scope** | Not implemented; single-node SQLite only |
| **CI cost / local gate** | **Accepted** | GitHub-hosted Actions minutes avoided; local `run_pre_target_gate.sh --full` and manual validation are the accepted approach; self-hosted runner is a future option |

### 3.3 Blocker Closure Status

| Blocker | ID | Status | Evidence |
|---------|----|--------|----------|
| D1–D6 target-host drills | B1 | ✅ Passed 2026-05-13 | `artifacts/2026-05-13-d1-d6-target-host-evidence.md` |
| Restore drill | B2 | ✅ Passed 2026-05-15 | `artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md` |
| Backup automation / retention pruning | B3 | ✅ Closed via delegated authority 2026-05-15 | Run id `20260515T1606Z-b3-retention`; `artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md` |
| TLS / reverse proxy | B4 | ☑ Closed via delegated authority 2026-05-15 | `artifacts/2026-05-15-b3-b4-b5-delegated-signing-status.md` |
| Bearer token | B5 | ☑ Closed via delegated authority 2026-05-15 | `artifacts/2026-05-15-b3-b4-b5-delegated-signing-status.md` |
| P5c.V1/V2 (PG) | B6–B7 | ☐ N/A | SQLite path selected; waived per `113-operator-path-selection-packet.md` §6 |
| G3.6 real workload | B8 | ☑ Full acceptance (P5b review only) | `106-g3-6-pilot-metrics-evidence-packet.md` |

---

## 4. Stale Text Fixes Applied

| Location | Stale Text | Current Text |
|----------|-----------|--------------|
| `67-production-readiness-roadmap.md` P0.4 | "retention pruning not verified" | "retention pruning verified with run id `20260515T1606Z-b3-retention`" |
| `67-production-readiness-roadmap.md` Consolidated Blocker Summary | "Backup automation — 🟡 Partial — retention pruning not verified" | "Backup automation — ✅ Done — retention pruning closed via delegated authority with run id `20260515T1606Z-b3-retention`" |
| `scripts/run_pre_target_gate.sh` final output | "remaining target gaps (B3-B5) are operator-owned" | "B3/B4/B5 are CLOSED via delegated authority on 2026-05-15. No production-ready claim. FerrumGate v1 remains RC-ready/conditional." |

---

## 5. Cross-References

| Artifact | Purpose |
|----------|---------|
| `docs/implementation-path/67-production-readiness-roadmap.md` | Authoritative production-readiness roadmap with updated P0.4 and production blocker review |
| `docs/implementation-path/112-post-p5c-completion-execution-plan.md` | Post-P5c plan with local full gate evidence note |
| `scripts/run_pre_target_gate.sh` | Local gate script with updated final next-steps wording |
| `artifacts/2026-05-13-d1-d6-target-host-evidence.md` | B1 D1–D6 target-host drill pass evidence |
| `artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md` | B2 restore drill success evidence |
| `artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md` | Historical B3/B4/B5 partial evidence (superseded by 2026-05-15 delegated closure) |
| `artifacts/2026-05-15-b3-b4-b5-delegated-signing-status.md` | B3/B4/B5 delegated signing status |
| `docs/implementation-path/70-security-hardening-local-only-plan.md` | Local-only security audit commands |

---

*Artifact created: 2026-05-15. Local Full Gate and Production Blocker Review — audit trail only. No execution claimed. No production-ready claim.*
