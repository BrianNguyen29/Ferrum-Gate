# 07 — Operator and deployment plan

## 1. Deployment strategy

### Current truth
- SQLite single-node is current supported baseline — see `19-v1-single-node-support-contract.md`
- health/ready are shallow
- backup/restore is manual file-level for SQLite
- auth should be bearer for non-loopback bind

### Roadmap direction
- Q1: keep SQLite baseline, harden docs and checks
- Q2: still engineer-centric, local/private beta deploys
- Q3: add Postgres and self-hosted commercial beta
- Q4: improve evidence and runtime governance, not cloud multi-tenant yet

> **V1 boundary**: "SQLite single-node is current supported baseline" is the v1 support
> contract statement. Multi-node, HA, read-replica are out of v1 scope per the v1
> support contract. Postgres support is Q3 post-v1 scope. Operator UI is post-v1 scope.

## 2. Operator personas

### Operator
Needs:
- inspect approvals
- inspect lineage
- verify execution state
- trigger/observe compensate
- investigate quarantine

### Engineer / Integrator
Needs:
- deploy FerrumGate reliably
- integrate gateway and adapters
- debug policy/capability issues

### Security / Platform lead
Needs:
- evidence bundle
- policy explainability
- audit trail integrity

## 3. Operator UI roadmap

### Q2-Q3 minimum UI surfaces
- [ ] execution list
- [ ] execution detail
- [ ] lineage graph/text detail
- [ ] approvals list/detail
- [ ] decision explanation panel
- [ ] quarantine view

### Q3-Q4 advanced UI surfaces
- [ ] evidence export
- [ ] operator actions (revoke/retry/compensate where allowed)
- [ ] incident review workspace

## 4. Deployment checklist by quarter

### Q1
- [ ] sqlite dev/staging docs clean
- [ ] startup failure docs updated
- [ ] functional probe documented as mandatory

### Q2
- [ ] sample configs for engineering workflow deployments
- [ ] examples repo or compose for local demo

### Q3
- [ ] postgres config profile
- [ ] docker image publishing
- [ ] compose stack
- [ ] helm/k8s draft
- [ ] secrets handling docs
- [ ] observability docs

### Q4
- [ ] runtime/MCP integration deployment patterns
- [ ] evidence export operational runbook

## 5. Observability plan

### Minimum metrics
- [ ] policy evaluation latency
- [ ] capability mint latency
- [ ] verify success/fail rates
- [ ] compensate success/fail rates
- [ ] approval latency
- [ ] quarantine count
- [ ] lineage query latency

### Minimum logs
- [ ] execution_id on all critical events
- [ ] intent_id / capability_id correlation
- [ ] decision reason codes
- [ ] verify failure reasons
- [ ] compensate outcome
- [ ] redact secrets/PII/internal control details

### Minimum traces
- [ ] compile -> evaluate -> mint -> authorize -> prepare -> execute -> verify -> terminal

## 6. Support playbooks to maintain

- [ ] startup failures
- [ ] auth failures
- [ ] sqlite/postgres connectivity failures
- [ ] approvals stuck / stale
- [ ] compensate success but no external undo
- [ ] lineage missing event investigation
- [ ] policy bundle mismatch

## 7. GA blockers (do not claim GA before these are solved)

- [ ] no real production adapters
- [ ] operator still depends on raw DB or raw JSON for common incidents
- [ ] no Postgres/private deployment story
- [ ] accepted risks still break core invariants under normal use

> **V1 boundary note**: All GA blockers above are **post-v1 scope**. The v1 support
> contract explicitly lists "no real adapters", "no operator UI", and "multi-node/HA/read-replica
> not supported". These blockers represent the gap between v1 and a commercially
> shippable product — they are not v1 defects.
