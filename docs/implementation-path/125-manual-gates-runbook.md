# 125 — Manual Gates Runbook

> **Scope**: CI-hosted manual validation gates triggered on-demand via GitHub Actions.
> **Not**: automatic CI, target-host operations, or production-ready certification.

## Overview

The repository includes a manual-only workflow at `.github/workflows/manual-gates.yml`.
It is triggered exclusively through `workflow_dispatch` (GitHub UI or API) and has **no**
`push` or `pull_request` triggers. This prevents burning CI minutes on expensive or
long-running checks while keeping the gates available when an operator or maintainer
explicitly chooses to run them.

## What This Is

- A **voluntary, on-demand** validation suite for local/CI-hosted checks
- A **cost-conscious** alternative to running heavy gates on every commit
- A **conservative** tool: running a gate does **not** change production-ready status,
  full G2 status, or Block A status

## What This Is Not

- **Not automatic CI**: It will not run on pushes or PRs
- **Not target evidence**: No real target host, domain, or live credentials are used
- **Not production-ready certification**: Passing a gate does not authorize pilot or claim production readiness
- **Not G2 completion**: G2.1–G2.8 remain pending operator action regardless of gate results
- **Not Block A closure**: Block A remains WAIVED/CONDITIONAL

## Cost Warning

Triggering this workflow consumes GitHub Actions runner minutes. If your organization
has limited Actions quota, use these gates sparingly:

- Prefer local execution (`make validate`, `make pretarget`, `bash scripts/run_mcp_lifecycle_smoke.sh`)
- Use the CI workflow only when you need CI-hosted artifacts or need to validate on a clean runner
- The `all` choice runs every gate sequentially and can take 15–30 minutes

## Gate Choices

| Choice | What it covers | Typical duration | When to run |
|--------|---------------|------------------|-------------|
| `audit` | `cargo-deny` + `cargo-audit` security scan | 2–5 min | Before releases, after dependency bumps |
| `pretarget` | Config validation, restore drill, evidence skeleton, bearer-auth smoke | 3–5 min | Before operator handoff, after config changes |
| `wal-drill` | SQLite WAL crash-recovery drill | 2–3 min | After store/rollback changes, before backup validation |
| `mcp-smoke` | MCP stdio transport, lifecycle tool wiring, blocked-tool behavior | 5–10 min | After MCP server changes, before D1 stage gates |
| `all` | Runs every gate above in sequence | 15–30 min | Before major milestones, RC tags, or operator review |

## How to Trigger

1. Open the repository on GitHub
2. Go to **Actions** → **manual-gates**
3. Click **Run workflow**
4. Select the desired gate from the dropdown
5. Click **Run workflow**

## Local Equivalents

Most gates can be run locally without CI:

```bash
# Layout + contract consistency
make validate

# Pre-target gate
make pretarget

# Security audit
make audit

# WAL crash-recovery drill
make wal-drill

# MCP lifecycle smoke (builds ferrumd and ferrum-mcp-server locally)
bash scripts/run_mcp_lifecycle_smoke.sh

# Required-tools regression (no live services needed)
bash scripts/validate_mcp_required_tools.sh
```

## Artifact Retention

Artifacts uploaded by `manual-gates` are retained for **30 days** by default.
They contain logs and evidence files but **no secrets or live credentials**.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| Gate fails on `cargo-deny` | Advisory DB outdated | Run locally with `make audit` and update DB |
| MCP smoke cannot find ferrumd | Binary not built | Script auto-builds; check disk space and Rust toolchain |
| WAL drill times out | SQLite busy | Close other connections to the test DB |

## Related Documents

- `.github/workflows/manual-gates.yml` — workflow definition
- `AGENTS.md` — high-level toolchain and verification status
- `docs/implementation-path/62-path-2-operator-runbook.md` — operator-target runbook
- `docs/implementation-path/67-production-readiness-roadmap.md` — P0/P1 blockers and owners

## Change Log

| Date | Change |
|------|--------|
| 2026-05-18 | Document created after manual-gates workflow hardening |
