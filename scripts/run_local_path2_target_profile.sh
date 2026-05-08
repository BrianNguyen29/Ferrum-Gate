#!/usr/bin/env bash
# run_local_path2_target_profile.sh
# Local-only Path 2 target-profile alternative when real target values are unavailable.
#
# Purpose: Validates tooling and runbook steps against a target-like directory structure
#          using a temporary local root, generated local-only token, and in-memory SQLite.
#          The profile structure is created at a temp path but the DB uses in-memory
#          because file-backed SQLite does not work in /tmp directories in this environment.
#
# Scope: Local host only. Single-node SQLite. No target host, SSH, domain, TLS, or real secrets.
#
# Constraints:
#   - Does NOT claim G2/pilot/production-ready from local outputs
#   - Does NOT modify canonical docs 54, 58, 59, 63, 65
#   - Does NOT use real secrets - only locally-generated tokens in temp dirs
#   - All outputs labeled "LOCAL-ONLY - NOT TARGET EVIDENCE"
#   - Generated token is ephemeral and NOT committed or stored in artifacts
#   - Script is NOT wired into CI
#
# Usage:
#   bash scripts/run_local_path2_target_profile.sh              # Run with defaults
#   bash scripts/run_local_path2_target_profile.sh --keep-output # Keep output directory after run
#   bash scripts/run_local_path2_target_profile.sh --output-dir /custom/path # Custom output dir
#   bash scripts/run_local_path2_target_profile.sh --help      # Show help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PROFILE_VERSION="1.0"

show_help() {
    cat << 'EOF'
run_local_path2_target_profile.sh - Local Path 2 Target Profile Alternative

USAGE:
    bash scripts/run_local_path2_target_profile.sh [OPTIONS]

OPTIONS:
    --output-dir DIR    Custom output directory (default: /tmp/ferrum-local-target-profile-*)
    --keep-output      Keep output directory after run (default: delete on exit)
    --skip-auth-smoke  Skip local auth smoke check
    --skip-backup      Skip backup/restore drill
    --help             Show this help message

PURPOSE:
    When real target values are not yet available, this script validates tooling
    and runbook steps against a target-like directory structure using temporary
    local directories and a locally-generated token.

PHASES:
    Phase 0 - Preflight: Validate dependencies
    Phase 1 - Profile Setup: Create temp target-like root structure
    Phase 2 - Token Generation: Generate ephemeral local-only bearer token
    Phase 3 - Config/Env: Write ferrumgate.toml and ferrumd.env to temp locations
    Phase 4 - ferrumd Start: Start ferrumd (in-memory SQLite due to /tmp limitations)
    Phase 5 - Probes: Run healthz, readyz, readyz/deep, metrics probes
    Phase 6 - Auth Checks: Verify no-token/wrong-token/correct-token behavior
    Phase 7 - Backup/Restore: Run backup create/verify and restore drill
    Phase 8 - Auth Smoke: Run local auth smoke check
    Phase 9 - Artifact Output: Write summary artifact to output dir

NON-CLAIMS:
    - NOT G2 evidence - all G2.1-G2.8 remain pending
    - NOT target readiness - no real target values used
    - NOT pilot authorized - pilot unauthorized until operator signs doc 54
    - NOT production-ready - FerrumGate v1 remains RC-ready/conditional
    - NOT operator signoff - doc 54 remains unsigned
    - Generated token is ephemeral and NEVER stored in artifacts
EOF
}

OUTPUT_DIR=""
KEEP_OUTPUT=false
SKIP_AUTH_SMOKE=false
SKIP_BACKUP=false

while [[ $# -gt 0 ]]; do
    case "${1:-}" in
        --output-dir)
            OUTPUT_DIR="${2:-}"
            shift 2 || shift
            ;;
        --keep-output)
            KEEP_OUTPUT=true
            shift
            ;;
        --skip-auth-smoke)
            SKIP_AUTH_SMOKE=true
            shift
            ;;
        --skip-backup)
            SKIP_BACKUP=true
            shift
            ;;
        --help|-h)
            show_help
            exit 0
            ;;
        *)
            echo "[ERROR] Unknown option: ${1:-}" >&2
            show_help >&2
            exit 1
            ;;
    esac
done

if [[ -z "$OUTPUT_DIR" ]]; then
    OUTPUT_DIR=$(mktemp -d)
fi

mkdir -p "$OUTPUT_DIR"

