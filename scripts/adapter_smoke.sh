#!/usr/bin/env bash
# adapter_smoke.sh
# Bounded, non-Docker, shape-only adapter smoke for FerrumGate.
#
# Runs deterministic, offline checks only:
#   1. ferrum-adapter-s3  library (shape-only) unit tests
#   2. ferrum-adapter-gcs library (shape-only) unit tests
#   3. MCP REQUIRED_TOOLS static validation (no ferrumd / MCP server required)
#
# Explicitly OUT of scope here:
#   - Live MinIO / GCS integration tests (gated behind s3-live-tests / #[ignore])
#   - MCP stdio lifecycle dispatch smoke (run separately via
#     scripts/run_mcp_lifecycle_smoke.sh and the advisory CI step)
#   - MCP HTTP/SSE transport smoke (no automated coverage)
#   - ferrum-stress scenarios
#
# This script does NOT claim live cloud, G2, or production-ready behavior.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PASSED=0
FAILED=0

run_check() {
    local name="$1"
    shift
    echo ""
    echo "==> $name"
    if "$@"; then
        echo "[PASS] $name"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] $name" >&2
        FAILED=$((FAILED + 1))
    fi
}

echo "========================================"
echo "ADAPTER SMOKE (shape-only, non-Docker)"
echo "========================================"

run_check "S3 adapter shape-only unit tests" \
    cargo test -p ferrum-adapter-s3 --lib --manifest-path "$REPO_ROOT/Cargo.toml"

run_check "GCS adapter shape-only unit tests" \
    cargo test -p ferrum-adapter-gcs --lib --manifest-path "$REPO_ROOT/Cargo.toml"

run_check "MCP REQUIRED_TOOLS static validation" \
    bash "$REPO_ROOT/scripts/validate_mcp_required_tools.sh"

echo ""
echo "========================================"
echo "ADAPTER SMOKE SUMMARY"
echo "========================================"
echo "Passed: $PASSED"
echo "Failed: $FAILED"
echo ""
echo "Scope: shape-only S3/GCS library tests + static MCP tool-contract validation."
echo "Live S3 (MinIO) and GCS SDK paths, MCP stdio lifecycle dispatch, and MCP"
echo "HTTP/SSE transport are NOT covered by this smoke and remain gated/experimental."
echo "This does NOT claim live cloud, G2, or production-ready behavior."
echo ""

if [[ "$FAILED" -eq 0 ]]; then
    echo "ADAPTER SMOKE: ALL CHECKS PASSED"
    exit 0
fi

echo "ADAPTER SMOKE: SOME CHECKS FAILED" >&2
exit 1
