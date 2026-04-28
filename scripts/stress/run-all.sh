#!/usr/bin/env bash
# run-all.sh — Master stress test runner for ferrum-gate
# Runs all sub-scripts sequentially and prints a summary table.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
TOKEN="${TOKEN:-}"
DURATION="${DURATION:-10}"
WORKERS="${WORKERS:-10}"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

declare -A RESULTS
declare -A REPORTS

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  FERRUM-GATE STRESS TEST SUITE"
echo "  BASE_URL: $BASE_URL"
echo "  WORKERS: $WORKERS  DURATION: ${DURATION}s"
echo "═══════════════════════════════════════════════════════════════"
echo ""

run_scenario() {
    local name="$1"
    local script="$2"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Running: $name"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    local start_time
    start_time=$(date +%s)
    
    local exit_code=0
    local output
    output=$("$script" --workers "$WORKERS" --duration "$DURATION" 2>&1) || exit_code=$?
    
    local end_time
    end_time=$(date +%s)
    local elapsed=$((end_time - start_time))
    
    # Extract key metrics from output
    local total_req=""
    local req_per_sec=""
    local error_pct=""
    
    total_req=$(echo "$output" | grep -E "^  Requests:" | head -1 | awk '{print $2}' | tr -d ',')
    req_per_sec=$(echo "$output" | grep -E "^  Throughput:" | awk '{print $2}')
    error_pct=$(echo "$output" | grep -E "^  Errors:" | head -1 | awk '{print $4}' | tr -d '()%')
    
    RESULTS["$name"]="$exit_code"
    REPORTS["$name"]="req=${total_req:-0}, rps=${req_per_sec:-0}, errors=${error_pct:-0}% (${elapsed}s)"
    
    echo "$output"
    
    if [[ $exit_code -eq 0 ]]; then
        echo -e "${GREEN}[PASS]${NC} $name completed successfully"
    else
        echo -e "${RED}[FAIL]${NC} $name exited with code $exit_code"
    fi
}

# Run all scenarios
run_scenario "s1-health"        "$SCRIPT_DIR/s1-health.sh"
run_scenario "s2-auth"          "$SCRIPT_DIR/s2-auth.sh"
run_scenario "s4-intent-compile" "$SCRIPT_DIR/s4-intent-compile.sh"
run_scenario "s7-sqlite-contention" "$SCRIPT_DIR/s7-sqlite-contention.sh"
run_scenario "s8-rate-limit"     "$SCRIPT_DIR/s8-rate-limit.sh"

# Print summary table
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  SUMMARY"
echo "═══════════════════════════════════════════════════════════════"
printf "%-30s %-10s %s\n" "SCENARIO" "STATUS" "METRICS"
echo "───────────────────────────────────────────────────────────────"
for name in s1-health s2-auth s4-intent-compile s7-sqlite-contention s8-rate-limit; do
    local status="${RESULTS[$name]:-99}"
    local report="${REPORTS[$name]:-}"
    if [[ "$status" -eq 0 ]]; then
        printf "%-30s ${GREEN}%-10s${NC} %s\n" "$name" "PASS" "$report"
    else
        printf "%-30s ${RED}%-10s${NC} %s\n" "$name" "FAIL ($status)" "$report"
    fi
done
echo "───────────────────────────────────────────────────────────────"
echo ""

# Overall status
total_failed=0
for name in s1-health s2-auth s4-intent-compile s7-sqlite-contention s8-rate-limit; do
    if [[ "${RESULTS[$name]:-99}" -ne 0 ]]; then
        ((total_failed++))
    fi
done

if [[ $total_failed -eq 0 ]]; then
    echo -e "${GREEN}All scenarios passed!${NC}"
    exit 0
else
    echo -e "${RED}$total_failed scenario(s) failed${NC}"
    exit 1
fi