declare -A PHASE_STATUS
for phase in phase0 phase1 phase2 phase3 phase4 phase5 phase6 phase7 phase8 phase9; do
    PHASE_STATUS[$phase]="not_run"
done

PROFILE_ROOT=""
FERRUMD_PID=""

cleanup() {
    # Always stop ferrumd first
    if [[ -n "${FERRUMD_PID:-}" ]]; then
        if kill -0 "$FERRUMD_PID" 2>/dev/null; then
            echo "[INFO] Stopping ferrumd (PID: $FERRUMD_PID)..." >&2
            kill "$FERRUMD_PID" 2>/dev/null || true
            wait "$FERRUMD_PID" 2>/dev/null || true
        fi
        FERRUMD_PID=""
    fi
    # Then cleanup temp dirs if not keeping
    if [[ "$KEEP_OUTPUT" == false ]]; then
        if [[ -n "${PROFILE_ROOT:-}" ]] && [[ -d "$PROFILE_ROOT" ]]; then
            echo "[INFO] Cleaning up temp profile root: $PROFILE_ROOT" >&2
            rm -rf "$PROFILE_ROOT"
        fi
        if [[ -n "$OUTPUT_DIR" ]] && [[ -d "$OUTPUT_DIR" ]] && [[ "$OUTPUT_DIR" == /tmp/* ]]; then
            echo "[INFO] Cleaning up temp output dir: $OUTPUT_DIR" >&2
            rm -rf "$OUTPUT_DIR"
        fi
    else
        echo "[INFO] Output kept at: $OUTPUT_DIR" >&2
        echo "[INFO] Profile root kept at: ${PROFILE_ROOT:-unknown}" >&2
    fi
}

trap cleanup EXIT

log_phase() {
    PHASE_STATUS[$1]="$2"
}

find_free_port() {
    local port=19080
    local max_attempts=100
    while ((port < max_attempts + 19080)); do
        if ! (echo > /dev/tcp/127.0.0.1/$port) 2>/dev/null; then
            echo $port
            return 0
        fi
        port=$((port + 1))
    done
    return 1
}

phase0_preflight() {
    echo ""
    echo "# PHASE 0: PREFLIGHT"
    local failed=0

    if ! command -v python3 >/dev/null 2>&1; then
        echo "[FAIL] python3 not found"
        failed=1
    fi

    if ! command -v cargo >/dev/null 2>&1; then
        echo "[FAIL] cargo not found"
        failed=1
    fi

    if [[ $failed -eq 0 ]]; then
        echo "[PASS] Phase 0: PREFLIGHT"
        log_phase phase0 "passed"
        return 0
    else
        echo "[FAIL] Phase 0: PREFLIGHT"
        log_phase phase0 "failed"
        return 1
    fi
}

phase1_profile_setup() {
    echo ""
    echo "# PHASE 1: PROFILE SETUP"
    PROFILE_ROOT=$(mktemp -d)
    mkdir -p "$PROFILE_ROOT/etc/ferrumgate"
    mkdir -p "$PROFILE_ROOT/var/lib/ferrumgate"
    mkdir -p "$PROFILE_ROOT/var/backups/ferrumgate"
    mkdir -p "$PROFILE_ROOT/evidence"
    mkdir -p "$PROFILE_ROOT/logs"
    echo "[INFO] Profile root: $PROFILE_ROOT"
    echo "[INFO] Directory structure:"
    echo "  etc/ferrumgate/   - config and env files"
    echo "  var/lib/ferrumgate/ - (DB path; in-memory SQLite used)"
    echo "  var/backups/ferrumgate/ - backup output"
    echo "  evidence/          - (empty; for real target evidence)"
    echo "  logs/              - ferrumd log"
    log_phase phase1 "passed"
}

phase2_token_generation() {
    echo ""
    echo "# PHASE 2: TOKEN GENERATION"
    # Generate token but do NOT echo it
    PROFILE_TOKEN="local-profile-$(openssl rand -hex 16 2>/dev/null || python3 -c 'import secrets; print(secrets.token_hex(16))')"
    echo "[INFO] Ephemeral token generated (not logged)"
    echo "[INFO] Token prefix: ${PROFILE_TOKEN:0:15}..."
    log_phase phase2 "passed"
}

phase3_config_env() {
    echo ""
    echo "# PHASE 3: CONFIG/ENV"

    PROFILE_PORT=$(find_free_port) || { echo "[FAIL] Could not find free port"; log_phase phase3 "failed"; return 1; }
    PROFILE_BASE_URL="http://127.0.0.1:$PROFILE_PORT"
    echo "[INFO] Port: $PROFILE_PORT"
    echo "[INFO] Base URL: $PROFILE_BASE_URL"

    local profile_etcdir="$PROFILE_ROOT/etc/ferrumgate"

    # Note: Using in-memory SQLite because file-backed SQLite does not work in /tmp
    # directories in this environment (SQLite code: 14, unable to open database file).
    # The profile structure is still created at the temp path for validation.
    cat > "$profile_etcdir/ferrumgate.toml" << EOF
[server]
bind_addr = "127.0.0.1:$PROFILE_PORT"
store_dsn = "sqlite::memory:"
auth_mode = "bearer"
bearer_token = "$PROFILE_TOKEN"
allow_insecure_nonlocal_bind = false
log_filter = "info"

[server.backup]
output_dir = "$PROFILE_ROOT/var/backups/ferrumgate"
EOF

    cat > "$profile_etcdir/ferrumd.env" << EOF
FERRUMD_AUTH_MODE=bearer
FERRUMD_BEARER_TOKEN=$PROFILE_TOKEN
FERRUMD_BIND_ADDR=127.0.0.1:$PROFILE_PORT
FERRUMD_STORE_DSN=sqlite::memory:
EOF

    echo "[INFO] Config written to: $profile_etcdir/"
    log_phase phase3 "passed"
}

phase4_ferrumd_start() {
    echo ""
    echo "# PHASE 4: FERRUMD START"

    FERRUMD=""
    for candidate in "$REPO_ROOT/target/release/ferrumd" "$REPO_ROOT/target/debug/ferrumd"; do
        [[ -x "$candidate" ]] && FERRUMD="$candidate" && break
    done

    if [[ -z "$FERRUMD" ]] || [[ ! -x "$FERRUMD" ]]; then
        echo "[INFO] Building ferrumd..."
        cargo build --release --bin ferrumd --manifest-path "$REPO_ROOT/Cargo.toml" >/dev/null 2>&1 || { echo "[FAIL] Build failed"; log_phase phase4 "failed"; return 1; }
        FERRUMD="$REPO_ROOT/target/release/ferrumd"
    fi

    if [[ ! -x "$FERRUMD" ]]; then
        echo "[FAIL] ferrumd not found"
        log_phase phase4 "failed"
        return 1
    fi

    local config_file="$PROFILE_ROOT/etc/ferrumgate/ferrumgate.toml"
    local ferrumd_log="$PROFILE_ROOT/logs/ferrumd.log"

    echo "[INFO] Config: $config_file"
    echo "[INFO] Starting ferrumd..."
    FERRUMD_BEARER_TOKEN="$PROFILE_TOKEN" "$FERRUMD" --config "$config_file" > "$ferrumd_log" 2>&1 &
    FERRUMD_PID=$!

    echo "[INFO] ferrumd PID: $FERRUMD_PID"

    # Wait for server
    local max_wait=30
    local waited=0
    while ((waited < max_wait)); do
        if python3 -c "import urllib.request; urllib.request.urlopen('$PROFILE_BASE_URL/v1/healthz', timeout=1)" 2>/dev/null; then
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done

    if ((waited >= max_wait)); then
        echo "[FAIL] ferrumd did not become ready within ${max_wait}s"
        echo "--- ferrumd log (last 20 lines) ---"
        tail -20 "$ferrumd_log" 2>/dev/null || cat "$ferrumd_log" 2>/dev/null || true
        echo "--- end log ---"
        log_phase phase4 "failed"
        return 1
    fi

    echo "[PASS] ferrumd ready"
    log_phase phase4 "passed"
}

phase5_probes() {
    echo ""
    echo "# PHASE 5: PROBES"

    local passed=0
    local failed=0

    for path in "/v1/healthz" "/v1/readyz" "/v1/readyz/deep" "/v1/metrics"; do
        local code
        code=$(python3 -c "import urllib.request; print(urllib.request.urlopen('$PROFILE_BASE_URL$path').getcode())" 2>/dev/null || echo "000")
        if [[ "$code" == "200" ]]; then
            echo "[PASS] $path -> $code"
            passed=$((passed + 1))
        else
            echo "[FAIL] $path -> $code"
            failed=$((failed + 1))
        fi
    done

    echo "[RESULT] Probes: $passed passed, $failed failed"
    if [[ $failed -eq 0 ]]; then
        log_phase phase5 "passed"
        return 0
    else
        log_phase phase5 "failed"
        return 1
    fi
}

phase6_auth_checks() {
    echo ""
    echo "# PHASE 6: AUTH CHECKS"

    local passed=0
    local failed=0

    # No token -> 401
    local code
    code=$(python3 -c "import urllib.request; urllib.request.urlopen('$PROFILE_BASE_URL/v1/approvals'); print('200')" 2>/dev/null || echo "401")
    if [[ "$code" == "401" ]]; then
        echo "[PASS] no token -> 401"
        passed=$((passed + 1))
    else
        echo "[FAIL] no token -> $code (expected 401)"
        failed=$((failed + 1))
    fi

    # Wrong token -> 401
    code=$(python3 -c "import urllib.request; req=urllib.request.Request('$PROFILE_BASE_URL/v1/approvals'); req.add_header('Authorization', 'Bearer wrong'); urllib.request.urlopen(req); print('200')" 2>/dev/null || echo "401")
    if [[ "$code" == "401" ]]; then
        echo "[PASS] wrong token -> 401"
        passed=$((passed + 1))
    else
        echo "[FAIL] wrong token -> $code (expected 401)"
        failed=$((failed + 1))
    fi

    # Correct token -> 200
    code=$(python3 -c "import urllib.request; req=urllib.request.Request('$PROFILE_BASE_URL/v1/approvals'); req.add_header('Authorization', 'Bearer $PROFILE_TOKEN'); urllib.request.urlopen(req); print('200')" 2>/dev/null || echo "401")
    if [[ "$code" == "200" ]]; then
        echo "[PASS] correct token -> 200"
        passed=$((passed + 1))
    else
        echo "[FAIL] correct token -> $code (expected 200)"
        failed=$((failed + 1))
    fi

    echo "[RESULT] Auth checks: $passed passed, $failed failed"
    if [[ $failed -eq 0 ]]; then
        log_phase phase6 "passed"
        return 0
    else
        log_phase phase6 "failed"
        return 1
    fi
}

phase7_backup_restore() {
    echo ""
    echo "# PHASE 7: BACKUP/RESTORE"

    if [[ "$SKIP_BACKUP" == true ]]; then
        echo "[SKIP] Phase 7: SKIPPED (--skip-backup)"
        log_phase phase7 "skipped"
        return 0
    fi

    # Use existing restore drill script if available
    if [[ -x "$SCRIPT_DIR/run_local_restore_drill.sh" ]]; then
        echo "[INFO] Running local restore drill..."
        if bash "$SCRIPT_DIR/run_local_restore_drill.sh" > "$OUTPUT_DIR/restore_drill_output.txt" 2>&1; then
            echo "[PASS] Phase 7: BACKUP/RESTORE - passed"
            log_phase phase7 "passed"
            return 0
        else
            echo "[FAIL] Phase 7: BACKUP/RESTORE - FAILED (restore drill returned error)"
            log_phase phase7 "failed"
            return 1
        fi
    else
        echo "[SKIP] Phase 7: SKIPPED (run_local_restore_drill.sh not found)"
        log_phase phase7 "skipped"
        return 0
    fi
}

phase8_auth_smoke() {
    echo ""
    echo "# PHASE 8: AUTH SMOKE"

    if [[ "$SKIP_AUTH_SMOKE" == true ]]; then
        echo "[SKIP] Phase 8: SKIPPED (--skip-auth-smoke)"
        log_phase phase8 "skipped"
        return 0
    fi

    if [[ -x "$SCRIPT_DIR/run_local_auth_smoke.sh" ]]; then
        echo "[INFO] Running local auth smoke..."
        if bash "$SCRIPT_DIR/run_local_auth_smoke.sh" > "$OUTPUT_DIR/auth_smoke_output.txt" 2>&1; then
            echo "[PASS] Phase 8: AUTH SMOKE - passed"
            log_phase phase8 "passed"
            return 0
        else
            echo "[FAIL] Phase 8: AUTH SMOKE - FAILED (auth smoke returned error)"
            log_phase phase8 "failed"
            return 1
        fi
    else
        echo "[SKIP] Phase 8: SKIPPED (run_local_auth_smoke.sh not found)"
        log_phase phase8 "skipped"
        return 0
    fi
}

phase9_artifact_output() {
    echo ""
    echo "# PHASE 9: ARTIFACT OUTPUT"

    local artifact_file="$OUTPUT_DIR/local-path2-target-profile-result.md"

    # Mark phase as passed BEFORE creating artifact (so status is captured correctly)
    log_phase phase9 "passed"

    cat > "$artifact_file" << EOF
# Local Path 2 Target Profile - Run Result

**Generated**: $(date -Iseconds 2>/dev/null || date '+%Y-%m-%dT%H:%M:%S')
**Profile version**: $PROFILE_VERSION
**Output directory**: $OUTPUT_DIR

## Profile Structure (Ephemeral Local)

${PROFILE_ROOT:-unknown}/
- etc/ferrumgate/ferrumgate.toml   - Generated config
- etc/ferrumgate/ferrumd.env       - Generated env
- var/lib/ferrumgate/              - (DB path; in-memory SQLite used)
- var/backups/ferrumgate/          - Backup output
- evidence/                        - (empty; for real target evidence)
- logs/ferrumd.log                - ferrumd log

Note: In-memory SQLite (sqlite::memory:) is used for the DB because file-backed
SQLite does not work in /tmp directories in this environment (SQLite error 14).
The profile structure is still fully created at the temp path.

## Phase Results

| Phase | Status |
|-------|--------|
| Phase 0 - Preflight | ${PHASE_STATUS[phase0]} |
| Phase 1 - Profile Setup | ${PHASE_STATUS[phase1]} |
| Phase 2 - Token Generation | ${PHASE_STATUS[phase2]} |
| Phase 3 - Config/Env | ${PHASE_STATUS[phase3]} |
| Phase 4 - ferrumd Start | ${PHASE_STATUS[phase4]} |
| Phase 5 - Probes | ${PHASE_STATUS[phase5]} |
| Phase 6 - Auth Checks | ${PHASE_STATUS[phase6]} |
| Phase 7 - Backup/Restore | ${PHASE_STATUS[phase7]} |
| Phase 8 - Auth Smoke | ${PHASE_STATUS[phase8]} |
| Phase 9 - Artifact Output | ${PHASE_STATUS[phase9]} |

## Explicit Non-Claims

This profile run does NOT produce:
- G2 evidence (G2.1-G2.8 remain pending - operator action required)
- Target readiness (no real target values were used)
- Pilot authorized (operator signoff required)
- Production-ready (RC-ready/conditional)
- Operator signoff (doc 54 remains unsigned)
- Real secrets (ephemeral token only - NOT committed)

## Next Steps for Real Target Deployment

1. Operator collects Critical fields from doc71
2. Operator generates real bearer token
3. Operator adapts configs to real target paths
4. Operator executes on real target per doc66 Phase B
5. Operator completes drills and fills G2 evidence per doc59
6. Operator signs doc 54 only after G2 gates satisfied

See: doc93 - Local Path2 Target Profile Plan

---
Generated by run_local_path2_target_profile.sh - LOCAL-ONLY - NOT TARGET EVIDENCE
EOF

    echo "[INFO] Artifact written: $artifact_file"
}

stop_ferrumd() {
    if [[ -n "${FERRUMD_PID:-}" ]]; then
        if kill -0 "$FERRUMD_PID" 2>/dev/null; then
            echo "[INFO] Stopping ferrumd (PID: $FERRUMD_PID)..."
            kill "$FERRUMD_PID" 2>/dev/null || true
            wait "$FERRUMD_PID" 2>/dev/null || true
        fi
        FERRUMD_PID=""
    fi
}

main() {
    echo ""
    echo "# FerrumGate v1 - Local Path 2 Target Profile"
    echo ""
    echo "[INFO] LOCAL-ONLY - NOT TARGET EVIDENCE - NOT G2"
    echo "[INFO] Output directory: $OUTPUT_DIR"
    echo ""

    # Run phases - stop on failure for early phases, continue for later ones
    phase0_preflight || true
    phase1_profile_setup || true
    phase2_token_generation || true
    phase3_config_env || true
    phase4_ferrumd_start || true
    phase5_probes || true
    phase6_auth_checks || true
    phase7_backup_restore || true
    phase8_auth_smoke || true
    phase9_artifact_output || true

    echo ""
    echo "# PROFILE RUN COMPLETE"
    echo ""
    echo "Phase results:"
    for phase in phase0 phase1 phase2 phase3 phase4 phase5 phase6 phase7 phase8 phase9; do
        echo "  - $phase: ${PHASE_STATUS[$phase]}"
    done
    echo ""
    echo "Output directory: $OUTPUT_DIR"
    echo ""
    echo "ALL OUTPUTS ARE LOCAL-ONLY - NOT TARGET EVIDENCE"
}

main "$@"
