#!/usr/bin/env bash
set -euo pipefail

# FerrumGate Phase 9 HA detection-only watchdog.
#
# This script intentionally NEVER promotes PostgreSQL and NEVER rewrites
# PgBouncer routing. It only detects an unreachable remote primary, logs an
# operator-required alert, and exits non-zero so systemd/journald can record it.

CONFIG_FILE="${1:-/etc/ferrumgate/ha-watchdog.env}"

if [[ -f "${CONFIG_FILE}" ]]; then
  # shellcheck disable=SC1090
  source "${CONFIG_FILE}"
fi

LOCAL_PORT="${LOCAL_PORT:?LOCAL_PORT required}"
REMOTE_HOST="${REMOTE_HOST:?REMOTE_HOST required}"
REMOTE_PORT="${REMOTE_PORT:?REMOTE_PORT required}"
DB_NAME="${DB_NAME:-postgres}"
LOG_FILE="${LOG_FILE:-/var/log/ferrumgate/ha-watchdog.log}"

mkdir -p "$(dirname "${LOG_FILE}")"

ts() {
  date -u +%Y-%m-%dT%H:%M:%SZ
}

log() {
  printf "%s %s\n" "$(ts)" "$*" | tee -a "${LOG_FILE}"
}

local_state=$(sudo -u postgres psql -p "${LOCAL_PORT}" -Atc "select pg_is_in_recovery();" 2>&1 || true)

if [[ "${local_state}" != "t" ]]; then
  log "OK local_port=${LOCAL_PORT} local_role=primary_or_unavailable state=${local_state} action=none reason=watchdog_is_detection_only"
  exit 0
fi

if pg_isready -h "${REMOTE_HOST}" -p "${REMOTE_PORT}" -d "${DB_NAME}" >/tmp/ha-watchdog-pg-isready.out 2>&1; then
  lag=$(sudo -u postgres psql -p "${LOCAL_PORT}" -Atc "select coalesce(extract(epoch from now() - pg_last_xact_replay_timestamp())::int,0);" 2>/dev/null || echo unknown)
  log "OK local_port=${LOCAL_PORT} local_role=standby remote=${REMOTE_HOST}:${REMOTE_PORT} remote_state=reachable replay_lag_seconds=${lag} action=none reason=watchdog_is_detection_only"
  exit 0
fi

ready_msg=$(tr "\n" " " </tmp/ha-watchdog-pg-isready.out | sed "s/[[:space:]]\+/ /g")
lag=$(sudo -u postgres psql -p "${LOCAL_PORT}" -Atc "select coalesce(extract(epoch from now() - pg_last_xact_replay_timestamp())::int,0);" 2>/dev/null || echo unknown)

log "ALERT local_port=${LOCAL_PORT} local_role=standby remote=${REMOTE_HOST}:${REMOTE_PORT} remote_state=unreachable replay_lag_seconds=${lag} action=operator_required next_step=confirm_fencing_then_manual_promote reason=watchdog_never_auto_promotes pg_isready='${ready_msg}'"
exit 2
