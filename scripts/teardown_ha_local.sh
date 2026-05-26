#!/usr/bin/env bash
# teardown_ha_local.sh
# Tear down the local HA PostgreSQL simulation.
#
# Boundary: local-only, manual/optional.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.ha-local.yml"
PROJECT_NAME="ferrumgate_ha_local"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/teardown_ha_local.sh

Stops and removes the HA local PostgreSQL containers and volumes.
EOF
    exit 0
fi

if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD=(docker compose -p "$PROJECT_NAME")
elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD=(docker-compose -p "$PROJECT_NAME")
else
    echo "[WARN] docker compose not found; attempting manual cleanup"
    docker rm -f ferrumgate_postgres_ha_primary ferrumgate_postgres_ha_standby 2>/dev/null || true
    docker volume rm -f "${PROJECT_NAME}_ha_primary_data" "${PROJECT_NAME}_ha_standby_data" 2>/dev/null || true
    exit 0
fi

echo "[INFO] Tearing down HA local simulation..."
"${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" down --volumes 2>/dev/null || true
docker volume rm -f "${PROJECT_NAME}_ha_primary_data" "${PROJECT_NAME}_ha_standby_data" 2>/dev/null || true
echo "[PASS] HA local simulation torn down"
