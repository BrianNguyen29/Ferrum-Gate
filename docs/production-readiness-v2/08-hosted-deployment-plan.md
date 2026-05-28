# 08 — Hosted Deployment Plan

> **Status**: Planning artifact. Local demo compose validated (DEP-1/DEP-2). Target-host packaging remains planned.
> **Owner**: Engineering
> **Last updated**: 2026-05-21
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Package FerrumGate into reproducible deployment modes so operators can deploy consistently across local demo, single-node self-hosted, PostgreSQL self-hosted, and eventually Kubernetes.

## Current state

- Config templates exist.
- Docker Compose PG local demo (`docker-compose.postgres-demo.yml`) exists and was runtime-validated locally for DEP-3.
- Local demo compose (`docker-compose.demo.yml`) + Dockerfile validated for DEP-1/DEP-2.
- Systemd service example exists (`configs/examples/ferrumd.service`); target-host `systemctl` runtime validated on `ferrumgate-nonprod` with evidence. **Not production-ready.**
- Env var reference example exists (`configs/examples/ferrumd.env.example`).
- Deployment guide exists (`docs/guides/hosted-deployment.md`); DEP-4/DEP-6 limited target-host evidence captured. Full production-ready validation pending.
- Helm chart scaffold exists locally (`deploy/helm/ferrumgate/`); local `helm lint` and `helm template` passed with Helm 3.15.4. Live cluster install remains deferred. Not production-ready.

## Gaps

| Gap | Why |
|-----|-----|
| Deployment guide validation | Scaffold exists; step-by-step runtime validation for each mode not yet done |
| Helm chart | K8s deployment is not packaged |
| Backup/restore production validation | Hosted single-node SQLite temp-copy drill passed with limited evidence. PostgreSQL production backup/restore **not** claimed. |

## Implementation tasks

### P0 deliverables

- [x] `docker-compose.demo.yml` — SQLite in-memory, auth disabled, loopback only. Config + Dockerfile present; DEP-1/DEP-2 validated locally.
- [x] `docker-compose.postgres-demo.yml` — ferrumd + PostgreSQL. Config + Dockerfile build-arg present; runtime validated locally for DEP-3.
- [x] `configs/examples/ferrumd.service` — systemd unit example. Target-host `systemctl` runtime validated on `ferrumgate-nonprod`; evidence captured. **Not production-ready.**
- [x] `configs/examples/ferrumd.env.example` — env var reference. Exists.
- [x] `docs/guides/hosted-deployment.md` — deployment guide (scaffold exists).

### P1 deliverables

- [x] Helm chart scaffold (local-only; `deploy/helm/ferrumgate/`). `helm lint` / `helm template` passed locally with Helm 3.15.4. Not production-ready.
- [x] K8s manifests (included in Helm chart templates).
- [ ] Prometheus/Grafana dashboard integration.
- [ ] Backup cron/timer docs.
- [ ] Managed PostgreSQL guide.

### P2 deliverables

- [x] Terraform single-node module (`deploy/terraform/ferrumgate-single-node/`). Local artifact generator using `local_file` + `null_resource`; no cloud credentials. Not production-ready.
- [ ] Pulumi module / cloud modules.
- [ ] Cloud marketplace style deployment.
- [ ] Zero-downtime upgrade guide.

## Acceptance criteria

- [x] DEP-1: Docker Compose demo starts ferrumd locally (`docker-compose.demo.yml` + `Dockerfile`). Evidence: `docs/implementation-path/artifacts/2026-05-19-compose-demo-evidence.md`.
- [x] DEP-2: Healthz passes after compose up locally. Evidence: same artifact.
- [x] DEP-3: Postgres deployment mode documented and tested locally (`docker-compose.postgres-demo.yml` + Dockerfile). Evidence: `docs/implementation-path/artifacts/2026-05-19-compose-demo-pg-evidence.md`.
- [x] DEP-4: Systemd unit works with env file. Target-host systemd runtime validated on `ferrumgate-nonprod`; evidence: `docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md`. Not a production-ready claim.
- [x] DEP-5: Helm chart scaffold created locally (`deploy/helm/ferrumgate/`). Templates render without syntax errors. Live kind cluster install succeeded 2026-05-21 (pod 1/1 Running, health/readiness OK). Evidence: `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §4. NOT production K8s/HA.
- [x] DEP-6: Backup/restore procedure works in hosted mode. Hosted single-node SQLite backup/restore temp-copy drill passed on `ferrumgate-nonprod`; evidence: `docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md`. Not a production-ready claim.

## Evidence required

- `deployment-test-evidence.md`
- Screenshots or logs for each DEP gate

## Non-claims

- **NOT production-ready**: Deployment packaging is a prerequisite, not a claim.
- **NOT all modes validated**: Mode D (K8s) validated on local kind only; production K8s/HA not implemented or claimed.
- **NOT a managed service**: These are self-hosted deployment templates.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.10, §4 Phase 8
- [`docs/guides/hosted-deployment.md`](../../guides/hosted-deployment.md) — User-facing guide scaffold.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites.
- [`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-runbook.md`](../../implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-runbook.md) — DEP-4 target-host runbook (prepared).
- [`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md`](../../implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md) — DEP-4 target-host systemd runtime evidence (captured; **not production-ready**).
- [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md`](../../implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md) — DEP-6 hosted backup preflight checklist (prepared).
- [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md`](../../implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md) — DEP-6 hosted SQLite temp-copy backup/restore evidence (captured; **not production-ready**).
- [`docs/implementation-path/artifacts/2026-05-19-dep4-dep6-target-execution-blocked.md`](../../implementation-path/artifacts/2026-05-19-dep4-dep6-target-execution-blocked.md) — DEP-4/DEP-6 target execution blocked evidence (SSH unreachable; historical, not edited).
