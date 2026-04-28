# Q1-P5 — Minimum Chain Evidence

**Date:** 2026-04-09
**Package:** Q1-P5
**Status:** SATISFIED (conservative minimum chain)
**Evidence type:** Integration test + code citation

---

## Q1-P5 Package Objective

> Gate C (step 1.4 + 1.5 → 1.6): Both cap enforcement and rollback fix must pass before
> end-to-end lineage test. Satisfy the minimum chain as a conservative slice pass — not a
> full claim of Gate C closure beyond the Q1-P4 package dimension.

---

## What Q1-P5 Demonstrates

The lineage chain test (`integration_lineage_chain.rs`) exercises the full authorize → prepare → compensate path over the existing gateway HTTP surface and asserts that the minimum chain (authorize event + prepare event + terminal-present event) is returned by the lineage query endpoint.

**The chain is satisfied over the existing gateway execution surface.** There is no literal `/v1/executions/execute` endpoint — the execute step is implicit in the state machine (authorize transitions execution to `Prepared`; prepare transitions to `Prepared`; compensate transitions to `Compensated`). The conservative wording reflects this: the minimum chain is demonstrated, not a specific execute-path claim.

---

## Code Evidence

### Authorize event emission — `server.rs:496`

```rust
// Emit provenance event for authorization (Q1-P5 conservative chain: authorize).
let auth_event = ProvenanceEvent {
    event_id: EventId::new(),
    kind: ferrum_proto::ProvenanceEventKind::ActionProposalSubmitted,
    ...
};
```

### Prepare event emission — `server.rs:621`

```rust
// Emit provenance event for preparation (Q1-P5 conservative chain: prepare).
let prepare_event = ProvenanceEvent {
    event_id: EventId::new(),
    kind: ferrum_proto::ProvenanceEventKind::SideEffectPrepared,
    ...
};
```

### Terminal-present event emission — `server.rs:748`

```rust
// Emit provenance event for compensation completion (Q1-P5 conservative chain: terminal-present).
let terminal_event = ProvenanceEvent {
    event_id: EventId::new(),
    kind: ferrum_proto::ProvenanceEventKind::SideEffectCompensated,
    ...
};
```

### Lineage query endpoint — `integration_lineage_chain.rs:268`

```rust
let request = Request::builder()
    .method(Method::GET)
    .uri(format!("/v1/provenance/lineage/{}", execution_id))
    ...
```

Lineage query returns 3 events (authorize + prepare + terminal-present) — minimum chain verified.

---

## Absent Execute Endpoint — Explicit Disclaimer

**There is no literal `/v1/executions/execute` endpoint in `server.rs`.** No handler is registered at that path. The execute step is handled implicitly by the state machine transitions:

| Step | HTTP endpoint | State transition |
|------|--------------|-------------------|
| Authorize | `POST /v1/executions/authorize` | → `Prepared` |
| Prepare | `POST /v1/executions/{id}/prepare` | → `Prepared` (confirms rollback contract) |
| Compensate | `POST /v1/executions/{id}/compensate` | → `Compensated` (terminal) |

Q1-P5 does **not** claim a literal execute endpoint. The minimum chain is demonstrated over the existing surface.

---

## Test Evidence

- `integration_lineage_chain.rs` — end-to-end test walks authorize → prepare → compensate → lineage query
- Lineage response contains exactly 3 events (authorize + prepare + terminal-present)
- Build: `cargo check --workspace` → PASS
- Integration tests: `cargo test --package ferrum-integration-tests` → PASS

---

## Gate C Conservative Wording

> **Gate C (Q1-P5) — SATISFIED (conservative minimum chain):**
> Q1-P4 (P4a mark_used at authorize + P4b rollback_class propagation) satisfies Gate C on the Q1-P4 package dimension. The minimum chain (authorize + prepare + terminal-present) is confirmed over the existing gateway execution surface via `integration_lineage_chain.rs`. A literal `/v1/executions/execute` endpoint is absent; no such endpoint is claimed. Gate C criterion is satisfied as a conservative slice pass — not a claim of full Q1 exit gate closure.

---

## Summary

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Minimum chain (authorize + prepare + terminal-present) over existing surface | SATISFIED | `server.rs:496,621,748`; `integration_lineage_chain.rs:268` |
| Literal execute endpoint | ABSENT | No `/v1/executions/execute` handler in `server.rs`; not claimed |
| Gate C conservative claim | PASS | This note uses conservative wording; no overclaim |

**Q1-P5: SATISFIED (conservative minimum chain)** — combined with Q1-P4 combined closure (`05-q1-p4-combined-closure-note.md`), Gate C is satisfied on the Q1-P4 package dimension.
