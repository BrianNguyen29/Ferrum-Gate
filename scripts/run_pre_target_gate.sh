#!/usr/bin/env bash
# run_pre_target_gate.sh
# Local pre-target validation gate for FerrumGate v1 single-node SQLite Path 2.
# Composes local repo-side checks; does NOT require target host, SSH, or real secrets.
# Single-node only; no PostgreSQL/multi-node/HA.
# Does NOT mark G2/doc54 complete.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

FAILED=0
SKIPPED=0

run_check() {
    local name="$1"
    local cmd="$2"
    echo ""
    echo "========================================"
    echo "CHECK: $name"
    echo "========================================"
    if eval "$cmd"; then
        echo "[PASS] $name"
    else
        local status=$?
        if [[ $status -eq 0 ]]; then
            echo "[PASS] $name"
        elif [[ $status -eq 127 ]]; then
            echo "[SKIP] $name (command not found)"
            SKIPPED=$((SKIPPED + 1))
        else
            echo "[FAIL] $name"
            FAILED=$((FAILED + 1))
        fi
    fi
}

echo ""
echo "========================================"
echo "FerrumGate v1 Pre-Target Gate"
echo "========================================"
echo ""
echo "This gate runs local repo-side validation checks only."
echo "It does NOT require a target host, SSH access, or real secrets."
echo "It does NOT complete G2, does NOT authorize the pilot, and does NOT claim production-ready."
echo ""

# --- 1. Config examples validation ---

run_check "Config examples validation" \
    "bash '$SCRIPT_DIR/validate_config_examples.sh'"

# --- 2. Local restore drill ---

run_check "Local restore drill (temp SQLite)" \
    "bash '$SCRIPT_DIR/run_local_restore_drill.sh'"

# --- 3. Evidence skeleton generator ---

echo ""
echo "========================================"
echo "CHECK: Evidence skeleton generator"
echo "========================================"
SKELETON_SCRIPT="$SCRIPT_DIR/generate_evidence_skeleton.py"
if [[ -x "$SKELETON_SCRIPT" ]]; then
    if python3 -m py_compile "$SKELETON_SCRIPT" 2>/dev/null; then
        if python3 "$SKELETON_SCRIPT" --help >/dev/null 2>&1; then
            echo "[PASS] Evidence skeleton generator is valid"
        else
            echo "[FAIL] Evidence skeleton generator --help failed"
            FAILED=$((FAILED + 1))
        fi
    else
        echo "[FAIL] Evidence skeleton generator has syntax errors"
        FAILED=$((FAILED + 1))
    fi
else
    echo "[SKIP] Evidence skeleton generator not found"
    SKIPPED=$((SKIPPED + 1))
fi

# --- 4. Docs present check ---

echo ""
echo "========================================"
echo "CHECK: Required Path 2 docs present"
echo "========================================"
REQUIRED_DOCS=(
    "docs/implementation-path/61-path-2-execution-plan.md"
    "docs/implementation-path/62-path-2-operator-runbook.md"
    "docs/implementation-path/63-path-2-target-environment-spec.md"
    "docs/implementation-path/65-path-2-target-questionnaire.md"
    "docs/implementation-path/66-path-2-operator-handoff.md"
    "docs/implementation-path/59-pilot-readiness-evidence-packet.md"
    "docs/implementation-path/58-workload-compensation-drill-evidence-template.md"
)
ALL_PRESENT=true
for doc in "${REQUIRED_DOCS[@]}"; do
    if [[ -f "$REPO_ROOT/$doc" ]]; then
        echo "  [PRESENT] $doc"
    else
        echo "  [MISSING] $doc"
        ALL_PRESENT=false
        FAILED=$((FAILED + 1))
    fi
done
$ALL_PRESENT && echo "[PASS] All required Path 2 docs present"

# --- 5. Config examples present check ---

echo ""
echo "========================================"
echo "CHECK: Required config examples present"
echo "========================================"
REQUIRED_EXAMPLES=(
    "configs/examples/ferrumd.service"
    "configs/examples/ferrumgate-backup.service"
    "configs/examples/ferrumgate-backup.timer"
    "configs/examples/ferrumgate-backup.cron"
    "configs/examples/nginx-ferrumgate.conf"
    "configs/examples/nonprod-ferrumgate.toml"
    "configs/examples/ferrumd.env.example"
)
ALL_EXAMPLES_PRESENT=true
for ex in "${REQUIRED_EXAMPLES[@]}"; do
    if [[ -f "$REPO_ROOT/$ex" ]]; then
        echo "  [PRESENT] $ex"
    else
        echo "  [MISSING] $ex"
        ALL_EXAMPLES_PRESENT=false
        FAILED=$((FAILED + 1))
    fi
done
$ALL_EXAMPLES_PRESENT && echo "[PASS] All required config examples present"

# --- Summary ---

echo ""
echo "========================================"
echo "PRE-TARGET GATE RESULT"
echo "========================================"
echo ""
if [[ $FAILED -eq 0 ]]; then
    echo "ALL LOCAL CHECKS PASSED"
    echo ""
    echo "Repo prepared for Tier 1 target deployment preparation."
    echo ""
    echo "NEXT STEPS (operator-owned, per doc 65 and doc 66 Phase B):"
    echo "  1. Complete doc 65 (Target Questionnaire) — PROVIDE fields still required"
    echo "  2. Generate bearer token: openssl rand -hex 32"
    echo "  3. Adapt configs/examples to target environment"
    echo "  4. Deploy ferrumd.service and backup service to target host"
    echo "  5. Run Phase 2 probes and D1-D6 drills on target"
    echo "  6. Complete doc 59 G2.1-G2.8 and sign doc 54"
    echo ""
    echo "G2 gates remain PENDING until operator signs doc 59 and doc 54."
    echo "No production-ready claim. FerrumGate v1 remains RC-ready/conditional."
    echo ""
    exit 0
else
    echo "SOME CHECKS FAILED (FAILED=$FAILED, SKIPPED=$SKIPPED)"
    echo ""
    echo "Fix the failed checks before proceeding to Phase B."
    echo "If ferrumctl was skipped (not built), build with: cargo build --release -p ferrumctl"
    echo ""
    exit 1
fi
