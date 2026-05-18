#!/usr/bin/env bash
# validate_mcp_required_tools.sh
# Regression guard for the REQUIRED_TOOLS list in run_mcp_lifecycle_smoke.sh.
# Runs without ferrumd or MCP server — pure static validation.
# Fails if required tool names drift, count drops below 19, or duplicates exist.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_SCRIPT="$SCRIPT_DIR/run_mcp_lifecycle_smoke.sh"

if [[ ! -f "$SMOKE_SCRIPT" ]]; then
    echo "[FAIL] Smoke script not found: $SMOKE_SCRIPT" >&2
    exit 1
fi

# Extract REQUIRED_TOOLS array entries (lines inside REQUIRED_TOOLS=( ... )).
# Stop at the first closing ')' to avoid matching later quoted strings.
mapfile -t TOOLS < <(awk '/^REQUIRED_TOOLS=\(/{found=1; next} found && /^\)/{exit} found{print}' "$SMOKE_SCRIPT" | grep -E '^\s*"' | sed 's/.*"\([^"]*\)".*/\1/')

MISSING=0
COUNT=${#TOOLS[@]}

echo "[INFO] Found $COUNT required tool(s) in $SMOKE_SCRIPT"

if [[ "$COUNT" -lt 19 ]]; then
    echo "[FAIL] REQUIRED_TOOLS count ($COUNT) is below minimum 19" >&2
    MISSING=$((MISSING + 1))
fi

# Check for empty names
for tool in "${TOOLS[@]}"; do
    if [[ -z "$tool" ]]; then
        echo "[FAIL] Empty tool name found in REQUIRED_TOOLS" >&2
        MISSING=$((MISSING + 1))
    fi
done

# Check prefix
for tool in "${TOOLS[@]}"; do
    if [[ -n "$tool" ]] && [[ ! "$tool" =~ ^ferrum_gate_ ]]; then
        echo "[FAIL] Tool name does not start with 'ferrum_gate_': $tool" >&2
        MISSING=$((MISSING + 1))
    fi
done

# Check duplicates
SEEN=""
for tool in "${TOOLS[@]}"; do
    if echo "$SEEN" | grep -qxF "$tool"; then
        echo "[FAIL] Duplicate tool name in REQUIRED_TOOLS: $tool" >&2
        MISSING=$((MISSING + 1))
    fi
    SEEN="$SEEN$tool"
done

if [[ "$MISSING" -gt 0 ]]; then
    echo "[FAIL] MCP REQUIRED_TOOLS validation failed with $MISSING issue(s)" >&2
    exit 1
fi

echo "[PASS] MCP REQUIRED_TOOLS validation passed: $COUNT tools, no empty names, correct prefix, no duplicates"
exit 0
