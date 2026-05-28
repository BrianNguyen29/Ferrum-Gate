# Policy Simulation API

FerrumGate provides two side-effect-free policy simulation endpoints. Both return a policy decision without persisting proposals, bundles, provenance, or minting capabilities.

## Endpoints

| Endpoint | Purpose |
|----------|---------|
| `POST /v1/policy/simulate` | Evaluate a proposal against the **active runtime policy** (active bundles + PDP fallback). |
| `POST /v1/policy-bundles/simulate` | Evaluate a proposal against a **supplied bundle YAML** (what-if for draft bundles). |

## POST /v1/policy/simulate

### Auth scope
`policy:read`

### Request body

```json
{
  "proposal": {
    "proposal_id": "...",
    "intent_id": "...",
    "step_index": 1,
    "title": "Create file",
    "tool_name": "filesystem.write",
    "server_name": "fs-server",
    "raw_arguments": {"path": "/tmp/example.txt", "content": "hello"},
    "expected_effect": "write file",
    "estimated_risk": "Medium",
    "requested_rollback_class": "R1SnapshotRecoverable",
    "taint_inputs": [],
    "metadata": {},
    "created_at": "2026-05-28T10:00:00Z"
  },
  "intent": null
}
```

- `proposal` (required): An `ActionProposal` to evaluate.
- `intent` (optional): An `IntentEnvelope`. When omitted, a minimal intent is scaffolded from the proposal.

### Response

```json
{
  "decision": "Allow",
  "reason": "policy evaluation",
  "matched_rule_ids": [],
  "warnings": []
}
```

Response type is `EvaluateProposalResponse`:
- `decision`: `Allow`, `Deny`, `Quarantine`, or `RequireApproval`.
- `reason`: Human-readable explanation.
- `matched_rule_ids`: IDs of matched active bundle rules, if any.
- `warnings`: Advisory warnings.

### No-side-effect guarantee

The endpoint:
- Does **not** persist the proposal.
- Does **not** persist the intent.
- Does **not** persist any bundle.
- Does **not** mint capabilities.
- Does **not** emit provenance events.
- Does **not** call adapters or prepare rollback contracts.

It performs pure evaluation against the currently active policy bundles and the static PDP engine.

### How it works

1. Build or accept the provided intent envelope.
2. Compute firewall taint score from intent + proposal.
3. Evaluate active policy bundles in priority order.
4. If no bundle rule matches, fall back to the static PDP engine.
5. Return the decision immediately.

## POST /v1/policy-bundles/simulate

### Auth scope
`policy:read`

### Request body

```json
{
  "bundle_yaml": "version: \"0.1.0\"\nbundle_id: \"draft-bundle\"\nrules:\n  - id: deny.mutation\n    decision: Deny\n    priority: 100\n    matchers:\n      - type: action_is_mutation\n",
  "proposal": { ... },
  "intent": null
}
```

- `bundle_yaml` (required): The YAML content of the policy bundle to evaluate.
- `proposal` (required): The sample proposal.
- `intent` (optional): Optional intent envelope.

### Response

```json
{
  "decision": "Deny",
  "reason": "policy bundle draft-bundle matched rule deny.mutation: Deny mutating actions",
  "matched_rule_ids": ["policy_bundle:draft-bundle:deny.mutation"],
  "warnings": []
}
```

Response type is `PolicyBundleSimulateResponse` (same fields as `EvaluateProposalResponse`).

### No-side-effect guarantee

Same as `/v1/policy/simulate`: no persistence, no provenance, no capability minting.

## CLI usage

### Simulate against active runtime policy

```bash
ferrumctl policy runtime-simulate --proposal proposal.json --intent intent.json
```

### Simulate against a bundle YAML

```bash
ferrumctl policy simulate --file bundle.yaml --proposal proposal.json --intent intent.json
```

Use `--json` on either command to get JSON output.

## Distinction summary

| Aspect | `/v1/policy/simulate` | `/v1/policy-bundles/simulate` |
|--------|----------------------|------------------------------|
| Policy source | Active bundles + PDP | Supplied bundle YAML only |
| Bundle persistence | N/A (uses already-active bundles) | No (bundle is read from request) |
| Use case | "What will the live system decide?" | "What would this draft bundle decide?" |
| Request fields | `proposal`, `intent` | `bundle_yaml`, `proposal`, `intent` |
