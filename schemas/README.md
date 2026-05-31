# Schemas

This directory contains JSON Schema definitions for FerrumGate core objects.

## Files

- `jsonschema/common.json`
- `jsonschema/intent-envelope.json`
- `jsonschema/capability-lease.json`
- `jsonschema/action-proposal.json`
- `jsonschema/provenance-event.json`
- `jsonschema/rollback-contract.json`
- `jsonschema/approval-request.json`

## Notes

- These schemas are designed to map to `ferrum-proto` Rust types.
- Validate at request boundaries, persistence layers, and replay paths.
- Some cross-object invariants are documented in `contracts/` and `docs/` and cannot be fully encoded in JSON Schema.
