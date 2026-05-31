# Security Policy

FerrumGate exists to improve execution governance for agentic systems. We take security seriously and welcome responsible disclosure.

## Supported Versions

| Version | Notes |
|---------|-------|
| v0.1.0 | Current development version; single-node SQLite focus |
| < v0.1.0 | Unsupported |

## Reporting a Vulnerability

**Please use private disclosure first.** Do not open a public issue for unpatched vulnerabilities.

To report a security issue, email the maintainers directly or use GitHub Private Vulnerability Reporting if enabled. Include:
- A clear description of the vulnerability
- Steps to reproduce (or a plausible attack scenario)
- Affected versions or commit ranges
- Any suggested mitigations

### Response Expectations

- **Acknowledgment**: Within 5 business days
- **Initial assessment**: Within 10 business days
- **Fix timeline**: Target 30 days for confirmed vulnerabilities; complex issues will receive a public timeline update
- **Disclosure**: Coordinated disclosure preferred; we will work with reporters to agree on a reasonable publication date

## Scope of Security Review

The following areas are in scope for vulnerability reports:
- Capability misuse or escalation
- Policy bypass in the PDP/gateway
- Taint / provenance gaps that could lead to unsanctioned execution
- Rollback or compensation failures that break reversibility guarantees
- Integrity mismatches in lineage or ledger records
- Output sanitization leaks
- Authentication or authorization weaknesses in the bearer-token gate
- Rate-limit bypass

## Out of Scope
- Denial of service via resource exhaustion without a reproducible exploit
- Issues in dependencies unless they directly affect FerrumGate's invariants
- Social engineering or physical attacks
- Vulnerabilities in unsupported versions

## No Bug Bounty

FerrumGate does not operate a bug bounty program. We appreciate responsible disclosure and will acknowledge reporters in release notes unless they prefer anonymity.

## Security Model

- Intent-scoped execution with capability bounds
- Approval-aware workflow with lineage tracking
- Rollback-classified operations (compensate is the v1 recovery endpoint)
- Bearer-token auth in bearer-auth mode; dev config remains auth-disabled for local development
- Rate limiting integrated with the gateway
