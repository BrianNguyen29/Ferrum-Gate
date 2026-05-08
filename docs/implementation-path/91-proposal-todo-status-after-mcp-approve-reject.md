# 91 — Proposal Todo/Status After MCP Approve/Reject Enablement

> **Status**: Created/updated 2026-05-08 after local MCP approve/reject implementation
> **Purpose**: Comprehensive todo/status record for all feasible proposals; fix stale approve/reject blocked/planning-only text
> **Scope**: Single-node SQLite v1; RC-ready/conditional; no G2/target/pilot/signoff claims
> **Evidence base**: commit e57bb8f, smoke 15/15, local MCP approve/reject implemented and verified

---

## 1. MCP Approve/Reject — Status: COMPLETED (Local)

| Item | Status | Evidence |
|------|--------|----------|
| Gateway resolve endpoint | ✅ Implemented | `/v1/approvals/{id}/resolve` POST |
| MCP approve tool (`ferrum_gate_approve_intent`) | ✅ Implemented | Dispatches to gateway resolve |
| MCP reject tool (`ferrum_gate_reject_intent`) | ✅ Implemented | Dispatches to gateway resolve |
| MCP list approvals (`ferrum_gate_list_approvals`) | ✅ Implemented | Read-only dispatch |
| Approve/reject smoke (D1.9) | ✅ 15/15 | `run_mcp_lifecycle_smoke.sh` |
| D1.11 smoke evidence update | ✅ Fixed | doc89 now shows 15/0 not 13/0 |

**Explicit non-claims**:
- Local approve/reject implementation does NOT complete G2
- Local approve/reject implementation does NOT provide target evidence
- Local approve/reject implementation does NOT authorize production pilot use
- Local approve/reject implementation does NOT change operator signoff status
- Direct MCP provenance emission remains forbidden; gateway owns provenance

**Stale text corrections applied 2026-05-08**:
- README line 76: removed `(planning only; not implemented)` → now says "implemented locally; smoke 15/15; G2/target evidence/pilot/signoff not claimed"
- doc89 lines 31, 83, 93: removed `approve/reject permanently blocked` → now says "approve/reject implemented locally but G2 not completed"
- doc89 evidence: updated from `Passed: 13, Failed: 0` → `Passed: 15, Failed: 0`
- doc90 Explicit Non-Claims: removed `This plan does not implement approve/reject` → now says `Local approve/reject implementation does not complete G2`
- Script summary lines 546-552: updated from `17-tool registry, 8 lifecycle tools wired, blocked approve/reject behavior` → `19-tool registry, 8 lifecycle + 2 approval tools wired, approve/reject dispatch error checks`

---

## 2. D1.11 Smoke — Status: COMPLETED

| Item | Status | Evidence |
|------|--------|----------|
| Script syntax validation | ✅ Passed | `bash -n run_mcp_lifecycle_smoke.sh` |
| D1.11.1 submit_intent dispatch | ✅ Passed | Soft-pass on -32003/-32004 |
| D1.11.2 evaluate_intent dispatch | ✅ Passed | Soft-pass on -32003/-32004 |
| D1.11.3 mint_capability dispatch | ✅ Passed | Soft-pass on -32003/-32004 |
| D1.11.4 list_intents dispatch | ✅ Passed | Soft-pass on -32003/-32004 |
| 19-tool registry (9 read-only + 8 lifecycle + 2 approval) | ✅ Verified | tools/list returns 19 |

---

## 3. Security Hardening Local/Manual Plan — Status: DOCUMENTED

> Reference: [`70-security-hardening-local-only-plan.md`](./70-security-hardening-local-only-plan.md)

| Category | Status | Notes |
|----------|--------|-------|
| Group A — Current Controls | ✅ Implemented | Bearer auth, rate limiting, capability TTL, single-use, SQLite write queue, FK constraints |
| Group B — Operator-Owned Controls | ☐ Pending | Token rotation, TLS termination, backup automation, restore drill, CORS opt-in |
| Group C — Deferred Controls | ⬜ Deferred | Durable capability persistence, PostgreSQL, DLP stub, HTTP replay breadth |
| Group D — Local/Manual Audit Commands | ✅ Documented | cargo deny, cargo audit, format/lint check (NOT CI) |
| Group E — Proxy-Owned Controls | ✅ Documented | Request body size limit, CORS preflight, TLS client certs |

**No CI additions claimed. No cargo-deny/cargo-audit in CI.**

---

## 4. Path 2 Target-Value Blockers — Status: BLOCKED (Pending Operator)

> Reference: [`71-path-2-target-values-intake-packet.md`](./71-path-2-target-values-intake-packet.md)

| Blocker | Severity | Owner | Status |
|---------|----------|-------|--------|
| Real operator name/role/contact | Critical | Operator | ☐ Pending |
| Target host FQDN or IP | Critical | Infra/operator | ☐ Pending |
| SSH host/user/access method | Critical | Infra/operator | ☐ Pending |
| Service user/group, install directory | Critical | Infra/operator | ☐ Pending |
| Config file path, SQLite store path | Critical | Operator | ☐ Pending |
| Bearer token generation | Critical | Security/operator | ☐ Pending |
| TLS certificate/private key | High | Security/operator | ☐ Pending |
| nginx upstream/proxy config | High | Infra/operator | ☐ Pending |
| Backup/restore drill evidence | High | Operator | ☐ Pending |
| G2 evidence packet | High | Operator | ☐ Pending after target run |
| Operator signoff (doc 54) | Critical | Operator | ☐ Pending |

