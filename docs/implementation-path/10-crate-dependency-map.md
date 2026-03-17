# 10 — Crate dependency map

## Lowest layer
- ferrum-proto

## Core logic
- ferrum-pdp
- ferrum-cap
- ferrum-firewall
- ferrum-rollback

## Storage / audit
- ferrum-store
- ferrum-graph
- ferrum-ledger

## Adapters
- ferrum-adapter-*

## Orchestration
- ferrum-gateway

## Rule tránh cycle
- proto không phụ thuộc crate nội bộ khác
- adapters không phụ thuộc gateway
- gateway là top layer
