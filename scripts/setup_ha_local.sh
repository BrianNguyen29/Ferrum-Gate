#!/usr/bin/env bash
# setup_ha_local.sh
# Local HA PostgreSQL simulation setup.
#
# Starts a primary container, creates a replication user, runs pg_basebackup
# into a standby volume, and starts the standby.
#
# Boundary: local-only, manual/optional, no production HA claim.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.ha-local.yml"
PROJECT_NAME="ferrumgate_ha_local"
PRIMARY_CONTAINER="ferrumgate_postgres_ha_primary"
STANDBY_CONTAINER="ferrumgate_postgres_ha_standby"
PRIMARY_SERVICE="postgres_ha_primary"
STANDBY_SERVICE="postgres_ha_standby"

PASS=0
FAIL=0
SKIP=0

pass() { echo "[PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "[FAIL] $1"; FAIL=$((FAIL + 1)); }
skip() { echo "[SKIP] $1"; SKIP=$((SKIP + 1)); }

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/setup_ha_local.sh

Sets up a local Docker-based primary/standby PostgreSQL simulation:
  1. Starts primary container with replication config.
  2. Creates replication user and HBA rule via init script.
  3. Runs pg_basebackup to populate standby volume.
  4. Starts standby container in streaming recovery mode.

Boundary: local-only; manual/optional; not executed in CI.
EOF
    exit 0
fi

echo ""
echo "========================================"
echo "HA Local Setup — Preflight"
echo "========================================"
echo ""

if command -v docker >/dev/null 2>&1; then
    pass "docker is available"
else
    fail "docker is not available"
    exit 1
fi

if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD=(docker compose -p "$PROJECT_NAME")
    pass "docker compose plugin is available"
elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD=(docker-compose -p "$PROJECT_NAME")
    pass "docker-compose is available"
else
    fail "docker compose is not available"
    exit 1
fi

echo ""
echo "========================================"
echo "Cleaning up any previous HA simulation"
echo "========================================"
echo ""

"${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" down --volumes 2>/dev/null || true
pass "previous containers and volumes removed"

echo ""
echo "========================================"
echo "Starting primary PostgreSQL"
echo "========================================"
echo ""

"${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" up -d "$PRIMARY_SERVICE"

echo "[INFO] Waiting for primary healthcheck..."
HEALTHY=false
for _ in {1..30}; do
    STATUS=$(docker inspect --format='{{.State.Health.Status}}' "$PRIMARY_CONTAINER" 2>/dev/null || echo "unknown")
    if [[ "$STATUS" == "healthy" ]]; then
        HEALTHY=true
        break
    fi
    sleep 1
done

if [[ "$HEALTHY" == "true" ]]; then
    pass "primary container is healthy"
else
    fail "primary container did not become healthy within 30s"
    exit 1
fi

echo ""
echo "========================================"
echo "Verifying replication user"
echo "========================================"
echo ""

REPLICATOR_EXISTS=$(docker exec -e FERRUM_SQL="SELECT 1 FROM pg_roles WHERE rolname='replicator';" "$PRIMARY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -At -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"')
if [[ "$REPLICATOR_EXISTS" == "1" ]]; then
    pass "replication user exists on primary"
else
    fail "replication user missing on primary"
    exit 1
fi

echo ""
echo "========================================"
echo "Running pg_basebackup for standby"
echo "========================================"
echo ""

STANDBY_VOLUME="${PROJECT_NAME}_ha_standby_data"

# Ensure standby volume exists and is empty
docker volume rm -f "$STANDBY_VOLUME" 2>/dev/null || true
docker volume create "$STANDBY_VOLUME" >/dev/null

docker run --rm \
    --network "${PROJECT_NAME}_default" \
    -v "${STANDBY_VOLUME}:/data" \
    -e PGPASSWORD=replicator_pass \
    postgres:16 \
    pg_basebackup -h postgres_ha_primary -p 5432 -U replicator -D /data -Fp -Xs -P -R

pass "pg_basebackup completed into standby volume"

echo ""
echo "========================================"
echo "Starting standby PostgreSQL"
echo "========================================"
echo ""

"${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" up -d "$STANDBY_SERVICE"

echo "[INFO] Waiting for standby healthcheck..."
HEALTHY=false
for _ in {1..30}; do
    STATUS=$(docker inspect --format='{{.State.Health.Status}}' "$STANDBY_CONTAINER" 2>/dev/null || echo "unknown")
    if [[ "$STATUS" == "healthy" ]]; then
        HEALTHY=true
        break
    fi
    sleep 1
done

if [[ "$HEALTHY" == "true" ]]; then
    pass "standby container is healthy"
else
    fail "standby container did not become healthy within 30s"
    exit 1
fi

echo ""
echo "========================================"
echo "Verifying streaming replication"
echo "========================================"
echo ""

# Quick replication sanity check: create table on primary and wait for standby
docker exec -e FERRUM_SQL="CREATE TABLE IF NOT EXISTS _ha_setup_verify (id int PRIMARY KEY); INSERT INTO _ha_setup_verify VALUES (42) ON CONFLICT DO NOTHING;" "$PRIMARY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'

REPLICATED=false
for _ in {1..15}; do
    STANDBY_VAL=$(docker exec -e FERRUM_SQL="SELECT id FROM _ha_setup_verify WHERE id=42;" "$STANDBY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -At -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"' || true)
    if [[ "$STANDBY_VAL" == "42" ]]; then
        REPLICATED=true
        break
    fi
    sleep 1
done

if [[ "$REPLICATED" == "true" ]]; then
    pass "streaming replication verified (row replicated from primary to standby)"
else
    fail "streaming replication not confirmed within 15s"
    exit 1
fi

echo ""
echo "========================================"
echo "HA LOCAL SETUP SUMMARY"
echo "========================================"
echo "Passed:  $PASS"
echo "Failed:  $FAIL"
echo "Skipped: $SKIP"
echo ""
echo "Primary: localhost:5433 (container: $PRIMARY_CONTAINER)"
echo "Standby: localhost:5434 (container: $STANDBY_CONTAINER)"
echo ""
echo "Boundary: local-only, Docker Compose, manual/optional."
echo "No production HA claim. No automated failover."
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo "HA LOCAL SETUP: ALL CHECKS PASSED"
    exit 0
else
    echo "HA LOCAL SETUP: SOME CHECKS FAILED"
    exit 1
fi
