# Handoff Note — 2026-04-09 Evening Session

## Where we are

- **Q1 exit gate passed (2026-04-09).** Q1-P7/v1.1 invariant matrix pass is SATISFIED.
  Evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`
- **Route reconciliation is complete.** 19/19 route parity confirmed between OpenAPI spec and runtime router. The `{proposal_id}` vs `{server_name}` discrepancy noted in the baseline has been resolved — the route is now correctly listed as "In v1 support contract" in the baseline.
- **WS1–WS4 are all addressed** for Q1/v1.1 gate scope:
  - WS1: rollback_class propagated from proposal at prepare
  - WS2: mark_used called at authorize
  - WS3: draft-only revalidated at prepare/evaluate gateway path
  - WS4: lineage minimum-chain integration test passes

## What changed

1. **`11-current-state-baseline.md`** — three sections updated to eliminate contradictions with Q1-P7 docs:
   - Section 2, route table row for `/v1/proposals/{proposal_id}/evaluate` — "must be reconciled" removed; now "In v1 support contract"
   - Section 6, Accepted risks — changed from itemised risks to status-as-of-Q1/v1.1 gate with evidence citations
   - Section 9, Using baseline — Q1 sentence now reflects that the Q1 exit gate is passed, not just "in progress"

2. **`docs/artifacts/2026-04-09/09-handoff-note.md`** — this file

3. **`docs/artifacts/2026-04-09/manifest.txt`** — updated to include the handoff note entry

## Likely next action

- **Q2 entry gate is satisfied.** Next session can proceed to Q2/v1.2 adapter work with confidence that v1.1 gate is closed. First Q2 items per `10-master-checklist.md`: implement fs adapter real path (skeleton → concrete). Ensure any new route added for adapter work is documented as post-v1 scope.
