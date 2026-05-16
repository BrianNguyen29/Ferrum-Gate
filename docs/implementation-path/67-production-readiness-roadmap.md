# 67 — Production-Readiness Roadmap

> **Status**: In-tree documentation. Bounded todo list for reaching RC-ready/conditional posture.
> **Purpose**: Durable, complete roadmap of pre-production items with priorities, blockers, owners, evidence, and non-claims.
> **Scope**: Single-node SQLite v1 only. No PostgreSQL/multi-node/HA. No production-ready claim.
> **Constraint**: Do not claim G2 complete, do not sign doc59/doc54, do not authorize pilot.

---

## Purpose

This document is the authoritative production-readiness roadmap for FerrumGate v1 single-node SQLite.
It consolidates all pre-production blockers, hardening items, and operational readiness gaps into a
single prioritized list with owners, evidence requirements, and explicit non-claims.

**This is NOT a plan to reach "production-ready."** FerrumGate v1 is RC-ready/conditional.
Reaching full production posture requires operator signoff (Path 2 G2 gates) and optional
Phase 3 PostgreSQL (Path 3) — both are outside the scope of this roadmap.

---

## Explicit Non-Claims

- **No production-ready claim**: This roadmap does not make FerrumGate "production-ready."
  FerrumGate v1 is RC-ready/conditional. Full production posture requires Path 2 operator
  signoff and optional Phase 3 PostgreSQL.
- **No G2 complete beyond conditional pilot**: G2.1–G2.8 are signed for conditional single-node SQLite
  pilot only (BrianNguyen, 09/05/2026). They do **not** constitute full production signoff or PostgreSQL
  authorization.
- **No pilot authorized beyond conditional scope**: Conditional pilot is authorized per doc100.
  Full production pilot remains pending.
- **No PostgreSQL**: PostgreSQL/multi-node/HA is Path 3 — not in scope for Phase 1.
- **No target-host evidence**: All target-host execution evidence (D1–D6 drills, restore drill,
  probe evidence) requires operator action on target environment.
- **Do not sign doc59/doc54 on behalf of operator**: All signature fields remain blank.

---

## Priority Definitions

| Priority | Meaning |
|----------|---------|
| **P0** | Must fix before any production pilot. Blocking item that prevents bounded RC use. |
| **P1** | Should fix before production pilot. Affects operational posture or safety. |
| **P2** | Fix before production pilot if practical. Improves operability. |
| **P3** | Post-pilot or deferred. Not a pilot blocker. |

---

## P0 — Must-Fix Before Any Production Pilot

| # | Item | Owner | Evidence Required | Status |
|---|---|---|---|---|
| P0.1 | **CI must not swallow cargo check** | Engineering | CI pipeline runs `cargo check --workspace` without `\|\| true` | ✅ Done (CI hardened 2026-05-03) |
| P0.1b | **CI dependency scanning deferred** | N/A | Security scanning (cargo-deny, cargo-audit) not in CI due to cost; local/manual alternatives documented in `70-security-hardening-local-only-plan.md` | ✅ Done (doc only) |
| P0.2 | **Target-host execution evidence** | Operator | D1–D6 drill evidence on target host passed 6/6 on 2026-05-13; `readyz/deep` returns HTTP 200 on target | ☑ Passed 2026-05-13 (operator-owned; not full production signoff) |
| P0.3 | **Restore drill executed on target** | Operator | Restore drill log with `PRAGMA integrity_check` passing on target; fixed binary deployed; 0.463s restore | ☑ Passed 2026-05-15 (operator-owned; not full production signoff) |
| P0.4 | **Backup automation configured and verified** | Operator | External scheduler (cron/systemd timer) configured; `ferrumctl backup verify` passes; retention pruning verified with run id `20260515T1606Z-b3-retention` (old matching sentinel pruned, nonmatching preserved, new backup verified OK rc=0, service healthy) | ✅ Done (operator-owned; not full production signoff) |
| P0.5 | **G2.1–G2.8 signed for conditional pilot** | Operator | `59-pilot-readiness-evidence-packet.md` G2.1–G2.8 signed by BrianNguyen 09/05/2026 for conditional single-node SQLite pilot | ✅ Done (conditional pilot only; not full production) |
| P0.6 | **Operator signoff obtained for conditional pilot** | Operator | `54-operator-signoff-packet.md` signed by BrianNguyen 09/05/2026 | ✅ Done (conditional pilot only; not full production) |

### P0 Notes

