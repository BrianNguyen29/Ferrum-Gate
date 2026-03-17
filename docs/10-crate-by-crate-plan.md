# 10 — Crate-by-crate plan

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
