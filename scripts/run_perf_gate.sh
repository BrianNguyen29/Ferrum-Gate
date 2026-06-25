#!/usr/bin/env bash
# run_perf_gate.sh — Performance regression gate runner
#
# Usage:
#   bash scripts/run_perf_gate.sh [--dry-run] [--duration N] [--scenarios "health,intent-compile"]
#
# Defaults:
#   --dry-run:      advisory mode (always exits 0)
#   --duration:     5 seconds (short, to avoid long CI waits)
#   --scenarios:    "health,intent-compile,sqlite-contention"
#
# The script builds ferrum-stress if needed, runs the selected scenarios,
# and compares the JSON output against baselines/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=""
DURATION="5"
SCENARIOS="health,intent-compile,sqlite-contention"
BASELINES_DIR="$REPO_ROOT/baselines"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN="--dry-run"; shift ;;
        --duration) DURATION="$2"; shift 2 ;;
        --scenarios) SCENARIOS="$2"; shift 2 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

STRESS_JSON="/tmp/ferrum-stress-gate.json"

# Ensure ferrum-stress binary is available and supports --output-format
FERRUM_STRESS=""
for candidate in "$REPO_ROOT/target/debug/ferrum-stress" "$REPO_ROOT/target/release/ferrum-stress"; do
    if [[ -x "$candidate" ]] && "$candidate" --help 2>/dev/null | grep -q -- '--output-format'; then
        FERRUM_STRESS="$candidate"
        break
    fi
done

if [[ -z "$FERRUM_STRESS" ]]; then
    echo "[INFO] Building ferrum-stress..."
    cargo build --bin ferrum-stress --manifest-path "$REPO_ROOT/Cargo.toml"
    FERRUM_STRESS="$REPO_ROOT/target/debug/ferrum-stress"
fi

echo "═══════════════════════════════════════════════════════════════"
echo "  PERF GATE — ferrum-stress"
echo "  duration: ${DURATION}s  scenarios: ${SCENARIOS}"
echo "  dry-run:  ${DRY_RUN:-no}"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Run ferrum-stress with JSON output for each scenario, then merge
# We use a simple approach: run all scenarios via "all" if requested,
# but to keep duration short and control concurrency, we run individually
# and merge into a single JSON.

SCENARIOS_ARRAY=()
IFS=',' read -ra SCENARIOS_ARRAY <<< "$SCENARIOS"

FIRST=true
for scenario in "${SCENARIOS_ARRAY[@]}"; do
    concurrency=50
    # Cap concurrency for write-heavy scenarios to avoid SQLite contention
    if [[ "$scenario" == "intent-compile" || "$scenario" == "execution-pipeline" || "$scenario" == "capability" || "$scenario" == "mixed" ]]; then
        concurrency=5
    fi

    echo "[INFO] Running scenario: $scenario (concurrency=$concurrency, duration=${DURATION}s)"
    scenario_json="/tmp/ferrum-stress-${scenario}.json"

    "$FERRUM_STRESS" \
        --scenario "$scenario" \
        --concurrency "$concurrency" \
        --duration "$DURATION" \
        --output-format json \
        > "$scenario_json"

    if [[ "$FIRST" == true ]]; then
        cp "$scenario_json" "$STRESS_JSON"
        FIRST=false
    else
        # Merge scenarios array using Python
        python3 - "$STRESS_JSON" "$scenario_json" <<'PY'
import json, sys

with open(sys.argv[1], encoding="utf-8") as fh:
    merged = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    extra = json.load(fh)

merged["scenarios"].extend(extra.get("scenarios", []))
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(merged, fh, indent=2)
PY
    fi
done

echo ""
echo "[INFO] Comparing against baselines in $BASELINES_DIR..."

python3 "$SCRIPT_DIR/compare_perf_baselines.py" \
    --stress-json "$STRESS_JSON" \
    --baselines-dir "$BASELINES_DIR" \
    ${DRY_RUN:+$DRY_RUN}

EXIT_CODE=$?

if [[ -n "$DRY_RUN" ]]; then
    echo ""
    echo "[INFO] Dry-run mode: ignoring any threshold failures."
    exit 0
fi

exit "$EXIT_CODE"
