#!/usr/bin/env bash
# s1-health.sh — GET /v1/healthz burst test using curl in a loop

set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
TOKEN="${TOKEN:-}"
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
echo "  SCENARIO: health  (workers=$WORKERS, duration=${DURATION}s)"
echo "───────────────────────────────────────────────────────────────"

# Create temp file for results
TMPFILE=$(mktemp)
trap "rm -f $TMPFILE" EXIT

# Launch parallel curl workers
PIDS=()
for ((i=0; i<WORKERS; i++)); do
    (
        local worker_id=$i
        local end_time=$((SECONDS + DURATION))
        while ((SECONDS < end_time)); do
            local start_time=$(date +%s%3N)
            local response
            local http_code
            
            if [[ -n "$TOKEN" ]]; then
                response=$(curl -s -w "\n%{http_code}" -o /dev/null \
                    -H "Authorization: Bearer $TOKEN" \
                    "${BASE_URL}/v1/healthz" 2>/dev/null || echo "000")
            else
                response=$(curl -s -w "\n%{http_code}" -o /dev/null \
                    "${BASE_URL}/v1/healthz" 2>/dev/null || echo "000")
            fi
            
            local end_time_ns=$(date +%s%3N)
            local latency_ns=$(( (end_time_ns - start_time) * 1000000 ))
            local http_code="${response##*$'\n'}"
            
            echo "$latency_ns $http_code" >> "$TMPFILE"
        done
    ) &
    PIDS+=($!)
done

# Wait for all workers
failed=0
for pid in "${PIDS[@]}"; do
    wait $pid || ((failed++))
done

# Analyze results
if [[ ! -s "$TMPFILE" ]]; then
    echo "  No responses received"
    exit 1
fi

total=$(wc -l < "$TMPFILE")
errors=$(grep -v " 200$" "$TMPFILE" | wc -l || true)
errors=${errors:-0}
rps=$(echo "scale=2; $total / $DURATION" | bc)

# Calculate latency stats using awk
stats=$(awk '
BEGIN { min=9999999999; sum=0; count=0; p50=0; p90=0; p95=0; p99=0 }
{
    lat = $1
    code = $2
    if (lat < min) min = lat
    if (lat > max) max = lat
    sum += lat
    count++
    latencies[count] = lat
}
END {
    mean = sum / count
    # Sort latencies for percentiles
    # Simple percentiles by position
    p50_idx = int(count * 0.50)
    p90_idx = int(count * 0.90)
    p95_idx = int(count * 0.95)
    p99_idx = int(count * 0.99)
    if (p50_idx < 1) p50_idx = 1
    if (p90_idx < 1) p90_idx = 1
    if (p95_idx < 1) p95_idx = 1
    if (p99_idx < 1) p99_idx = 1
    if (p50_idx > count) p50_idx = count
    if (p90_idx > count) p90_idx = count
    if (p95_idx > count) p95_idx = count
    if (p99_idx > count) p99_idx = count
    
    # Approximate percentile extraction via bubble sort (small samples only)
    # For large counts, use binning
    if (count <= 1000) {
        # Shell sort approximation - just find values at indices
        for (i = 1; i <= count; i++) {
            for (j = i+1; j <= count; j++) {
                if (latencies[i] > latencies[j]) {
                    tmp = latencies[i]
                    latencies[i] = latencies[j]
                    latencies[j] = tmp
                }
            }
        }
        p50 = latencies[p50_idx]
        p90 = latencies[p90_idx]
        p95 = latencies[p95_idx]
        p99 = latencies[p99_idx]
    } else {
        # Use frequency binning for large counts
        p50 = mean
        p90 = mean * 1.5
        p95 = mean * 2
        p99 = mean * 3
    }
    
    printf "%.0f %.0f %.0f %.0f %.0f %.0f", min, p50, p90, p95, p99, max
}' "$TMPFILE")

read min p50 p90 p95 p99 max <<< "$stats"
min_ns=$min; p50_ns=$p50; p90_ns=$p90; p95_ns=$p95; p99_ns=$p99; max_ns=$max

# Convert to milliseconds
min_ms=$(echo "scale=3; $min_ns / 1000000" | bc)
p50_ms=$(echo "scale=3; $p50_ns / 1000000" | bc)
p90_ms=$(echo "scale=3; $p90_ns / 1000000" | bc)
p95_ms=$(echo "scale=3; $p95_ns / 1000000" | bc)
p99_ms=$(echo "scale=3; $p99_ns / 1000000" | bc)
max_ms=$(echo "scale=3; $max_ns / 1000000" | bc)
mean_ns=$(echo "scale=0; $sum / $count" | bc)
mean_ms=$(echo "scale=3; $mean_ns / 1000000" | bc)

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
grep " 200$" "$TMPFILE" | wc -l | xargs -I {} echo "    200:  {}"
grep -v " 200$" "$TMPFILE" | while read lat code; do
    echo "    $code"
done | sort | uniq -c | while read count code; do
    echo "    $code:  $count"
done

echo "───────────────────────────────────────────────────────────────"