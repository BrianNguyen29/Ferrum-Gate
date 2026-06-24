# ferrum-graph

> **Status: Runtime prototype / internal-use only.**
>
> This crate is not yet stable for external consumers. APIs may change without deprecation. Integration with the main gateway is partial and evolving.

Provenance graph querying and lineage helpers for FerrumGate.

## Responsibilities

- Query provenance graphs
- Provide lineage helpers for audit and traceability

## Integration Status

- `ferrum-graph` is used internally by audit and provenance reconciliation paths.
- It is **not** a required dependency of `ferrum-gateway` or `ferrumd`.
- External users should not depend on this crate until it reaches a stable release status.
