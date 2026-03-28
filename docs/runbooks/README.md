# Operator Runbooks

Operational playbooks for day-to-day and incident response.

## Available runbooks

- [TLS / Ingress (nginx)](ops-tls-ingress-runbook.md) — production TLS termination in front of ferrumd
- [SQLite Backup / Restore / Capacity Planning](ops-sqlite-backup-runbook.md) — online and offline backup, restore procedures, DB growth estimation, and connection/throughput planning
- [Provenance Audit](provenance-audit-runbook.md) — execution audit, lineage investigation, external event verification, and compliance evidence export for operators and security/compliance reviewers

## Release Artifacts

- [v1 Single-Node RC Evidence](../implementation-path/25-v1-single-node-rc-evidence.md) -- RC gate status: all checklist items green; single-node v1 ready to close

## When to use these

These runbooks are operator-facing, step-by-step guides for specific production scenarios. For general deployment and operations guidance, see [15-deployment-and-operations.md](../15-deployment-and-operations.md).