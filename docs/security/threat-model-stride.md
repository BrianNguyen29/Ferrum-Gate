# STRIDE Threat Model

## Status

Phase 2.6 documentation — maps trust boundaries, STRIDE categories, existing controls, gaps, and deferred items. This is a living document; controls are updated as they are implemented.

## Trust Boundaries

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Human/Operator  │────▶│  FerrumGate     │────▶│   Policy/PDP    │
│   (Browser/CLI) │     │   Gateway       │     │   Engine        │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                       │
         │                       ├────▶ Store (SQLite/PostgreSQL)
         │                       ├────▶ Provenance Ledger
         │                       └────▶ Adapters (FS, Git, HTTP, SQLite)
         │
┌─────────────────┐     ┌─────────────────┐
│  Agent/MCP      │────▶│  MCP Server     │────▶ Gateway
│   Client        │     │  (FerrumGate)   │
└─────────────────┘     └─────────────────┘
```

Boundary list:
1. **B1**: Human/Operator → FerrumGate Gateway
2. **B2**: Agent/MCP Client → FerrumGate MCP Server
3. **B3**: MCP Server → Gateway
4. **B4**: Gateway → PDP (Policy Decision Point)
5. **B5**: Gateway → Store
6. **B6**: Gateway → Provenance Ledger
7. **B7**: Gateway → Adapters
8. **B8**: Operator → Approval Resolve endpoint

## STRIDE Mapping

### B1: Human/Operator → Gateway

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing (impersonate operator) | S | Bearer / scoped token auth; token lookup hash + salt | OIDC/JWT federation (Phase 4 deferred) |
| Tampering (modify config/policy) | T | Deny-by-default scope checks; RBAC | scoped token RBAC fully implemented (Phase 1 done) |
| Repudiation (deny admin action) | R | Audit log with hash chain (Phase 2.3–2.4) | local CLI direct-verify deferred; signed checkpoints deferred |
| Information Disclosure | I | TLS in transit; token values never logged | mTLS service-to-service (Phase 6 deferred) |
| Denial of Service | D | Rate limiting (`tower_governor`); SQLite write queue | sustained load testing incomplete |
| Elevation of Privilege | E | Scope enforcement; deny-by-default for unknown paths | no elevation path known at this time |

### B2: Agent/MCP Client → MCP Server

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | MCP server runs behind gateway; no direct adapter access | Agent Ed25519 identity (Phase 4.5–4.7 deferred) |
| Tampering | T | JSON-RPC request validated by gateway before execution | Streamable HTTP MCP integrity (Phase 6 deferred) |
| Repudiation | R | Execution provenance logged | agent-signed intent envelope deferred |
| Information Disclosure | I | Tool schemas exposed via MCP stdio only | no public MCP endpoint without auth |
| Denial of Service | D | Rate limiting on gateway | no MCP-specific quota yet |
| Elevation of Privilege | E | Capability minting requires policy evaluation | none known |

### B3: MCP Server → Gateway

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | Internal bridge; same process | N/A — same trust domain |
| Tampering | T | Internal typed API | N/A |
| Repudiation | R | Provenance events link MCP request to execution | N/A |
| Information Disclosure | I | In-memory only | N/A |
| Denial of Service | D | Internal queue limits | N/A |
| Elevation of Privilege | E | Capability validation before adapter call | N/A |

### B4: Gateway → PDP

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | Same-process PDP | N/A |
| Tampering | T | Policy bundle content-hash idempotency | N/A |
| Repudiation | R | Policy simulation logs decision without side-effect | N/A |
| Information Disclosure | I | In-memory evaluation | N/A |
| Denial of Service | D | PDP evaluation timeout | complex rule DoS not formally bounded |
| Elevation of Privilege | E | PDP returns Allow/Deny; gateway enforces | N/A |

### B5: Gateway → Store

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | Local connection (SQLite) or TLS (PostgreSQL) | mTLS for PG (Phase 6 deferred) |
| Tampering | T | Audit log hash chain; ledger hash chain | full Merkle root deferred |
| Repudiation | R | Append-only audit log; ledger entries immutable | signed checkpoint deferred |
| Information Disclosure | I | Connection string with credentials in config file | secrets management (vault integration) deferred |
| Denial of Service | D | Connection pool limits; busy_timeout; write queue | sustained SLO window incomplete |
| Elevation of Privilege | E | Store access via repo traits; no raw SQL exposure | none known |

### B6: Gateway → Provenance Ledger

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | Same-process write | N/A |
| Tampering | T | Ledger hash chain with `previous_ledger_hash` | external anchoring deferred |
| Repudiation | R | Append-only; raw_json snapshot | export bundle deferred |
| Information Disclosure | I | Same-process read | N/A |
| Denial of Service | D | WAL mode; write queue | N/A |
| Elevation of Privilege | E | LedgerRepo trait boundary | N/A |

### B7: Gateway → Adapters

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | Adapter key registry | no adapter authentication yet |
| Tampering | T | Rollback contracts; action digest | adapter-side integrity not enforced by gateway |
| Repudiation | R | Execution record with result digest | adapter attestation deferred |
| Information Disclosure | I | Adapter credentials in config | secret rotation automation deferred |
| Denial of Service | D | Execution timeouts | adapter resource exhaustion not bounded |
| Elevation of Privilege | E | Capability lease scoped to tool/action | none known |

### B8: Operator → Approval Resolve

| Threat | Category | Existing Control | Gap / Deferred |
|--------|----------|------------------|----------------|
| Spoofing | S | Scoped token with `approval:resolve` | second-factor approval deferred |
| Tampering | T | Approval state machine with valid transitions | none known |
| Repudiation | R | Audit log entry on every resolve | hash chain verifies tampering |
| Information Disclosure | I | Approval list scoped to token permissions | none known |
| Denial of Service | D | Rate limiting | none known |
| Elevation of Privilege | E | Only pending approvals can be resolved | none known |

## Deferred Controls

| Control | Target Phase | Reason |
|---------|--------------|--------|
| OIDC/JWT federation | Phase 4 | Identity integration; not core execution governance |
| Agent Ed25519 identity | Phase 4 | Cryptographic agent identity; nice-to-have |
| Merkle root per time window | Phase 5 | Batch verification; adds complexity |
| Signed checkpoint / export bundle | Phase 5 | Offline verification; requires key management |
| External anchoring | Phase 5 | Third-party trust; overkill for current scope |
| mTLS service-to-service | Phase 6 | Transport hardening; tunnel integration covers baseline |
| WORM sink | Phase 5 | Compliance-grade immutability; not MVP |

## Non-Claims

- This threat model is **not** a formal security audit or penetration test report.
- FerrumGate is **not** claimed to be production-ready, Tier 2 complete, or SOC2 compliant.
- STRIDE mapping identifies risks and mitigations; residual risk exists, especially for privileged insider threats.
- Unknown threats may exist; this document is updated as new attack vectors are discovered.
