#!/usr/bin/env bash
set -euo pipefail

if [ "${FERRUMGATE_RELEASE_FEATURES:-}" = "unsafe-unbounded-adapters" ] ||
   [[ ",${FERRUMGATE_RELEASE_FEATURES:-}," == *",unsafe-unbounded-adapters,"* ]]; then
  echo "release builds must not enable unsafe-unbounded-adapters" >&2
  exit 1
fi

cargo build --release -p ferrumd --no-default-features

smoke_dir="$(mktemp -d)"
log_file="$smoke_dir/ferrumd-release-smoke.log"
cleanup() {
  rm -rf "$smoke_dir"
}
trap cleanup EXIT

set +e
FERRUMD_BEARER_TOKEN="release-smoke-token-not-for-production" \
FERRUMD_BIND_ADDR="127.0.0.1:0" \
FERRUMD_STORE_DSN="sqlite://$smoke_dir/ferrumgate.db?mode=rwc" \
FERRUMD_FS_WORKDIR="$smoke_dir/workdir" \
FERRUMD_GIT_REPO_ROOTS="" \
FERRUMD_SQLITE_DB_ROOTS="" \
timeout 8s target/release/ferrumd --config configs/ferrumgate.prod.toml >"$log_file" 2>&1
status=$?
set -e

if [ "$status" -eq 124 ]; then
  echo "release smoke passed: ferrumd started with production config overrides"
  exit 0
fi

echo "release smoke failed with exit status $status" >&2
cat "$log_file" >&2
exit "$status"
