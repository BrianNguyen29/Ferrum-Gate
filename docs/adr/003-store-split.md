# ADR 003 — SQLite/PostgreSQL Store Split

## Status
Accepted

## Context

FerrumGate started with SQLite as the only store. As deployment scenarios expanded, we needed:
- Higher write throughput than a single SQLite connection can provide
- Cross-process access (e.g., multiple ferrumd instances or external tools)
- Docker/container-friendly persistence without volume locking concerns

PostgreSQL is the only alternative supported; MySQL is explicitly out of scope.

## Decision

- **SQLite** remains the default for single-node, single-process deployments. It uses WAL mode with conservative pragmas (`synchronous=NORMAL`, `busy_timeout=5000ms`).
- **PostgreSQL** is feature-gated behind `--features postgres` and selected at runtime via a DSN starting with `postgres://` or `postgresql://`.
- The `StoreFacade` trait abstracts both backends so the gateway and adapters are backend-agnostic.
- Embedded migrations are applied automatically on startup for both backends.

## Consequences

- **Positive**: Single-node users pay no extra dependency cost (PostgreSQL is opt-in).
- **Positive**: PostgreSQL path is bounded to local Docker/runtime only; no HA/multi-node claims.
- **Negative**: Two code paths to maintain for every store operation (though the trait reduces surface area).
- **Negative**: PostgreSQL feature adds compile-time and dependency cost when enabled.
