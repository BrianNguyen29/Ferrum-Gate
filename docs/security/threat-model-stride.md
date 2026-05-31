# STRIDE Threat Model

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

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing (impersonate operator) | S | Bearer / scoped token auth; token lookup hash + salt | OIDC/JWT federation |
| Tampering (modify config/policy) | T | Deny-by-default scope checks; RBAC | scoped token RBAC fully implemented |
| Repudiation (deny admin action) | R | Audit log with hash chain | local CLI direct-verify; signed checkpoints |
| Information Disclosure | I | TLS in transit; token values never logged | mTLS service-to-service |
| Denial of Service | D | Rate limiting (`tower_governor`); SQLite write queue | sustained load testing incomplete |
| Elevation of Privilege | E | Scope enforcement; deny-by-default for unknown paths | no elevation path known at this time |

### B2: Agent/MCP Client → MCP Server

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | MCP server runs behind gateway; no direct adapter access | Agent Ed25519 identity |
| Tampering | T | JSON-RPC request validated by gateway before execution | Streamable HTTP MCP integrity |
| Repudiation | R | Execution provenance logged | agent-signed intent envelope |
| Information Disclosure | I | Tool schemas exposed via MCP stdio only | no public MCP endpoint without auth |
| Denial of Service | D | Rate limiting on gateway | no MCP-specific quota yet |
| Elevation of Privilege | E | Capability minting requires policy evaluation | none known |

### B3: MCP Server → Gateway

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | Local HTTP/REST to localhost; same trust domain | N/A — same trust domain |
| Tampering | T | Local HTTP/REST over localhost/TCP | N/A |
| Repudiation | R | Provenance events link MCP request to execution | N/A |
| Information Disclosure | I | Localhost only; no cross-host exposure | N/A |
| Denial of Service | D | Internal queue limits | N/A |
| Elevation of Privilege | E | Capability validation before adapter call | N/A |

### B4: Gateway → PDP

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | Same-process PDP | N/A |
| Tampering | T | Policy bundle content-hash idempotency | N/A |
| Repudiation | R | Policy simulation logs decision without side-effect | N/A |
| Information Disclosure | I | In-memory evaluation | N/A |
| Denial of Service | D | PDP evaluation timeout | complex rule DoS not formally bounded |
| Elevation of Privilege | E | PDP returns Allow/Deny; gateway enforces | N/A |

### B5: Gateway → Store

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | Local connection (SQLite) or TLS (PostgreSQL) | mTLS for PG |
| Tampering | T | Audit log hash chain; ledger hash chain | full Merkle root |
| Repudiation | R | Append-only audit log; ledger entries immutable | signed checkpoint |
| Information Disclosure | I | Connection string with credentials in config file | secrets management (vault integration) |
| Denial of Service | D | Connection pool limits; busy_timeout; write queue | sustained load testing |
| Elevation of Privilege | E | Store access via repo traits; no raw SQL exposure | none known |

### B6: Gateway → Provenance Ledger

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | Same-process write | N/A |
| Tampering | T | Ledger hash chain with `previous_ledger_hash` | external anchoring |
| Repudiation | R | Append-only; raw_json snapshot | export bundle |
| Information Disclosure | I | Same-process read | N/A |
| Denial of Service | D | WAL mode; write queue | N/A |
| Elevation of Privilege | E | LedgerRepo trait boundary | N/A |

### B7: Gateway → Adapters

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | Adapter key registry | no adapter authentication yet |
| Tampering | T | Rollback contracts; action digest | adapter-side integrity not enforced by gateway |
| Repudiation | R | Execution record with result digest | adapter attestation |
| Information Disclosure | I | Adapter credentials in config | secret rotation automation |
| Denial of Service | D | Execution timeouts | adapter resource exhaustion not bounded |
| Elevation of Privilege | E | Capability lease scoped to tool/action | none known |

### B8: Operator → Approval Resolve

| Threat | Category | Existing Control | Additional Controls / Design Considerations |
|--------|----------|------------------|----------------|
| Spoofing | S | Scoped token with `approval:resolve` | second-factor approval |
| Tampering | T | Approval state machine with valid transitions | none known |
| Repudiation | R | Audit log entry on every resolve | hash chain verifies tampering |
| Information Disclosure | I | Approval list scoped to token permissions | none known |
| Denial of Service | D | Rate limiting | none known |
| Elevation of Privilege | E | Only pending approvals can be resolved | none known |

## Additional Controls

| Control | Reason |
|---------|--------|
| OIDC/JWT federation | Identity integration; not core execution governance |
| Agent Ed25519 identity | Cryptographic agent identity; nice-to-have |
| Merkle root per time window | Batch verification; adds complexity |
| Signed checkpoint / export bundle | Offline verification; requires key management |
| External anchoring | Third-party trust; overkill for current scope |
| mTLS service-to-service | Transport hardening; tunnel integration covers baseline |
| WORM sink | Compliance-grade immutability; not MVP |

## Notes

- This threat model is **not** a formal security audit or penetration test report.
- STRIDE mapping identifies risks and mitigations; residual risk exists, especially for privileged insider threats.
- Unknown threats may exist; this document is updated as new attack vectors are discovered.
