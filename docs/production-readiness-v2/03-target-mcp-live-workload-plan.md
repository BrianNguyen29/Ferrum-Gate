# 03 — Target-Host MCP/Live Workload Plan

> **Status**: Planning artifact. Target-host MCP smoke not yet executed.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Prove that MCP is not just local smoke, but a target-host/live governed agent path with end-to-end lifecycle evidence, redaction verification, and lineage integrity.

## Current state

- MCP stdio server has 19 tools.
- Local MCP smoke passes.
- D1–D6 local/API drills pass.
- Target-host bridge L1–L5 has evidence, but not full MCP live workload.

## Gaps

| Gap | Why |
|-----|-----|
| MCP target-host smoke missing | Local smoke does not prove target gateway behavior. |
| No governed lifecycle evidence on target | Need capability single-use, TTL enforcement, provenance chain on live gateway. |
| No live workload evidence | Need sustained agent flows with mixed adapters on target. |
| No MCP evidence artifact | Need sanitized, secret-free artifact for signoff. |

## Implementation tasks

### Layer 1 — Target-host MCP smoke

- [ ] Adapt MCP lifecycle smoke for target gateway:
  - Configurable gateway URL.
  - Bearer token from env (no printing).
  - TLS target verification.
- [ ] Verify `tools/list` returns all 19 tools.
- [ ] Verify 9 read-only tools work against target.
- [ ] Verify mutating tools fail closed without auth.
- [ ] Verify mutating lifecycle tools work with valid auth in bounded fixture.
- [ ] Verify output redaction/sanitization.
- [ ] Verify logs do not print secrets.

### Layer 2 — MCP governed lifecycle

- [ ] Run full lifecycle on target:
  ```
  submit_intent → evaluate_intent → mint_capability
  → authorize_execution → prepare_execution → execute_prepared
  → verify → query_lineage
  ```
- [ ] Prove capability single-use.
- [ ] Verify TTL max 300s enforced.
- [ ] Verify R3 does not auto-commit.
- [ ] Verify provenance chain is complete.
- [ ] Run at least one compensate path.
- [ ] Run at least one approval path.

### Layer 3 — MCP live workload

- [ ] Run repeated small workflows via agent/MCP client on target.
- [ ] Mix adapters: fs, git, http, sqlite, maildraft.
- [ ] Measure latency, error rate, readiness.
- [ ] Create evidence artifact with:
  - No secrets.
  - Pass/fail summary.
  - Request counts.
  - Error categories.
  - Sanitized lineage sample IDs.

## Workload matrix

| Flow | Adapter | Goal |
|------|---------|------|
| file write + verify + rollback | fs | validate rollback |
| git branch/commit dry path | git | validate ref capture |
| HTTP mutation bounded | http | validate idempotency/replay |
| SQLite mutation | sqlite | validate DB rollback |
| maildraft create/update/delete | maildraft | validate safe external communication draft |

## Acceptance criteria

| Gate | Criteria |
|------|----------|
| MCP-1 | Target `tools/list` returns 19 tools |
| MCP-2 | Read-only tools pass against target |
| MCP-3 | Mutating tools fail closed without auth |
| MCP-4 | Lifecycle flow passes with auth |
| MCP-5 | Provenance chain exists |
| MCP-6 | Redaction/sanitization verified |
| MCP-7 | Target evidence artifact created |

## Evidence required

- `mcp-target-smoke-evidence.md`
- `mcp-lifecycle-evidence.md`
- `mcp-live-workload-evidence.md`

## Non-claims

- **NOT production workload**: Live workload is bounded and time-boxed; not sustained production traffic.
- **NOT all adapters verified**: Only selected adapter flows in workload matrix.
- **NOT target-host verified until run**: This is a plan; evidence comes after execution.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.2, §4 Phase 3
- [`docs/implementation-path/71-mcp-server-feasibility-and-design.md`](../../implementation-path/71-mcp-server-feasibility-and-design.md)
- [`docs/implementation-path/72-mcp-server-phase-a-implementation-plan.md`](../../implementation-path/72-mcp-server-phase-a-implementation-plan.md)