**Doc 71 explicitly prohibits**: copying dummy/local values into docs 54, 59, 63, 65 as real values.

---

## 5. Deferred Phases — Status: DEFERRED

### 5.1 HTTP Retry/Backoff with Rollback Semantics

> Reference: doc33 §P5, doc67 P2.1

| Aspect | Status | Notes |
|--------|--------|-------|
| Bounded http.replay_v1 (POST/PUT/PATCH) | ✅ Implemented | 103 tests; exact URL/digest binding |
| Connection pooling/keepalive policy | ⬜ Deferred | Post-v1 scope |
| Full retry/backoff policy coverage | ⬜ Deferred | Bounded semantics verified; broader coverage post-v1 |
| Timeout and cancellation | ⬜ Deferred | Post-v1 scope |
| TLS trust/cert pinning | ⬜ Deferred | Post-v1 scope |

**Rationale**: Bounded replay is verified for local smoke; production-grade retry/backoff beyond the verified slice is post-v1.

### 5.2 Lineage/Provenance Enhancement

| Aspect | Status | Notes |
|--------|--------|-------|
| Cross-runtime Provenance Fabric (U3) | ✅ Implemented | `ExternalEventSource`, POST `/v1/provenance/ingest` |
| Direct MCP provenance emission | ❌ Forbidden | Gateway must own provenance emission |
| Provenance for approve/reject | ✅ Implemented | `ApprovalGranted`/`ApprovalDenied` via gateway |

**Rationale**: MCP must not emit provenance directly per doc90 design constraint.

### 5.3 DLP Semantic Enhancement

> Reference: [`86-mcp-server-d1-9-dlp-field-redaction-preflight.md`](./86-mcp-server-d1-9-dlp-field-redaction-preflight.md)

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1 + Phase 2 | ✅ Implemented | `dlp_findings` stub, `sanitize_output` control-char-only |
| Option B (raw_arguments/metadata first) | ✅ Oracle-approved 2026-05-07 | Explorer recommends |
| Full DLP semantic scanning | ⬜ Deferred | Post-v1 scope per design |

**Rationale**: DLP stub is explicit; full content inspection not required for v1.

---

## 6. Phase 3 Block — Status: BLOCKED (Pending Path 2)

> Reference: [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 3

| Gate | Owner | Status |
|------|-------|--------|
| G3.1 v1 RC tag cut | Release engineer | ✅ DONE (v0.1.0-rc.1 at 5fce844d) |
| G3.2 Path 2 pilot confirmed | Operator | ☐ Pending |
| G3.3 Engineering capacity confirmed | Engineering lead | ☐ Pending (~2000-3000 LOC) |
| G3.4 ADR-50 Phase P1 reviewed | Engineering lead | ☐ Pending |

**What Phase 3 is NOT**:
- NOT an extension of v1 RC tag
- NOT minor feature addition
- NOT covered by v1 single-node support contract
- PostgreSQL/multi-node/HA out of scope until Phase 3 complete

---

## 7. Proposal Status Summary

| Proposal | Status | Evidence |
|----------|--------|----------|
| MCP approve/reject (local) | ✅ Completed | smoke 15/15, commit e57bb8f |
| D1.11 live-local smoke | ✅ Completed | doc89 |
| MCP D1.7 tool dispatch (8 lifecycle) | ✅ Completed | doc84 |
| MCP D1.9 DLP field redaction | ✅ Implemented (Phase 1+2) | doc86 |
| MCP output sanitization | ✅ Implemented (bounded) | doc85 |
| Security hardening (local/manual) | ✅ Documented | doc70 |
| Path 2 target-value intake | ☐ Pending | doc71 (operator-owned) |
| G2 evidence/signoff | ☐ Pending | doc54/doc59 (operator-owned) |
| HTTP retry/bounded replay | ✅ Bounded slice done | doc33 P5 |
| HTTP retry/backoff full | ⬜ Deferred post-v1 | doc33 P5 |
| DLP semantic full | ⬜ Deferred post-v1 | doc86 |
| Durable capability persistence | ⬜ Deferred post-v1 | doc70 Group C |
| PostgreSQL/HA | ⬜ Deferred Phase 3 | doc31 Path 3 |
| Phase 3 (P1-P4) | 🔒 Blocked | Pending Path 2 pilot |

**Status key**:
- ✅ = Completed/Implemented (local)
- ☐ = Pending/Blocked (operator-owned)
- ⬜ = Deferred (post-v1)
- 🔒 = Blocked on upstream gate

---

## 8. Linked Documents

| This Doc | Links To | Purpose |
|----------|----------|---------|
| This doc | [README.md](./README.md) | Entry point |
| This doc | [doc89](./89-mcp-server-d1-11-live-local-smoke.md) | D1.11 smoke evidence |
| This doc | [doc90](./90-mcp-approve-reject-enable-plan.md) | Approve/reject plan |
| This doc | [doc70](./70-security-hardening-local-only-plan.md) | Security hardening |
| This doc | [doc71](./71-path-2-target-values-intake-packet.md) | Path 2 intake |
| This doc | [doc31](./31-release-paths-todo.md) | Release paths |
| This doc | [doc33](./33-feature-completion-backlog.md) | Feature backlog |
| This doc | [artifacts/2026-05-08-mcp-live-local-smoke-d1-11.md](./artifacts/2026-05-08-mcp-live-local-smoke-d1-11.md) | Smoke artifact (pre-approve/reject) |

---

*Document created: 2026-05-08. Updated after local MCP approve/reject enablement. No G2/target/pilot/signoff claims made.*