- P0.1 is a repo-side blocker fixed by CI hardening (2026-05-03).
- P0.2–P0.3 remain **operator-owned** target-host blockers.
- P0.4 is **closed** via delegated authority with evidence run id `20260515T1606Z-b3-retention`.
- P0.5–P0.6 are signed for **conditional single-node SQLite pilot only** (BrianNguyen, 09/05/2026).
  They do **not** constitute full production signoff or PostgreSQL authorization.
- See [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) §Step 1–5 for the ordered
  execution checklist.
- See [`66-path-2-operator-handoff.md`](./66-path-2-operator-handoff.md) §Phase B for blockers.

---

## P1 — Should-Fix Before Production Pilot

| # | Item | Owner | Evidence Required | Status |
|---|---|---|---|---|
| P1.1 | **Readiness semantics: `/v1/readyz/deep` functional probe** | Engineering | Load balancers and Kubernetes should use `/v1/readyz/deep` as functional readiness probe; `/v1/healthz` and `/v1/readyz` are shallow and always return 200 | ✅ Done — documented in `PRODUCTION_NOTES.md` §Health and Readiness Endpoints; `/v1/readyz/deep` returns 200 when store healthy and write queue depth <= 100, 503 when store unhealthy or write queue depth > 100 |
| P1.2 | **Configurable rate limit** | Engineering | Rate limit configurable via CLI/env/config file (2 req/s, burst 50 default); operator confirms fit for target workload | ✅ Done — CLI: `--rate-limit-per-second` and `--rate-limit-burst`; env: `FERRUMD_RATE_LIMIT_PER_SECOND` and `FERRUMD_RATE_LIMIT_BURST`; config file: `rate_limit_per_second` and `rate_limit_burst` under `[server]` |
| P1.3 | **Structured logging (JSON)** | Engineering | Logs are unstructured text; production debugging and log aggregation benefit from JSON structured output | ✅ Done — CLI: `--log-format`; env: `FERRUMD_LOG_FORMAT`; config file: `log_format` under `[server]`; default is "text" (human-readable); accepted values: "text", "compact", "json"; documented in `PRODUCTION_NOTES.md` |
| P1.4 | **Full metrics/observability** | Engineering | `/v1/metrics` with method labels on request/governance counters and latency histograms for public endpoints | ✅ Done — `/v1/metrics` provides: request counters per endpoint with HTTP method labels (healthz, readyz, readyz/deep, metrics), bounded HTTP status labels for public endpoints (status="200" for healthz/readyz/metrics; status="200"/"503" for readyz/deep), store health gauge (`ferrumgate_store_health_up`), SQLite write queue depth gauge (`ferrumgate_write_queue_depth`), governance error counters per route with HTTP method labels (26 routes), governance success counters per route with HTTP method labels (26 routes), and latency histogram (`ferrumgate_request_duration_seconds`) for public endpoints with bounded labels (route, method, status, le) emitting _bucket/_sum/_count lines |
| P1.5 | **RPO/RTO formally accepted** | Operator | Backup/restore objectives formally accepted per `27-production-evaluation-plan.md` §Operator Signoff Packet §3 | ✅ Done (conditional pilot only; RPO=15min, RTO=15min) |
| P1.6 | **Compensate noop risk accepted** | Operator | Operator acknowledges compensate may be noop-backed for target adapters per G2.8 | ✅ Done (conditional pilot only) |

### P1 Notes

- P1.1–P1.4 are engineering items. P1.5–P1.6 are operator-owned and signed for conditional pilot only.
- P1.5: RPO/RTO accepted by BrianNguyen 09/05/2026 for conditional single-node SQLite pilot (15min/15min).
- P1.6: Compensate noop risk accepted by BrianNguyen 09/05/2026 for conditional single-node SQLite pilot.
- P1.2: Rate limiting is built-in via `tower_governor` with per-IP enforcement. Configurable via
  CLI flags (`--rate-limit-per-second`, `--rate-limit-burst`), environment variables
  (`FERRUMD_RATE_LIMIT_PER_SECOND`, `FERRUMD_RATE_LIMIT_BURST`), or config file fields
  (`rate_limit_per_second`, `rate_limit_burst` under `[server]`). Defaults remain 2 req/s and burst 50.
  Validation rejects 0 and values >10000 for burst. CLI > env > config file > defaults precedence.
- P1.1: `/v1/readyz/deep` is the functional readiness probe. Returns HTTP 200 when store is healthy
  AND write queue depth <= 100; returns HTTP 503 when store is unhealthy OR write queue depth > 100.
  Use for load balancers and Kubernetes readiness probes.
  `/v1/healthz` and `/v1/readyz` are shallow checks — always return 200, do NOT check store health.
