# Provenance Audit Runbook

Operational runbook for production operators and security/compliance reviewers auditing governed executions.

## Audience

- **Production operators** investigating execution behavior, unexpected terminal states, or lineage gaps
- **Security/compliance reviewers** collecting provenance evidence for internal review, incident response, or formal audit trails

## Scope

This runbook covers the supported provenance surfaces that exist today:

- `ferrumctl server inspect-execution <execution_id>`
- `ferrumctl server inspect-lineage <execution_id>`
- `ferrumctl server inspect-provenance ...`
- `ferrumctl server inspect-event <event_id>`
- `ferrumctl server ingest-external-event ...`
- `POST /v1/provenance/query`
- `POST /v1/provenance/lineage`
- `POST /v1/provenance/events/external`

**Handling rule:** treat raw provenance exports as internal data. When evidence must leave the immediate ops boundary, prepare and share a **redacted derivative by default** rather than the raw export.

---

## 1 - Complete Execution Audit

Use this flow when you need to reconstruct what happened to one governed execution from intake to terminal state.

### CLI (preferred)

```sh
# 1) Confirm execution record and terminal state
ferrumctl server inspect-execution <execution_id>

# 2) Check terminal provenance events for the execution
ferrumctl server inspect-provenance \
  --execution-id <execution_id> \
  --terminal-only

# 3) Export the full provenance stream as JSONL
ferrumctl server inspect-provenance \
  --execution-id <execution_id> \
  --all-pages > /tmp/provenance-<execution_id>.jsonl

# 4) Render the persisted lineage graph for visual review
ferrumctl server inspect-lineage <execution_id> \
  --format dot \
  --output /tmp/lineage-<execution_id>.dot
```

### curl fallback

```sh
# Execution record
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/executions/<execution_id>" | jq .

# First provenance page for the execution
curl -s -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -X POST "http://localhost:8080/v1/provenance/query" \
  -d '{
    "execution_id": "<execution_id>",
    "limit": 100
  }' | jq .

# Multi-hop lineage from a suspicious event
# (requires --ancestry and/or --descendants; max-hops validated 1-32)
ferrumctl server inspect-lineage-query \
  --execution-id "<execution_id>" \
  --event-id "<event_id>" \
  --ancestry --descendants \
  --max-hops 8 \
  --json
```

### What to check

- `inspect-execution` reports the expected execution `state`, `decision`, `started_at`, and `finished_at`
- provenance events consistently carry the same `execution_id`
- the event stream contains a coherent progression for the workflow you are auditing
- `terminal_only` returns the expected terminal event for the outcome you observed
- `inspect-lineage` and `inspect-event --ancestry --descendants` do not show broken parent-edge continuity around the suspicious event

### Escalate when

- the execution record exists but provenance is empty
- the terminal execution state conflicts with the terminal provenance event
- an expected gate event is missing around policy evaluation, capability minting, execute, verify, or rollback

---

## 2 - Investigate Missing or Broken Lineage

Use this when an event chain looks incomplete, parent edges appear inconsistent, or an execution outcome cannot be explained from lineage alone.

### CLI

```sh
# Export all events so you can inspect them offline in chronological order
ferrumctl server inspect-provenance \
  --execution-id <execution_id> \
  --all-pages > /tmp/provenance-<execution_id>.jsonl

# Inspect one suspicious event and include both directions
ferrumctl server inspect-event <event_id> \
  --ancestry \
  --descendants \
  --json

# Render the execution lineage graph
ferrumctl server inspect-lineage <execution_id> \
  --format dot \
  --output /tmp/lineage-<execution_id>.dot
```

### curl fallback

```sh
# Fetch one event with ancestry and descendants
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/provenance/events/<event_id>?ancestry=true&descendants=true" | jq .

# Query only error or terminal events for the execution
curl -s -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -X POST "http://localhost:8080/v1/provenance/query" \
  -d '{
    "execution_id": "<execution_id>",
    "terminal_only": true,
    "limit": 100
  }' | jq .
```

### Common investigation patterns

- **Execution exists, but no provenance events**
  - verify the server did not fall back to an in-memory SQLite store
  - verify you are querying the correct `execution_id`
- **Specific event has no expected ancestors**
  - inspect the event with `--ancestry`
  - check whether the event was ingested externally against the wrong parent event
- **Specific event has no expected descendants**
  - inspect with `--descendants`
  - compare the execution terminal state with the event kind to see where progress stopped
- **Lineage graph looks incomplete**
  - compare `inspect-lineage` output with the raw provenance JSONL export
  - check for `ErrorRaised`, `Quarantined`, `ApprovalDenied`, or rollback terminal events that explain early termination

---

## 3 - Verify External Event Ingest

