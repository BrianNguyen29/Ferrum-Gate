# 10 — Crate-by-crate plan

> **⚠️ Historical / Planning-era**: This document describes intended crate-level work at time of planning. Checkbox status here is stale — do not treat unchecked items as authoritative pending work.
>
> **For current crate status**: See `docs/implementation-path/01-current-state.md` for what exists now.
>
> **Known historical stale descriptions**:
> - `ferrum-adapter-fs`: described as "no real implementation" — 135 tests now exist with verified local slice
> - `ferrum-adapter-git`: described as "no real implementation" — 86 tests now exist with verified local slice
> - `ferrum-adapter-http`: described as "no real implementation" — 103 tests now exist with bounded replay support
> - `ferrum-adapter-sqlite`: described as "no real implementation" — 16 tests now exist with transaction rollback
> - `ferrum-adapter-maildraft`: described as "no real implementation" — 13 tests now exist with full lifecycle



## ferrum-proto
- sync đủ shapes với schemas
- thêm validation helpers nếu cần
- giữ naming ổn định

## ferrum-store
- trait repos
- sqlite implementation
- CRUD cho objects lõi

## ferrum-pdp
- policy evaluate với explainable decisions
- hard rules cho scope/taint/R3

## ferrum-cap
- TTL enforcement
- single-use enforcement
- scope subset validation
- revoke path

## ferrum-firewall
- trust labeling
- taint scoring
- contradiction checks
- DLP/sanitize

## ferrum-rollback
- lifecycle transitions
- adapter registry
- verify/compensate/rollback

## ferrum-adapter-fs
- backup trước mutate
- hash verify
- restore path

## ferrum-adapter-sqlite
- transaction wrapper
- verify predicate
- rollback

## ferrum-adapter-maildraft
- create/delete draft
- no-send hard rule

## ferrum-adapter-git
- before_ref/after_ref
- revert/reset path

## ferrum-adapter-http
- endpoint allowlist
- idempotency-aware semantics
- destructive remote mutation => R3 by default

## ferrum-graph
- lineage query helpers

## ferrum-ledger
- append-only audit trail
- optional hash chain

## ferrum-gateway
- wire full happy path
- sanitize outputs
- emit provenance

## ferrumctl
- debug/inspect/validate commands

## ferrum-testkit / tests
- fixtures
- happy path
- deny/quarantine
- rollback
- poisoned input
