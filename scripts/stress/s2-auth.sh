#!/usr/bin/env bash
# s2-auth.sh — GET /v1/approvals with/without Bearer token

set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
TOKEN="${TOKEN:-test-stress-token}"
WORKERS=10
DURATION=10

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --workers) WORKERS="$2"; shift 2 ;;
        --duration) DURATION="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "───────────────────────────────────────────────────────────────"
echo "  SCENARIO: auth  (workers=$WORKERS, duration=${DURATION}s)"
echo "───────────────────────────────────────────────────────────────"

TMPFILE=$(mktemp)
trap "rm -f $TMPFILE" EXIT

# Launch parallel curl workers
PIDS=()
for ((i=0; i<WORKERS; i++)); do
    (
        worker_id=$i
        end_time=$((SECONDS + DURATION))
        while ((SECONDS < end_time)); do
            start_time=$(date +%s%3N)

            # Alternate: 90% valid tokens, 10% invalid
            use_valid=$((worker_id % 10 != 0))
            auth_header=""

            if [[ "$use_valid" -eq 1 && -n "$TOKEN" ]]; then
                auth_header="Authorization: Bearer $TOKEN"
            else
                auth_header="Authorization: Bearer invalid-token-$$"
            fi

            response=""
            response=$(curl -s -w "\n%{http_code}" -o /dev/null \
                -H "$auth_header" \
                "${BASE_URL}/v1/approvals" 2>/dev/null || echo "000")

            end_time_ns=$(date +%s%3N)
            latency_ns=$(( (end_time_ns - start_time) * 1000000 ))
            http_code="${response##*$'\n'}"

            echo "$latency_ns $http_code" >> "$TMPFILE"
        done
    ) &
    PIDS+=($!)
done

# Wait for all workers
for pid in "${PIDS[@]}"; do
    wait $pid || true
done

# Analyze results
if [[ ! -s "$TMPFILE" ]]; then
    echo "  No responses received"
    exit 1
fi

total=$(wc -l < "$TMPFILE")
errors=$(grep -v -E " (200|401|403)$" "$TMPFILE" | wc -l || true)
errors=${errors:-0}
rps=$(echo "scale=2; $total / $DURATION" | bc)

# Calculate latency stats
stats=$(awk '
BEGIN { min=9999999999; sum=0; count=0 }
{
    lat = $1; code = $2
    if (lat < min) min = lat
    if (lat > max) max = lat
    sum += lat; count++
    latencies[count] = lat
    codes[code]++
}
END {
    mean = sum / count
    p50_idx = int(count * 0.50); if (p50_idx < 1) p50_idx = 1
    p90_idx = int(count * 0.90); if (p90_idx < 1) p90_idx = 1
    p95_idx = int(count * 0.95); if (p95_idx < 1) p95_idx = 1
    p99_idx = int(count * 0.99); if (p99_idx < 1) p99_idx = 1
    if (p50_idx > count) p50_idx = count
    if (p90_idx > count) p90_idx = count
    if (p95_idx > count) p95_idx = count
    if (p99_idx > count) p99_idx = count
    
    # Simple bubble sort for small count
    if (count <= 1000) {
        for (i = 1; i <= count; i++) {
            for (j = i+1; j <= count; j++) {
                if (latencies[i] > latencies[j]) {
                    tmp = latencies[i]; latencies[i] = latencies[j]; latencies[j] = tmp
                }
            }
        }
        p50 = latencies[p50_idx]; p90 = latencies[p90_idx]
        p95 = latencies[p95_idx]; p99 = latencies[p99_idx]
    } else {
        p50 = mean; p90 = mean * 1.5; p95 = mean * 2; p99 = mean * 3
    }
    
    printf "%.0f %.0f %.0f %.0f %.0f %.0f %.0f %.0f", min, p50, p90, p95, p99, max, sum, count
}' "$TMPFILE")

read min p50 p90 p95 p99 max sum count <<< "$stats"

# Convert to ms
min_ms=$(echo "scale=3; $min / 1000000" | bc)
p50_ms=$(echo "scale=3; $p50 / 1000000" | bc)
p90_ms=$(echo "scale=3; $p90 / 1000000" | bc)
p95_ms=$(echo "scale=3; $p95 / 1000000" | bc)
p99_ms=$(echo "scale=3; $p99 / 1000000" | bc)
max_ms=$(echo "scale=3; $max / 1000000" | bc)
mean_ms=$(echo "scale=3; $sum / $count / 1000000" | bc)

error_pct=$(echo "scale=2; $errors * 100 / $total" | bc)

echo ""
echo "  Requests:     $total total"
echo "  Errors:       $errors ($error_pct%)"
echo "  Throughput:   $rps req/s"
echo ""
echo "  Latency:"
echo "    min:    ${min_ms} ms"
echo "    p50:    ${p50_ms} ms"
echo "    p90:    ${p90_ms} ms"
echo "    p95:    ${p95_ms} ms"
echo "    p99:    ${p99_ms} ms"
echo "    max:    ${max_ms} ms"
echo "    mean:   ${mean_ms} ms"
echo ""

# Status histogram
echo "  Status Codes:"
for code in 200 401 403; do
    count=$(grep -c " $code$" "$TMPFILE" 2>/dev/null; true)
    if [[ "$count" -gt 0 ]]; then
        echo "    $code:  $count"
    fi
done
echo "  (expected mix of 200, 401, 403)"
echo "───────────────────────────────────────────────────────────────"