Use this after recording an externally observed runtime signal and before relying on it for lineage-based reasoning or audit evidence.

### CLI

```sh
# Record the external event
ferrumctl server ingest-external-event \
  --execution-id <execution_id> \
  --parent-event-id <parent_event_id> \
  --source-system <system_name> \
  --source-event-id <source_event_id> \
  --summary "external runtime observed side effect" \
  --metadata-json '{"ticket":"INC-1234"}' \
  --json

# Confirm the new event appears in the execution's provenance stream
ferrumctl server inspect-provenance \
  --execution-id <execution_id> \
  --all-pages | jq -c 'select(.kind == "ExternalEventObserved")'
```

### curl fallback

```sh
curl -s -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -X POST "http://localhost:8080/v1/provenance/events/external" \
  -d '{
    "execution_id": "<execution_id>",
    "parent_event_id": "<parent_event_id>",
    "source_system": "runtime-A",
    "source_event_id": "evt-123",
    "summary": "external runtime observed side effect",
    "metadata": {"ticket": "INC-1234"}
  }' | jq .
```

### What to verify

- the ingest call succeeded and returned an `event`
- the returned event has `kind = ExternalEventObserved`
- the returned event carries the expected `execution_id`
- the event's `parent_edges` include the intended parent event
- `metadata.source_system` and `metadata.source_event_id` match the source system you recorded

### If ingest fails

- `404` or `409` typically means the `execution_id` or `parent_event_id` is wrong, missing, or mismatched
- re-check that the parent event belongs to the same execution before retrying

---

## 4 - Export Evidence for Compliance

Use this when you need to hand off provenance evidence outside the immediate operations boundary.

### Recommended workflow

1. Generate the raw internal export
2. Produce a redacted derivative
3. Retain the raw export only in the controlled internal archive
4. Share the redacted derivative unless a stronger forensic requirement is explicitly approved

### CLI

```sh
# Raw internal export (JSONL to local file)
ferrumctl server inspect-provenance \
  --execution-id <execution_id> \
  --all-pages > /tmp/provenance-raw-<execution_id>.jsonl

# Redacted derivative for sharing outside the ops boundary
jq -c 'del(.metadata.external_metadata, .metadata.payload_digest)' \
  /tmp/provenance-raw-<execution_id>.jsonl \
  > /tmp/provenance-shareable-<execution_id>.jsonl

# Optional lineage graph for human review
ferrumctl server inspect-lineage <execution_id> \
  --format dot \
  --output /tmp/lineage-<execution_id>.dot

# Optional single-event packet for an incident ticket
ferrumctl server inspect-event <event_id> \
  --ancestry \
  --descendants \
  --json > /tmp/event-<event_id>.json
```

### curl fallback

```sh
curl -s -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -X POST "http://localhost:8080/v1/provenance/query" \
  -d '{
    "execution_id": "<execution_id>",
    "limit": 100
  }' > /tmp/provenance-page-1.json
```

### Redaction checklist

Before sharing evidence outside the core operations boundary:

- review `metadata` for environment-specific identifiers or sensitive values
- remove nested `metadata.external_metadata` unless the recipient explicitly needs it
- remove `metadata.payload_digest` if it reveals sensitive upstream correlation information
- verify the export still preserves `event_id`, `kind`, `occurred_at`, `execution_id`, and parent-edge relationships needed for auditability

### Compliance checklist

- [ ] execution record captured (`inspect-execution`)
- [ ] full provenance stream exported (`inspect-provenance --all-pages`)
- [ ] lineage graph or lineage JSON captured for structural review
- [ ] any external events verified as `ExternalEventObserved`
- [ ] redacted derivative produced before broader sharing
- [ ] operator identity and export timestamp recorded in the ticket or audit record

---

## Quick Reference

| Task | Command |
|------|---------|
| Inspect execution state | `ferrumctl server inspect-execution <execution_id>` |
| Watch execution until terminal | `ferrumctl server watch-execution <execution_id> [--iterations N] [--require-terminal]` |
| Export full provenance stream | `ferrumctl server inspect-provenance --execution-id <id> --all-pages > provenance.jsonl` |
| Inspect one event with context | `ferrumctl server inspect-event <event_id> --ancestry --descendants --json` |
| Export execution lineage graph | `ferrumctl server inspect-lineage <execution_id> --format dot --output lineage.dot` |
| Ingest external event | `ferrumctl server ingest-external-event --execution-id <id> --parent-event-id <pid> --source-system <sys> --source-event-id <eid>` |
| Readiness check | `ferrumctl server ready` |

## Related Docs

- [17 - Troubleshooting](../17-troubleshooting.md)
- [15 - Deployment and Operations](../15-deployment-and-operations.md)
- [14 - API and Contracts Map](../14-api-and-contracts-map.md)
