#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
HOST="127.0.0.1"
PORT="8080"
DURATION="10s"
CONCURRENCY=100
WARMUP_DURATION="2s"
RUNS=1

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[bench]${NC} $*" >&2; }
warn() { echo -e "${YELLOW}[warn]${NC} $*" >&2; }
err() { echo -e "${RED}[error]${NC} $*" >&2; }

start_server() {
    local name="$1"
    shift
    log "Starting $name..."
    cd "$PROJECT_ROOT"
    "$@" >/dev/null 2>&1 &
    local pid=$!
    sleep 3

    if ! kill -0 "$pid" 2>/dev/null; then
        err "$name failed to start"
        return 1
    fi

    local retries=10
    while ! curl -s "http://${HOST}:${PORT}/hello" >/dev/null 2>&1; do
        retries=$((retries - 1))
        if [[ $retries -le 0 ]]; then
            err "$name didn't respond on port $PORT"
            kill "$pid" 2>/dev/null || true
            return 1
        fi
        sleep 1
    done

    log "$name is ready (PID: $pid)"
    echo "$pid"
}

stop_server() {
    local pid="$1"
    local name="$2"
    if kill -0 "$pid" 2>/dev/null; then
        log "Stopping $name (PID: $pid)..."
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        sleep 1
    fi
}

run_benchmark() {
    local name="$1"
    local url="$2"
    local method="${3:-GET}"

    log "  Warmup ($WARMUP_DURATION)..."
    if [[ "$method" == "POST" ]]; then
        oha -z "$WARMUP_DURATION" -c "$CONCURRENCY" -m POST \
            -H "Content-Type: application/json" \
            -d '{"user":{"name":"test","id":42},"items":[1,2,3],"meta":{"version":"1.0"}}' \
            "$url" >/dev/null 2>&1 || true
    else
        oha -z "$WARMUP_DURATION" -c "$CONCURRENCY" "$url" >/dev/null 2>&1 || true
    fi

    for run in $(seq 1 "$RUNS"); do
        log "  Run $run/$RUNS ($DURATION, $CONCURRENCY connections)..."
        if [[ "$method" == "POST" ]]; then
            oha -z "$DURATION" -c "$CONCURRENCY" -m POST \
                -H "Content-Type: application/json" \
                -d '{"user":{"name":"test","id":42},"items":[1,2,3],"meta":{"version":"1.0"}}' \
                "$url" 2>&1
        else
            oha -z "$DURATION" -c "$CONCURRENCY" "$url" 2>&1
        fi
        echo ""
        echo "---"
        echo ""
    done
}

bench_server() {
    local server_name="$1"
    shift
    local start_cmd=("$@")

    echo ""
    echo "## Benchmark: $server_name (recorded $(date +%Y-%m-%d))"
    echo ""

    local pid
    pid=$(start_server "$server_name" "${start_cmd[@]}") || return 1

    local rss_before
    rss_before=$(cat "/proc/$pid/status" 2>/dev/null | grep VmHWM | awk '{print $2, $3}' || echo "N/A")

    echo "### Workload: hello-world"
    echo ""
    echo "Server command: \`${start_cmd[*]}\`"
    echo ""
    run_benchmark "$server_name hello" "http://${HOST}:${PORT}/hello"

    echo "### Workload: JSON-echo (nested payload)"
    echo ""
    run_benchmark "$server_name echo" "http://${HOST}:${PORT}/echo" "POST"

    local rss_after
    rss_after=$(cat "/proc/$pid/status" 2>/dev/null | grep VmHWM | awk '{print $2, $3}' || echo "N/A")
    echo "Peak RSS (VmHWM): $rss_after"
    echo ""

    stop_server "$pid" "$server_name"
}

main() {
    log "JustAPI Benchmark Suite"
    log "======================="
    echo ""

    # JustAPI 
    bench_server "JustAPI" \
        python3 benchmarks/workloads_justapi.py "$PORT"

    log "Done. Compare these results with run_baselines.sh output."
}

main "$@"