- P1.3: Configurable log format via CLI (`--log-format`), env (`FERRUMD_LOG_FORMAT`), or config file
  (`log_format` under `[server]`). Default is "text" (human-readable). Accepted values: "text",
  "compact" (both are the same human-readable format), "json" (structured JSON for log aggregation).
  Config precedence: CLI > env > config file > defaults. Documented in `PRODUCTION_NOTES.md`.
- P1.4: `/v1/metrics` provides: request counters per endpoint with HTTP method labels (healthz,
  readyz, readyz/deep, metrics), bounded HTTP status labels for public endpoints (status="200" for
  healthz/readyz/metrics; status="200"/"503" for readyz/deep based on health),
  `ferrumgate_store_health_up` gauge, `ferrumgate_write_queue_depth` gauge for accepted SQLite write
  operations not yet processed by the writer loop, `ferrumgate_metrics_scrapes_total`,
  `ferrumgate_governance_errors_total` per route with HTTP method labels (26 governance endpoints), and
  `ferrumgate_governance_success_total` per route with HTTP method labels (26 governance endpoints).
  Latency histograms (`ferrumgate_request_duration_seconds`) implemented for public endpoints only
  with bounded labels (route, method, status, le). Governance route latency instrumentation
  is out of scope. HTTP status labels are implemented for public endpoints only
  (bounded change, per-handler instrumentation).
- See [`27-production-evaluation-plan.md`](./27-production-evaluation-plan.md) for the full
  production evaluation framework.

---

## P2 — Fix Before Production Pilot If Practical

| # | Item | Owner | Evidence Required | Status |
|---|---|---|---|---|
| P2.1 | **Adapter hardening beyond bounded slices** | Engineering | Target adapter surface verified for target workload; compensate behavior confirmed | 🟡 Partial — adapters have verified local slices; remaining surface is post-v1 |
| P2.2 | **`ferrumctl` expanded surface** | Engineering | `ferrumctl` includes health/inspect/backup/restore plus list-intents and cancel-execution API coverage | ✅ Done — `GET /v1/intents` implemented for existing `ferrumctl list-intents`; `POST /v1/executions/{execution_id}/cancel` was already implemented and is now documented in OpenAPI |
| P2.3 | **Deep health check** | Engineering | Functional readiness probe documented and operational | ✅ Done — `/v1/readyz/deep` documented in `PRODUCTION_NOTES.md` §Health and Readiness Endpoints; returns 200 when store healthy, 503 when unhealthy |
| P2.4 | **TLS/reverse proxy configured and verified** | Operator | TLS termination at reverse proxy (nginx/etc.); ferrumd does not terminate TLS; HTTPS probes pass through proxy; HTTP→HTTPS redirect verified (308); with-token auth through proxy verified (200 after remediation) | ☑ Closed via delegated authority 2026-05-15 (operator-owned; not full production signoff) |
| P2.5 | **Bearer token generated and verified** | Operator | Real bearer token generated by operator via `openssl rand -hex 32`; auth_mode=bearer confirmed; no-token rejection verified (401); with-token acceptance verified (200 after remediation) | ☑ Closed via delegated authority 2026-05-15 (operator-owned; not full production signoff) |

### P2 Notes

- P2.1–P2.3 are engineering items. P2.4–P2.5 are operator-owned.
- P2.1: Adapters have verified local slices (fs: 146 tests, git: 86 tests, http: 103 tests,
  sqlite: 16 tests, maildraft: 16 tests). Remaining surface (permissions/symlinks for fs,
  remote push/pull for git, broader replay for http) is post-v1 scope.
- P2.2: `GET /v1/intents` supports `intent_id`, repeated `state`, `cursor`, and `limit`
  parameters and returns the JSON shape expected by `ferrumctl list-intents`. `exec_state` is
  populated from the latest execution state when one exists and remains `null` when no execution
  exists. `POST /v1/executions/{execution_id}/cancel`
  remains available via `ferrumctl cancel-execution --confirm` and is documented in OpenAPI.
- P2.4: FerrumGate v1 does not include TLS termination. Deploy behind a TLS-terminating
  reverse proxy. Example nginx config in `configs/examples/nginx-ferrumgate.conf`.
- See [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md)
  for v1 support boundaries.

---

## P3 — Post-Pilot / Deferred (P3.1 Complete)

