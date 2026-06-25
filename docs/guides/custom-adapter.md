# Custom Adapter Developer Guide

> **Parent**: [`guides/README.md`](./README.md)

---

## Overview

This guide explains how to add a custom side-effect adapter to FerrumGate. Adapters are the bridge between the gateway governance lifecycle and external systems.

> **Stability warning**: The internal adapter registration API is subject to change. This guide describes the conceptual contract. Refer to the source code in `ferrum-adapters` and `ferrum-gateway` for the current implementation.

## Adapter contract

An adapter implements four phases:

| Phase | Purpose | Failure behavior |
|-------|---------|------------------|
| **Prepare** | Capture pre-state, validate inputs, create rollback contract | Fails the proposal before side effects |
| **Execute** | Perform the side effect | Fails closed; provenance records the failure |
| **Verify** | Confirm the side effect succeeded | If verify fails, triggers compensation |
| **Rollback / Compensate** | Revert or repair the mutation | Uses the prepare snapshot |

## Action binding

The gateway maps a `tool_name` to an adapter/action pair via `ActionBinding`:

```json
{
  "action_type": "CustomAction",
  "adapter_key": "my_adapter"
}
```

Unknown mutating tools fail closed unless the proposal carries an explicit binding.

## Risk classes

Adapters must declare a default risk class:

| Class | Meaning | Example |
|-------|---------|---------|
| R0 | Read-only, no side effects | Query, list |
| R1 | Mutating, easily reversible | File write with snapshot |
| R2 | Destructive or hard to reverse | File delete, schema drop |
| R3 | Requires human approval | Production deployment |

## Registration

1. Add your adapter crate or module under the workspace.
2. Implement the `Adapter` trait (or equivalent internal interface): `prepare`, `execute`, `verify`, `rollback`.
3. Register the adapter in the gateway bridge so `tool_name` resolves to your adapter key.
4. Add tests: prepare snapshot roundtrip, execute+verify success, rollback restores pre-state, and unknown tool rejection.

## Testing checklist

- [ ] Prepare captures rollback data deterministically.
- [ ] Execute performs the actual side effect.
- [ ] Verify detects both success and failure.
- [ ] Rollback restores the exact pre-state or cleans up safely.
- [ ] Unknown tool names fail closed.
- [ ] Risk class is enforced by policy.

## Example: conceptual skeleton

```rust
// Conceptual only — not a stable API
impl Adapter for MyAdapter {
    fn prepare(&self, req: PrepareRequest) -> Result<RollbackContract, AdapterError>;
    fn execute(&self, req: ExecuteRequest) -> Result<ExecuteResult, AdapterError>;
    fn verify(&self, req: VerifyRequest) -> Result<bool, AdapterError>;
    fn rollback(&self, contract: RollbackContract) -> Result<(), AdapterError>;
}
```

## Related docs

- [`adapter-reference.md`](./adapter-reference.md) — First-party adapter operations and rollback behavior.
- [`concepts.md`](./concepts.md) — Intent, capability, provenance, and risk classes.
- [`docs/adr/000-adapter-port.md`](../../docs/adr/000-adapter-port.md) — Architecture decision record for adapter ports.
