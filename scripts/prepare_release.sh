#!/usr/bin/env bash
# scripts/prepare_release.sh
# Conservative release-preflight script for FerrumGate.
# Does NOT create tags, push, publish crates, or require secrets.
# Usage: ./scripts/prepare_release.sh [--dry-run] [--execute] [--version VERSION]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=true
TARGET_VERSION=""

# --- argument parsing ---
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --execute)
      DRY_RUN=false
      shift
      ;;
    --version)
      TARGET_VERSION="${2:-}"
      shift 2
      ;;
    *)
      echo "Usage: $0 [--dry-run] [--execute] [--version VERSION]" >&2
      exit 1
      ;;
  esac
done

# --- helpers ---
log_info() { printf '[INFO] %s\n' "$*"; }
log_ok()   { printf '[OK]   %s\n' "$*"; }
log_warn() { printf '[WARN] %s\n' "$*"; }
log_err()  { printf '[ERR]  %s\n' "$*"; }

failures=0

run_check() {
  local name="$1"
  shift
  log_info "Running: $name"
  if "$@"; then
    log_ok "$name"
  else
    log_err "$name"
    failures=$((failures + 1))
  fi
}

# --- determine version ---
if [[ -z "$TARGET_VERSION" ]]; then
  TARGET_VERSION=$(grep -E '^version\s*=' "$REPO_ROOT/Cargo.toml" | head -n1 | sed 's/.*"\(.*\)".*/\1/')
fi

log_info "Target version: $TARGET_VERSION"
log_info "Dry-run mode: $DRY_RUN"
log_info "Repository root: $REPO_ROOT"

# --- changelog validation ---
validate_changelog() {
  if [[ ! -f "$REPO_ROOT/CHANGELOG.md" ]]; then
    log_err "CHANGELOG.md not found"
    return 1
  fi

  if ! grep -qE "^## v?$TARGET_VERSION" "$REPO_ROOT/CHANGELOG.md"; then
    log_err "CHANGELOG.md missing section for version $TARGET_VERSION"
    return 1
  fi

  log_ok "CHANGELOG.md contains section for $TARGET_VERSION"
}

# --- version consistency ---
validate_version_consistency() {
  local workspace_version
  workspace_version=$(grep -E '^version\s*=' "$REPO_ROOT/Cargo.toml" | head -n1 | sed 's/.*"\(.*\)".*/\1/')
  if [[ "$workspace_version" != "$TARGET_VERSION" ]]; then
    log_err "Cargo.toml workspace version ($workspace_version) != target ($TARGET_VERSION)"
    return 1
  fi
  log_ok "Cargo.toml workspace version matches $TARGET_VERSION"
}

# --- cargo checks ---
check_cargo() {
  cargo check --workspace
}

check_fmt() {
  cargo fmt --all -- --check
}

check_lint() {
  cargo clippy --workspace --all-targets -- -D warnings
}

check_test() {
  cargo test --workspace
}

check_all_features() {
  cargo check --workspace --all-features
}

check_postgres_features() {
  cargo check -p ferrumd -p ferrum-migrate -p ferrum-store -p ferrum-gateway --features postgres
}

check_s3_features() {
  cargo check -p ferrumd --features s3
}

# --- make targets ---
check_docs() {
  make docs
}

check_validate() {
  make validate
}

check_audit() {
  make audit
}

check_pretarget() {
  make pretarget
}

# --- SBOM generation (optional) ---
generate_sbom() {
  if command -v cargo-cyclonedx >/dev/null 2>&1 || cargo install --list | grep -q cargo-cyclonedx; then
    log_info "Generating SBOM with cargo-cyclonedx..."
    cargo cyclonedx --all
    log_ok "SBOM generated in target/cyclonedx/"
  else
    log_warn "cargo-cyclonedx not found; skipping SBOM generation"
    log_warn "Install with: cargo install cargo-cyclonedx"
  fi
}

# --- release profile smoke ---
check_release_smoke() {
  bash "$REPO_ROOT/scripts/validate_release_feature_profile.sh"
}

# --- roadmap status ---
validate_roadmap() {
  if [[ ! -f "$REPO_ROOT/docs/ROADMAP.md" ]]; then
    log_warn "docs/ROADMAP.md not found; skipping"
    return 0
  fi
  log_ok "docs/ROADMAP.md present"
}

# --- main ---
main() {
  cd "$REPO_ROOT"

  run_check "changelog validation" validate_changelog
  run_check "version consistency" validate_version_consistency
  run_check "cargo check" check_cargo
  run_check "cargo fmt" check_fmt
  run_check "cargo clippy" check_lint
  run_check "cargo test" check_test
  run_check "cargo check --all-features" check_all_features
  run_check "cargo check (postgres features)" check_postgres_features
  run_check "cargo check (s3 features)" check_s3_features
  run_check "make docs" check_docs
  run_check "make validate" check_validate
  run_check "make audit" check_audit
  run_check "make pretarget" check_pretarget
  run_check "release profile smoke" check_release_smoke
  run_check "roadmap presence" validate_roadmap

  if [[ "$DRY_RUN" == true ]]; then
    log_info "Dry-run: skipping SBOM generation"
  else
    generate_sbom || log_warn "SBOM generation failed (non-fatal)"
  fi

  echo ""
  echo "========================================"
  if [[ "$failures" -eq 0 ]]; then
    log_ok "All preflight checks passed"
  else
    log_err "$failures preflight check(s) failed"
  fi
  echo "========================================"
  echo ""
  echo "Next steps (manual):"
  echo "  1. Review CHANGELOG.md and ensure the release notes are complete."
  echo "  2. If SBOM was generated, attach target/cyclonedx/*.json to the GitHub Release."
  echo "  3. Commit changes: git commit -am 'chore(release): prepare v$TARGET_VERSION'"
  echo "  4. Tag locally: git tag -a v$TARGET_VERSION -m 'Release v$TARGET_VERSION'"
  echo "  5. Push tag: git push origin v$TARGET_VERSION"
  echo "  6. Create a GitHub Release from the tag and paste the CHANGELOG section."
  echo "  7. Attach SBOM artifact (CycloneDX JSON) to the GitHub Release."
  echo ""
  echo "To run with SBOM generation (still no push): ./scripts/prepare_release.sh --execute"
  echo ""

  if [[ "$failures" -gt 0 ]]; then
    exit 1
  fi
}

main