| # | Item | Owner | Status | Notes |
|---|---|---|---|---|
| P3.1 | **PostgreSQL local Docker/runtime implementation** | Engineering | ✅ Complete (local Docker) | Path 3; ADR-50 Phase P1–P4.4; ~2000–3000 LOC + migrations + container tests — **P3 repos + P4.1 DSN switching + P4.2 migration infra + P4.3 benchmark validation + P4.4 MVP migration complete**. Production/HA/multi-node remains deferred. P5a design authorized; P5b–P5e implementation gated. |
| P3.2 | **Multi-node / HA / read-replica** | Engineering | ☐ Pending | Not implemented; out of v1 scope |
| P3.3 | **Target-host execution beyond local slices** | Operator | ☐ Pending | D1–D6 drills require operator execution on target host |
| P3.4 | **Phase 2 transaction batching** | Engineering | ✅ Reverted | Benchmark regression; Phase 1 write queue remains production target |
| P3.5 | **Outcome-aware Governance (U1)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |
| P3.6 | **Reversible Execution Planner (U2)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |
| P3.7 | **Cross-runtime Provenance Fabric (U3)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |
| P3.8 | **Runtime Integrations — MCP/local/NemoClaw (U4)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |

### P3 Notes

- P3.1 is complete for local Docker/runtime (P1–P4.4). Production/HA/multi-node remains deferred.
- P5a (design/ADR) is authorized. P5b conservative defaults implemented; post-deploy monitoring required.
- P5c (backup/restore design/docs) is complete (`109-p5c-postgresql-backup-restore-runbook.md`; RPO=15min/RTO=30min). P5c.V1–V2 pending operator drill.
- P5d skipped (D1=A/D3=A). P5e implementation remains gated on P5b–P5c design complete.
- P3.2 is blocked until P5a–P5e are complete and P6 assessment is done.
- P3.3 is operator-owned target execution evidence.
- P3.5–P3.8 are implemented but explicitly outside the v1 single-node support baseline.
  They do not contribute to the production-ready claim.
