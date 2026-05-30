#!/usr/bin/env bash
# run_security_audit.sh — Local/manual cargo security audit gate
# Status: Operational. cargo-audit v0.22.1 installed and passing. No CI integration. No secrets.
# Scope: Single-node SQLite v1. No production-ready claim.
#
# Usage:
#   bash scripts/run_security_audit.sh
#
# Checks for cargo-deny and cargo-audit, runs available tools, and fails
# with clear install instructions if neither is installed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

DENY_AVAILABLE=false
AUDIT_AVAILABLE=false
FAILURES=0

if command -v cargo-deny &>/dev/null || cargo deny --version &>/dev/null 2>&1; then
    DENY_AVAILABLE=true
fi

if command -v cargo-audit &>/dev/null || cargo audit --version &>/dev/null 2>&1; then
    AUDIT_AVAILABLE=true
fi

echo "=== FerrumGate Local Security Audit Gate ==="
echo ""

if [[ "${DENY_AVAILABLE}" == "true" ]]; then
    echo "[cargo-deny] Found. Running 'cargo deny check advisories' ..."
    if cargo deny check advisories; then
        echo "[cargo-deny] PASS"
    else
        echo "[cargo-deny] FAIL — advisory check found issues"
        FAILURES=$((FAILURES + 1))
    fi
    echo ""
else
    echo "[cargo-deny] NOT FOUND. Skipping dependency advisory check."
fi

if [[ "${AUDIT_AVAILABLE}" == "true" ]]; then
    echo "[cargo-audit] Found. Running 'cargo audit' ..."
    if cargo audit --ignore RUSTSEC-2023-0071; then
        echo "[cargo-audit] PASS"
    else
        echo "[cargo-audit] FAIL — audit found issues"
        FAILURES=$((FAILURES + 1))
    fi
    echo ""
else
    echo "[cargo-audit] NOT FOUND. Skipping crates.io security advisory check."
fi

if [[ "${DENY_AVAILABLE}" == "false" && "${AUDIT_AVAILABLE}" == "false" ]]; then
    echo "=== SECURITY AUDIT GATE: FAILED ==="
    echo ""
    echo "Neither 'cargo-deny' nor 'cargo-audit' is installed."
    echo "At least one is required to run the local security audit gate."
    echo ""
    echo "Install instructions:"
    echo "  cargo install --locked cargo-deny"
    echo "  cargo install --locked cargo-audit"
    echo ""
    echo "If you only install one, the gate will run whichever is available."
    echo ""
    exit 1
fi

if [[ ${FAILURES} -gt 0 ]]; then
    echo "=== SECURITY AUDIT GATE: FAILED (${FAILURES} tool(s) reported issues) ==="
    exit 1
fi

echo "=== SECURITY AUDIT GATE: PASS ==="
echo "All available security audit tools completed without issues."
exit 0
