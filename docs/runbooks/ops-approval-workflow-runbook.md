# Approval Workflow Runbook

## Overview

R3 (IrreversibleHighConsequence) executions require explicit operator approval before the capability is consumed. While an execution awaits approval, it remains in `AwaitingApproval` state and the capability is **not** consumed.

## States

| Approval state | Execution state | Capability consumed? |
|----------------|-----------------|---------------------|
| `Pending` | `AwaitingApproval` | No |
| `Granted` | `Authorized` | Yes |
| `Denied` | `Denied` | No |
| `Expired` | `AwaitingApproval` | No (must re-authorize) |

## Workflow

### 1. Discover pending approvals

```sh
# List all pending approvals (most recent first)
ferrumctl server inspect-approvals

# Filter by proposal or execution
ferrumctl server inspect-approvals --proposal-id <uuid>
ferrumctl server inspect-approvals --execution-id <uuid>

# Paginate through large result sets
ferrumctl server inspect-approvals --limit 10 --cursor <cursor>
```

### 2. Inspect a specific approval

```sh
ferrumctl server inspect-approval <approval_id>
ferrumctl server inspect-approval <approval_id> --json
```

### 3. Resolve (approve or deny)

```sh
# APPROVE — grants the capability and advances execution to Authorized
ferrumctl server resolve-approval <approval_id> \
  --approve \
  --actor-id <your-operator-id> \
  --actor-type Operator \
  --reason "<why this is approved>"

# DENY — leaves execution in Denied state, does NOT consume capability
ferrumctl server resolve-approval <approval_id> \
  --actor-id <your-operator-id> \
  --actor-type Operator \
  --reason "<why this is denied>"
```

### 4. Verify resolution

After resolving, confirm the approval state changed:

```sh
ferrumctl server inspect-approval <approval_id>
```

For approved executions, verify the execution advanced:

```sh
ferrumctl server inspect-execution <execution_id>
```

## Monitoring for R3 Approvals

Set up polling or alerting to catch R3 approvals before they expire (15 minutes):

```sh
# Check for any pending R3 approvals
ferrumctl server inspect-approvals --limit 50

# Script: loop until approval appears or timeout
timeout=900
interval=30
elapsed=0
while [ $elapsed -lt $timeout ]; do
  approvals=$(ferrumctl server inspect-approvals --limit 1 --json 2>/dev/null)
  count=$(echo "$approvals" | jq '[.items[] | select(.state == "Pending")] | length')
  if [ "$count" -gt 0 ]; then
    echo "Pending approvals found:"
    echo "$approvals" | jq '.items'
    break
  fi
  sleep $interval
  elapsed=$((elapsed + interval))
done
```

## Expiry

Approvals expire after 15 minutes. Expired approvals **cannot** be resolved. If an approval expires:

1. The execution remains in `AwaitingApproval` but can no longer be resolved.
2. A new approval must be created by re-authorizing the execution (re-submitting the intent/proposal).

## Authorization requirements

All approval routes require bearer authentication when `auth.mode = "bearer"`:

```sh
export FERRUMCTL_BEARER_TOKEN="<token>"
# or
ferrumctl server resolve-approval ... --bearer-token <token>
```

## Quick reference

| Task | Command |
|------|---------|
| List pending | `ferrumctl server inspect-approvals` |
| Inspect one | `ferrumctl server inspect-approval <id>` |
| Approve | `ferrumctl server resolve-approval <id> --approve --actor-id X --actor-type Operator` |
| Deny | `ferrumctl server resolve-approval <id> --actor-id X --actor-type Operator` |
| Check execution | `ferrumctl server inspect-execution <id>` |