- See [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 3 for G3 gates.

---

## Consolidated Blocker Summary

| Blocker | Type | Owner | Resolution |
|---------|------|-------|------------|
| CI swallows cargo check | Repo-side | Engineering | ✅ Fixed — CI now runs fmt/check/clippy/test without `\|\| true` |
| Readiness probe semantics undocumented | Engineering | Engineering | ✅ Fixed — `/v1/readyz/deep` documented as functional probe in `PRODUCTION_NOTES.md` |
| Target-host execution evidence | Operator | Operator | ✅ Passed 2026-05-13 — D1–D6 drills passed 6/6 on target host |
| G2.1–G2.8 signed (conditional pilot) | Operator | Operator | ✅ Signed 09/05/2026 by BrianNguyen for conditional single-node SQLite pilot only |
| Operator signoff obtained (conditional pilot) | Operator | Operator | ✅ Signed 09/05/2026 by BrianNguyen for conditional single-node SQLite pilot only |
| Backup automation | Operator | Operator | ✅ Done — timer active, backups verified; retention pruning closed via delegated authority with run id `20260515T1606Z-b3-retention` |
| Restore drill | Operator | Operator | ✅ Passed 2026-05-15 — fixed binary deployed; 0.463s restore; live DB verify OK |
| TLS/reverse proxy | Operator | Operator | ☑ Closed via delegated authority 2026-05-15 — HTTPS probes pass; HTTP→HTTPS redirect verified (308); with-token auth through proxy verified (200 after remediation) |
| PostgreSQL local runtime | Engineering | Engineering | ✅ Complete — local Docker/runtime support implemented; production/HA/multi-node deferred |

---

## Production Blocker Review (2026-05-15)

> **Scope**: Operator-action blockers for single-node SQLite v1 conditional pilot.
> **Status**: B1/B2/B3/B4/B5 closed/accepted or delegated-closed. New active blockers documented below as Blocks A/B/C with exact commands, evidence gates, and runbook references.
> **No production-ready claim**.

### Active Operator-Action Blockers (Sequenced)

| # | Blocker | Owner | Evidence Gate | Sequencing |
|---|---------|-------|---------------|------------|
| 1 | **Real owned domain** (Block A) | Operator | DuckDNS is not a production-owned domain; operator must procure and configure a production domain | P0 — before any external exposure |
| 2 | **Off-VM alerting / external notification** (Block B) | Operator | Alerting must reach an off-VM channel (email/SMS/pager) with confirmed delivery | P0 — before unattended operation |
| 3 | **Keyless backup / VM OAuth scope blocker** (Block C) | Operator | Backup storage must not rely on VM-instance OAuth scopes; use service-account key or workload identity with scoped storage permissions; **or** operator explicitly accepts key-based backup risk | P0 — before production data volume |

### Block A — Real Owned Domain

**Current state**: VM uses DuckDNS (`ferrumgate.duckdns.org`). DuckDNS is acceptable for non-production exploration but is **not** a production-owned domain. Operator confirmed no real owned domain and no DNS configuration are available yet.

**VM evidence**:
- External IP: `34.158.51.8`
- VM: `ferrumgate-nonprod` in `asia-southeast1-a`
- Script: `scripts/gcp/phase3g_configure_real_domain.sh`

**Exact command (placeholder)**:
```bash
bash scripts/gcp/phase3g_configure_real_domain.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --real-domain <REAL_DOMAIN>
```

**Operator inputs required**:
- `REAL_DOMAIN`: operator-owned domain with DNS A record pointing to `34.158.51.8`

**Evidence gates**:
| Gate | Evidence |
|------|----------|
| G-A1 | `curl` HTTPS 200 on `https://<REAL_DOMAIN>/v1/healthz` |
| G-A2 | `curl` HTTPS 200 on `https://<REAL_DOMAIN>/v1/approvals` with bearer token |
| G-A3 | `dig` output showing `<REAL_DOMAIN>` → `34.158.51.8` |

**Rollback**: Restore `/etc/caddy/Caddyfile.backup.*` on VM and reload Caddy.

**Reference**: [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) §Block A.

---

### Block B — Off-VM Alerting

**Current state**: Prometheus + AlertManager active. SendGrid API key secret present on VM. AlertManager config contains SendGrid webhook receiver. Synthetic alert POST returned HTTP 200 and was visible in AlertManager API. Operator confirmed receipt of inbox-check alert `TEST_ID=fg-inbox-check-20260516-052910` for at least one contact, with email content matching subject `FerrumGate Alert: FerrumGateInboxDeliveryCheck`, status `resolved`, severity `warning`, service `ferrumgate`. Bearer token rotation executed on VM (new token 200, old token 401, ROTATION_RESULT=PASS; token generated on VM and never printed). Secondary-contact confirmation remains pending unless separately verified. SendGrid API key rotation remains pending/operator-blocked.

**Prior evidence**: Direct API test and AlertManager webhook delivered in non-prod (Phase 3H/4A).

**SendGrid bridge template**: `configs/monitoring/alertmanager-sendgrid-bridge.example.yaml` (placeholder only; no real API key in repo).

**Operator inputs required**:
- `ALERT_PROVIDER`: SendGrid, SES, PagerDuty, Slack webhook, or SMTP relay
- `PROVIDER_API_KEY`: stored VM-locally at `/etc/ferrumgate/secrets/alert-provider-api-key`
- `PRIMARY_CONTACT`: email or webhook URL
- `SECONDARY_CONTACT`: escalation email or webhook URL
- `ALERT_SENDER`: verified sender identity

**Evidence gates**:
| Gate | Evidence | Status |
|------|----------|--------|
| G-B1 | Test alert delivered to at least one operator contact | 🟡 Partial — operator confirmed inbox receipt of `TEST_ID=fg-inbox-check-20260516-052910`; covers at least one contact |
| G-B2 | Test alert delivered to `SECONDARY_CONTACT` | ☐ Pending — secondary-contact confirmation remains pending unless separately verified |
| G-B3 | Key rotation procedure executed at least once in non-prod | 🟡 Partial — bearer token rotation executed on VM (new token 200, old token 401, ROTATION_RESULT=PASS); SendGrid API key rotation remains pending/operator-blocked |
| G-B4 | Escalation matrix documented and acknowledged by operator | ☐ Pending |

**Rollback**: Remove external receivers from AlertManager config; reload AlertManager; delete API key secret.

**Reference**:
- [`artifacts/2026-05-15-r1-alerting-rotation-policy.md`](./artifacts/2026-05-15-r1-alerting-rotation-policy.md)
- [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) §Block B.
- [`artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md`](./artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md) §Block B SendGrid smoke-test state.

---

### Block C — Keyless Backup / VM OAuth Scope Blocker

**Current state (2026-05-16)**: Operator selected **C1**. VM scopes updated via `set-service-account` to include `devstorage.read_write`. Initial restart failed with `ZONE_RESOURCE_POOL_EXHAUSTED` for `e2-medium` and `e2-small`; recovery succeeded with `n2-standard-2`. Static IP `34.158.51.8` preserved. Keyless GCS probe (isolated HOME, no key env) passed: `gsutil ls` PASS, `gsutil cp` PASS. Offsite sync script ran successfully (`OFFSITE_SYNC_RC=0`, 15.3 MiB copied). Residual old key file at `/etc/ferrumgate/gcs-service-account.json` was removed; post-removal keyless probe and offsite sync both PASS. e2-medium revert was attempted but failed with `ZONE_RESOURCE_POOL_EXHAUSTED`; rolled back to `n2-standard-2` successfully. `n2-standard-2` remains temporary operational type.

**Historical state (pre-2026-05-16)**: VM service account had OAuth scope `devstorage.read_only` but not `devstorage.read_write` or `cloud-platform`. Keyless GCS write was blocked.

**Two paths**:

#### Path C1 — Stop-start VM with GCS write scopes (keyless, preferred)
- Primary: stop VM, update scopes via `set-service-account`, start VM (brief downtime)
- Fallback (C1b): recreate VM from snapshot only if `set-service-account` fails
- Zero long-lived key material on disk
- Exact commands in runbook R4 §C.3

#### Path C2 — Accept key-based backup risk with rotation procedure
- Zero downtime
- Service account key JSON stored on VM at `/etc/ferrumgate/secrets/gcs-service-account.json`
- Operator must sign risk acceptance statement (see `R2` artifact)
- 90-day key rotation required
- Exact commands in runbook R4 §C.4

**Operator inputs required**:
- Path selection: `C1` or `C2`
- `GCS_BUCKET`: e.g., `gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/`
- `OPERATOR_BACKUP_SA_ID` (C2 only): short ID for `gcloud iam service-accounts create`, e.g., `ferrumgate-backup`
- `OPERATOR_BACKUP_SA_EMAIL` (C2 only): full email, e.g., `OPERATOR_BACKUP_SA_ID@fairy-b13f4.iam.gserviceaccount.com`

**Evidence gates**:
| Gate | Evidence | Status |
|------|----------|--------|
| G-C1 | Operator selects C1 or C2 and records decision with rationale | ✅ C1 selected 2026-05-16 |
| G-C2 | If C1: VM scopes updated via `set-service-account` to include `storage-rw`; `gsutil rsync` from VM succeeds without key file | ✅ Keyless probe PASS; offsite sync PASS (rc=0) |
| G-C3 | If C2: Signed risk acceptance statement; key file present at `/etc/ferrumgate/secrets/gcs-service-account.json` with `chmod 600`; `gsutil rsync` succeeds | N/A — C1 selected |
| G-C4 | If C2: Key rotation procedure documented and schedule acknowledged | N/A — C1 selected |

**Follow-up**:
- Revert machine type from `n2-standard-2` to `e2-medium` when zone capacity permits — attempted 2026-05-16 but failed `ZONE_RESOURCE_POOL_EXHAUSTED`; rolled back to `n2-standard-2` successfully
- Remove residual `/etc/ferrumgate/gcs-service-account.json` — **completed 2026-05-16**

**Rollback**:
- C1: Stop VM, revert to original scopes with `set-service-account`, start VM; fallback C1b uses snapshot restore
- C2: Revoke key at IAM; remove key file from VM; remove gsutil auth

**Reference**:
- [`artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md`](./artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md)
- [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) §Block C.
- [`artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md`](./artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md) §Block C execution evidence.

---

### Settled Decisions (Not Blockers)

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **PostgreSQL production deployment** | **NO** | Remains deferred unless explicitly selected by operator; Path 3 scope |
| **HA / multi-node** | **NO / out of v1 scope** | Not implemented; single-node SQLite only |
| **CI cost / local gate** | **Accepted** | GitHub-hosted Actions minutes avoided; local `run_pre_target_gate.sh --full` and manual validation are the accepted approach; self-hosted runner is a future option |

---

## Cross-Reference Index

| From | To | Purpose |
|---|---|---|
| This doc | [`31-release-paths-todo.md`](./31-release-paths-todo.md) | Path 2 G2 gates and checklists |
| This doc | [`66-path-2-operator-handoff.md`](./66-path-2-operator-handoff.md) | Phase A/B handoff; operator-owned blockers |
| This doc | [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Ordered execution checklist |
| This doc | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence packet |
| This doc | [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) | Operator signoff form |
| This doc | [`27-production-evaluation-plan.md`](./27-production-evaluation-plan.md) | Production evaluation framework |
| This doc | [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md) | v1 support boundaries and constraints |
| This doc | [`30-production-roadmap.md`](./30-production-roadmap.md) | Phase 1/2/3 production roadmap |
| This doc | [`PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) | SQLite configuration and stress test baseline |
| This doc | [`70-security-hardening-local-only-plan.md`](./70-security-hardening-local-only-plan.md) | Security hardening proposals, local-only audit commands, token rotation procedure |
| This doc | [`71-mcp-server-feasibility-and-design.md`](./71-mcp-server-feasibility-and-design.md) | MCP server design and todo-list (post-v1 scope; v1.4 MCP Governance Beta; U4 bridge exists, MCP server is next step) |
| This doc | [`72-mcp-server-phase-a-implementation-plan.md`](./72-mcp-server-phase-a-implementation-plan.md) | Phase A–C implementation plan/tracker: Phases A, B, C complete; D-0 ready to implement; D-1 deferred |
| This doc | [`73-mcp-server-phase-d-implementation-plan.md`](./73-mcp-server-phase-d-implementation-plan.md) | Phase D-0 read-only REST client plan + D-1 deferred governance pipeline |
| This doc | [`artifacts/2026-05-13-d1-d6-target-host-evidence.md`](./artifacts/2026-05-13-d1-d6-target-host-evidence.md) | D1–D6 target-host drill pass evidence |
| This doc | [`artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md`](./artifacts/2026-05-15-g36-t3b-restore-drill-fixed-success-evidence.md) | T3b fixed restore drill success and G3.6 full acceptance |
| This doc | [`artifacts/2026-05-15-r1-alerting-rotation-policy.md`](./artifacts/2026-05-15-r1-alerting-rotation-policy.md) | Block B alerting rotation policy |
| This doc | [`artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md`](./artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md) | Block C key-based backup risk acceptance and C1/C2 decision matrix |
| This doc | [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) | Exact command sequences and rollback for Blocks A/B/C |
| This doc | [`artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md`](./artifacts/2026-05-16-c1-keyless-recovery-and-block-b-status.md) | C1 keyless backup scope update, zone-capacity recovery, keyless verification, and Block B SendGrid smoke-test state |
| This doc | [`122-completion-roadmap-and-hardening-tracker.md`](./122-completion-roadmap-and-hardening-tracker.md) | 10-item completion tracker and hardening tasks for May 13–16 follow-up |

---

## Evidence Sources

| Check | Command | Pass Criteria |
|-------|---------|---------------|
| CI hardening | `.github/workflows/ci.yml` | `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all run without `\|\| true` |
| Local pretarget gate | `bash scripts/run_pre_target_gate.sh` | All checks pass; Tier 0 smoke validation |
| Local full gate | `bash scripts/run_pre_target_gate.sh --full` | `ALL LOCAL CHECKS PASSED`; includes cargo workspace tests and clippy; orchestrator-run on 2026-05-15 |
| CI cost preference | N/A (local/manual-only) | GitHub-hosted CI not triggered for private repo due to Actions minutes cost; local validation/self-hosted/manual-only approach accepted |
| Layout validation | `bash scripts/validate_repo_layout.sh` | "Repository layout looks OK" |
| Contract consistency | `python3 scripts/check_contract_consistency.py` | "VALIDATION PASSED" |
| Local security audit | `make audit` | `cargo-deny v0.19.6` and `cargo-audit v0.22.1` installed; cargo-deny advisory DB fetched, advisories ok; cargo-audit scans 384 dependencies against 1090 advisories; `RUSTSEC-2023-0071` ignored as uncompiled optional dependency; `[cargo-deny] PASS`; `[cargo-audit] PASS`; `SECURITY AUDIT GATE: PASS` |

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-03 | Initial production-readiness roadmap | Engineering |
| 2026-05-15 | Reconciled with latest evidence: P0.2 D1–D6 passed 6/6 on 2026-05-13; P0.3 restore drill passed on 2026-05-15; P0.4 retention pruning verified with run id `20260515T1606Z-b3-retention`; P2.4/P2.5 closed via delegated authority 2026-05-15. Added production blocker review with active blockers (real owned domain, off-VM alerting, keyless backup) and settled decisions (PostgreSQL=NO, HA=NO, CI cost accepted). Local full gate evidence recorded. No production-ready claim preserved. | Engineering |
| 2026-05-15 | Added Blocks A/B/C detail with exact command sequences, evidence gates, and rollback. Cross-referenced new R1–R4 artifacts: alerting rotation policy (`R1`), key-based backup risk acceptance (`R2`), production blocker execution runbook (`R4`). Updated cross-reference index. No production-ready claim preserved. | Engineering |
| 2026-05-16 | Updated Block C status: C1 executed with `set-service-account` scope update; zone-capacity recovery via `n2-standard-2`; keyless GCS probe PASS; offsite sync PASS (rc=0, 15.3 MiB). Updated Block B status: SendGrid synthetic alert POST 200, visible in AlertManager API; inbox delivery remains pending. Block A remains blocked (no real domain). Cross-referenced `2026-05-16-c1-keyless-recovery-and-block-b-status.md`. No production-ready claim preserved. | Engineering |
| 2026-05-16 (follow-up) | Recorded live follow-up execution: residual `/etc/ferrumgate/gcs-service-account.json` removed with post-removal keyless probe PASS and offsite sync PASS; e2-medium revert attempted but failed with `ZONE_RESOURCE_POOL_EXHAUSTED`, rolled back to `n2-standard-2` successfully; SendGrid inbox-check alert dispatched (TEST_ID `fg-inbox-check-20260516-052910`); SSH firewall temporarily opened to `14.239.184.129/32` then restored to `118.69.4.63/32`. Block A and conservative statuses unchanged. | Engineering |
| 2026-05-16 (inbox confirmation) | Operator confirmed receipt of SendGrid inbox-check alert `TEST_ID=fg-inbox-check-20260516-052910` for at least one contact, with email content matching subject `FerrumGate Alert: FerrumGateInboxDeliveryCheck`, status `resolved`, severity `warning`, service `ferrumgate`. Block B G-B1 marked partial. Block A remains blocked — operator confirmed no real owned domain and no DNS configuration available yet. Block C and conservative statuses unchanged. | Engineering |
| 2026-05-16 (tracker) | Created `122-completion-roadmap-and-hardening-tracker.md` with 10-item tracker. Updated `AGENTS.md` stale P0/status text and `01-current-state.md` with May 13–16 Block A/B/C evidence. Cross-referenced tracker. No production-ready claim preserved. | Engineering |
| 2026-05-16 (hardening) | Added local/manual security audit gate (`scripts/run_security_audit.sh` + `make audit`). Updated tracker items 7–9: ferrum-cap fix verified (atomic `update_status_if_active`, 9 tests), escalation matrix skeleton added to tracker, key rotation and secondary contact marked operator-blocked. No production-ready claim preserved. | Engineering |
| 2026-05-16 (audit evidence) | `cargo-audit v0.22.1` installed; `make audit` passes (cargo-audit scans 384 dependencies against 1090 advisories, 0 actionable issues; `RUSTSEC-2023-0071` ignored as uncompiled optional dependency via `default-features = false` on `sqlx`). Block A (domain), G-B2 (secondary contact), G-B3 (key rotation) remain operator-blocked. No production-ready claim preserved. | Engineering |
| 2026-05-16 (operator execution) | `cargo-deny v0.19.6` installed (debug build after release timeout); `make audit` passes with both cargo-deny and cargo-audit. Bearer token rotation executed on VM securely (token generated on VM, never printed; new token 200, old token 401, ROTATION_RESULT=PASS). SSH firewall temporarily opened to `14.239.184.129/32` for live work and restored to `118.69.4.63/32` after. Block A (domain), G-B2 (secondary contact), and SendGrid API key rotation remain operator-blocked. No production-ready claim preserved. | Engineering |

---

*Document created: 2026-05-03. Production-readiness roadmap — no production-ready claim, no G2 complete, no operator signature pre-populated.*

*Next update: Block A remains blocked — operator confirmed no real domain and no DNS available yet. Block B: operator confirmed inbox receipt of `TEST_ID=fg-inbox-check-20260516-052910` with subject/content verified (G-B1 partial); secondary-contact confirmation (G-B2) remains pending/operator-blocked; bearer token rotation executed on VM (G-B3 partial — new token 200, old token 401, ROTATION_RESULT=PASS); SendGrid API key rotation remains pending/operator-blocked; escalation matrix skeleton added to tracker (G-B4 partial). Engineering items 7–9 closed: ferrum-cap fix verified, local audit gate added, and `cargo-deny v0.19.6` + `cargo-audit v0.22.1` installed with `make audit` passing. `RUSTSEC-2023-0071` ignored as uncompiled optional dependency (`rsa` via `sqlx-mysql`, blocked by `default-features = false` on `sqlx`).*
