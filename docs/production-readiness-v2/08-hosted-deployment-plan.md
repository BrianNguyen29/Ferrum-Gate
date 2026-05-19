# 08 — Hosted Deployment Plan

> **Status**: Planning artifact. Local demo compose validated (DEP-1/DEP-2). Target-host packaging remains planned.
> **Owner**: Engineering
> **Last updated**: 2026-05-19
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Package FerrumGate into reproducible deployment modes so operators can deploy consistently across local demo, single-node self-hosted, PostgreSQL self-hosted, and eventually Kubernetes.

## Current state

- Config templates exist.
- Docker Compose PG local demo (`docker-compose.postgres-demo.yml`) exists and was runtime-validated locally for DEP-3.
- Local demo compose (`docker-compose.demo.yml`) + Dockerfile validated for DEP-1/DEP-2.
- Systemd service example exists (`configs/examples/ferrumd.service`); local preflight validation completed, real target-host `systemctl` runtime validation pending.
- Env var reference example exists (`configs/examples/ferrumd.env.example`).
- Deployment guide scaffold exists (`docs/guides/hosted-deployment.md`); full validation pending.
- Helm chart is not implemented.

## Gaps

| Gap | Why |
|-----|-----|
| Deployment guide validation | Scaffold exists; step-by-step runtime validation for each mode not yet done |
| Helm chart | K8s deployment is not packaged |
| Backup/restore hosted validation | Backup examples exist in `configs/examples/`; operational drill in hosted mode not yet validated |

## Implementation tasks

### P0 deliverables

- [x] `docker-compose.demo.yml` — SQLite in-memory, auth disabled, loopback only. Config + Dockerfile present; DEP-1/DEP-2 validated locally.
- [x] `docker-compose.postgres-demo.yml` — ferrumd + PostgreSQL. Config + Dockerfile build-arg present; runtime validated locally for DEP-3.
- [x] `configs/examples/ferrumd.service` — systemd unit example. Exists; local preflight evidence recorded; target-host `systemctl` runtime validation pending.
- [x] `configs/examples/ferrumd.env.example` — env var reference. Exists.
- [x] `docs/guides/hosted-deployment.md` — deployment guide (scaffold exists).

### P1 deliverables

- [ ] Helm chart.
- [ ] K8s manifests.
- [ ] Prometheus/Grafana dashboard integration.
- [ ] Backup cron/timer docs.
- [ ] Managed PostgreSQL guide.

### P2 deliverables

- [ ] Terraform/Pulumi module.
- [ ] Cloud marketplace style deployment.
- [ ] Zero-downtime upgrade guide.

## Acceptance criteria

- [x] DEP-1: Docker Compose demo starts ferrumd locally (`docker-compose.demo.yml` + `Dockerfile`). Evidence: `docs/implementation-path/artifacts/2026-05-19-compose-demo-evidence.md`.
- [x] DEP-2: Healthz passes after compose up locally. Evidence: same artifact.
- [x] DEP-3: Postgres deployment mode documented and tested locally (`docker-compose.postgres-demo.yml` + Dockerfile). Evidence: `docs/implementation-path/artifacts/2026-05-19-compose-demo-pg-evidence.md`.
- [ ] DEP-4: Systemd unit works with env file. Local preflight evidence recorded at `docs/implementation-path/artifacts/2026-05-19-systemd-validation-evidence.md`; real `systemctl status ferrumd` evidence pending.
- [ ] DEP-5: Helm install produces ready pod. Not implemented.
- [ ] DEP-6: Backup/restore procedure works in hosted mode. Backup examples exist in `configs/examples/`; hosted-mode drill not yet validated.

## Evidence required

- `deployment-test-evidence.md`
- Screenshots or logs for each DEP gate

## Non-claims

- **NOT production-ready**: Deployment packaging is a prerequisite, not a claim.
- **NOT all modes validated**: Mode D (K8s) is not implemented yet.
- **NOT a managed service**: These are self-hosted deployment templates.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.10, §4 Phase 8
- [`docs/guides/hosted-deployment.md`](../../guides/hosted-deployment.md) — User-facing guide scaffold.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites.
