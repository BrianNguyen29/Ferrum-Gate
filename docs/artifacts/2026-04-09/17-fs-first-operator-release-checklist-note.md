# fs-first FileWrite — Operator-Facing Release Checklist Note

**Date:** 2026-04-11  
**Scope:** fs-first FileWrite slice (FileWrite-only, FsAdapter)  
**Audience:** Operators evaluating the beta slice

---

## Release Status: BETA — demo-ready · not production-ready

The fs-first FileWrite slice is **ready for beta evaluation** in demo environments only. It is **not production-ready**.

### ✅ Demo-Ready Because

- HTTP prepare → execute → verify → compensate end-to-end flow works (FileWrite only)
- GET /v1/executions/{id} returns rollback_contract data with meaningful inspectable state after each phase
- 409 invalid-state guards prevent out-of-order execute/verify/compensate calls
- SQLite persistence: contract metadata, plan, and checks survive server restart
- Single-use capability enforcement works at the gateway layer

### 🚫 Not Production-Ready Because

| Reason | Description |
|--------|-------------|
| Git/SQLite adapters not implemented | Only FileWrite via FsAdapter is supported; Git and SQLite adapters are not started |
| No idempotency guarantees | Re-calling execute on a Running execution is not safe; mid-execute failure not atomically cleaned up |
| Limited error surface coverage | Error model has known rough edges documented in `15-gateway-execute-verify-compensate-error-model.md` |
| No load/stress testing | Concurrency limits, connection pooling, and high-throughput behavior untested |

### 📋 Beta Exit Criteria (to production-ready)

1. **Idempotency for execute/verify** — guarantee safe re-call on Running/AwaitingVerification states
2. **Atomic cleanup on mid-execute failure** — handle partial file writes with proper rollback
3. **Git adapter** — implement ferrum-adapter-git for git-based snapshot/rollback
4. **SQLite adapter** — implement ferrum-adapter-sqlite for DB-based snapshot/rollback
5. **Load testing pass** — demonstrate stable behavior under concurrent execution load

---

## What Operators Can Evaluate Now

| Feature | Status |
|---------|--------|
| End-to-end FileWrite lifecycle (prepare → execute → verify → compensate) | ✅ Works |
| Inspect execution state via GET /v1/executions/{id} | ✅ Works |
| rollback_contract presence and state transitions | ✅ Works |
| 409 guards for out-of-order calls | ✅ Works |
| SQLite persistence across restarts | ✅ Works |
| Git/SQLite adapters | 🚫 Not implemented |
| Idempotent execute/verify | 🚫 Not implemented |
| Atomic mid-execute cleanup | 🚫 Not implemented |

## Cross-Reference

- Error model: `15-gateway-execute-verify-compensate-error-model.md`
- Design note: `11-gateway-execute-verify-surface-design-note.md`
- Beta readiness: `16-fs-first-beta-readiness-note.md`
- Happy-path evidence: `13-happy-path-execute-verify-evidence.md`
- fs-first foundation: `10-q2-fs-foundation-evidence.md`
