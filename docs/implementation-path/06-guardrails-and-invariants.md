# 06 — Guardrails and invariants

> **Note**: This is a condensed guardrails summary. For the complete invariant set
> with detailed definitions, see
> [`../ferrumgate-roadmap-v1/06-constraints-and-invariants.md`](../ferrumgate-roadmap-v1/06-constraints-and-invariants.md).


## Không được phá
- intent-scoped execution
- single-use capability
- provenance-first lineage
- rollback-by-default cho side effects

## Không được bypass
- gateway
- policy
- capability validation
- rollback prepare
- provenance emission

## Không được quên
- R3 never auto-commit
- output phải sanitize
- scope không được vượt intent
