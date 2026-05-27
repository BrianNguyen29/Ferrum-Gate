#!/usr/bin/env bash
set -euo pipefail

# GCP Phase 9 fencing utility.
#
# Safety boundary: this script only fences (stops) a GCP Compute instance. It
# never promotes PostgreSQL and never rewrites PgBouncer routing.

DEFAULT_PROJECT="fairy-b13f4"
DEFAULT_ZONE="asia-southeast1-a"
APP_HOST="ferrumgate-nonprod"

PROJECT="${DEFAULT_PROJECT}"
ZONE="${DEFAULT_ZONE}"
TARGET=""
CONFIRM=""
DRY_RUN=true
FORCE_APP_HOST=false
LOG_FILE=""

usage() {
  cat <<EOF
Usage: $(basename "$0") --target INSTANCE_NAME [options]

Options:
  --target INSTANCE_NAME    Target instance to fence (required)
  --dry-run                 Show what would be done without acting (default)
  --fence                   Actually stop the instance
  --confirm INSTANCE_NAME   Required with --fence; must match --target
  --force-app-host          Allow fencing ${APP_HOST} (app/PgBouncer host)
  --project PROJECT         GCP project (default: ${DEFAULT_PROJECT})
  --zone ZONE               GCP zone (default: ${DEFAULT_ZONE})
  --log-file FILE           Log file path (default: stdout only)

Safety: this script only stops the GCP instance. It does NOT promote
PostgreSQL and does NOT rewrite PgBouncer.
EOF
}

log() {
  local msg="[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*"
  if [[ -n "${LOG_FILE}" ]]; then
    echo "${msg}" >>"${LOG_FILE}" 2>/dev/null || echo "${msg}"
  else
    echo "${msg}"
  fi
}

fatal() {
  log "FATAL: $*"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --confirm)
      CONFIRM="${2:-}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --fence)
      DRY_RUN=false
      shift
      ;;
    --force-app-host)
      FORCE_APP_HOST=true
      shift
      ;;
    --project)
      PROJECT="${2:-}"
      shift 2
      ;;
    --zone)
      ZONE="${2:-}"
      shift 2
      ;;
    --log-file)
      LOG_FILE="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fatal "Unknown option: $1"
      ;;
  esac
done

if [[ -z "${TARGET}" ]]; then
  usage
  fatal "--target is required"
fi

if [[ -n "${LOG_FILE}" ]]; then
  touch "${LOG_FILE}" 2>/dev/null || {
    echo "Warning: cannot write to log file '${LOG_FILE}', using stdout only" >&2
    LOG_FILE=""
  }
fi

log "=== GCP Phase 9 fencing utility ==="
log "target=${TARGET} project=${PROJECT} zone=${ZONE} dry_run=${DRY_RUN} force_app_host=${FORCE_APP_HOST}"
log "safety=no_postgres_promote no_pgbouncer_rewrite"

if ! command -v gcloud >/dev/null 2>&1; then
  fatal "gcloud CLI not found in PATH"
fi

if ! STATUS=$(gcloud compute instances describe "${TARGET}" \
  --project="${PROJECT}" \
  --zone="${ZONE}" \
  --format='value(status)' 2>&1); then
  fatal "instance '${TARGET}' not found or gcloud failed: ${STATUS}"
fi

STATUS=$(echo "${STATUS}" | tr -d '[:space:]')
log "instance_status=${STATUS}"

if [[ "${DRY_RUN}" == true ]]; then
  log "dry_run=would_stop_instance target=${TARGET} if --fence --confirm ${TARGET} were provided"
  if [[ "${STATUS}" != "RUNNING" ]]; then
    log "dry_run_note=target_not_running status=${STATUS}"
  fi
  log "no_action_taken no_postgres_promote no_pgbouncer_rewrite"
  exit 0
fi

if [[ "${TARGET}" == "${APP_HOST}" && "${FORCE_APP_HOST}" != true ]]; then
  fatal "refusing to fence app/PgBouncer host '${APP_HOST}' without --force-app-host"
fi

if [[ -z "${CONFIRM}" ]]; then
  fatal "--confirm is required when using --fence"
fi

if [[ "${CONFIRM}" != "${TARGET}" ]]; then
  fatal "--confirm ('${CONFIRM}') must exactly match --target ('${TARGET}')"
fi

if [[ "${STATUS}" != "RUNNING" ]]; then
  fatal "instance '${TARGET}' is not RUNNING (status=${STATUS}); refusing to fence"
fi

log "fence_start target=${TARGET} action=gcloud_compute_instances_stop"
gcloud compute instances stop "${TARGET}" \
  --project="${PROJECT}" \
  --zone="${ZONE}" \
  --quiet

log "poll_start target=${TARGET} desired_status=TERMINATED timeout_seconds=180"
poll_interval=5
max_wait=180
elapsed=0

while true; do
  if ! current_status=$(gcloud compute instances describe "${TARGET}" \
    --project="${PROJECT}" \
    --zone="${ZONE}" \
    --format='value(status)' 2>&1); then
    fatal "failed to query status during poll: ${current_status}"
  fi
  current_status=$(echo "${current_status}" | tr -d '[:space:]')
  log "poll_status=${current_status} elapsed_seconds=${elapsed}"

  if [[ "${current_status}" == "TERMINATED" ]]; then
    log "fence_complete target=${TARGET} status=TERMINATED"
    log "reminder=no_postgres_promote no_pgbouncer_rewrite"
    exit 0
  fi

  if [[ "${elapsed}" -ge "${max_wait}" ]]; then
    fatal "timeout waiting for '${TARGET}' to reach TERMINATED; current_status=${current_status}"
  fi

  sleep "${poll_interval}"
  elapsed=$((elapsed + poll_interval))
done
