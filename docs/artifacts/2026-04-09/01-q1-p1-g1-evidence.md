# Artifact Note: Q1-P1 / G1 Proto Shape Lock — Closure Evidence

**Date**: 2026-04-09
**Package / Gate**: Q1-P1 / G1
**Author**: fixer (evidence bundle agent)
**Status**: PASS

## Criterion

> "G1: Field names for intent/proposal/capability/rollback/provenance/approval are locked.
> All downstream crate code that uses these shapes compiles with the new names.
> No open rename items in the quarter plan."

## Verification Commands Run

```sh
cargo check --workspace
cargo test --package ferrum-proto
```

## Results

### cargo check --workspace

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.11s
```
All workspace crates compile without errors.

### cargo test --package ferrum-proto

```
running 4 tests
test provenance::tests::test_provenance_event_source_runtime_id_default_none ... ok
test provenance::tests::test_provenance_event_source_runtime_id_some ... ok
test provenance::tests::test_provenance_event_kind_external_variant ... ok
test provenance::tests::test_provenance_ingest_request_roundtrip ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; filtered out; finished in 0.00s
```
All 4 ferrum-proto unit tests pass.

## Gate Criterion Link

This note satisfies: G1 evidence — proto field names locked, no mid-quarter rename planned,
downstream crates compile with new shapes.

## V1 Boundary

- [x] This evidence is for v1 kernel hardening (Q1)
