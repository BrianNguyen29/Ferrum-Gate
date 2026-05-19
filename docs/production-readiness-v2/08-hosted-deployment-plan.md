# 08 — Hosted Deployment Plan

> **Status**: Planning artifact. Docker Compose local exists; target-host packaging is planned.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Package FerrumGate into reproducible deployment modes so operators can deploy consistently across local demo, single-node self-hosted, PostgreSQL self-hosted, and eventually Kubernetes.

## Current state

- Config templates exist.
- Docker Compose PG local exists.
- Systemd service example does not exist yet.
- No coherent hosted deployment story.
- Helm chart is not implemented.

## Gaps

| Gap | Why |
|-----|-----|
| No docker-compose.demo.yml | Cannot start a local demo in one command |
| No docker-compose.postgres-demo.yml | Cannot demo PG mode easily |
| No systemd service example | Self-hosted operators must write their own |
| No env var reference doc | Operators do not know all config options |
| No deployment guide | No step-by-step for any mode |
| No Helm chart | K8s deployment is not packaged |
| No backup/restore hosted guide | Operators do not know how to back up in hosted mode |

## Implementation tasks

### P0 deliverables

- [x] `docker-compose.demo.yml` — SQLite in-memory, auth disabled, loopback only. Config present; runtime validation pending image availability.
- [ ] `docker-compose.postgres-demo.yml` — ferrumd + PostgreSQL.
- [ ] `configs/examples/ferrumd.service` — systemd unit example.
- [ ] `configs/examples/ferrumd.env.example` — env var reference.
- [ ] `docs/guides/hosted-deployment.md` — deployment guide (scaffold exists).

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

- [x] DEP-1: Docker Compose demo config present (`docker-compose.demo.yml`). Runtime start pending image availability.
- [ ] DEP-2: Healthz passes after compose up — open until DEP-1 runtime validated.
- [ ] DEP-3: Postgres deployment mode documented and tested.
- [ ] DEP-4: Systemd unit works with env file.
- [ ] DEP-5: Helm install produces ready pod.
- [ ] DEP-6: Backup/restore procedure works in hosted mode.

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
