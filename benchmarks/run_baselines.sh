#!/usr/bin/env bash
# benchmarks/run_baselines.sh — Run baseline benchmarks against Uvicorn and Granian.
#
# Prerequisites:
#   pip install uvicorn fastapi granian
#   cargo install oha
#
# Usage:
#   cd /home/swadhin/RastAPI
#   bash benchmarks/run_baselines.sh
#
# Output is printed to stdout in a format suitable for pasting into BENCHMARKS.md.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
WORKLOAD_MODULE="benchmarks.workloads"
FASTAPI_MODULE="benchmarks.workloads_fastapi:app"
HOST="127.0.0.1"
PORT="8080"
DURATION="10s"
CONCURRENCY=100
WARMUP_DURATION="2s"
RUNS=1

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[bench]${NC} $*" >&2; }
warn() { echo -e "${YELLOW}[warn]${NC} $*" >&2; }
err() { echo -e "${RED}[error]${NC} $*" >&2; }

# Check prerequisites
check_prereqs() {
    local missing=0
    for cmd in oha python3; do
        if ! command -v "$cmd" &>/dev/null; then
            err "Missing required tool: $cmd"
            missing=1
        fi
    done

    for pkg in uvicorn granian; do
        if ! python3 -c "import $pkg" 2>/dev/null; then
            warn "Python package '$pkg' not installed — will skip its benchmarks"
        fi
    done

    if [[ $missing -ne 0 ]]; then
        err "Install missing tools and retry."
        exit 1
    fi

    log "oha version: $(oha --version 2>&1 || echo 'unknown')"
}

# Print hardware fingerprint
print_hardware() {
    echo ""
    echo "## Hardware Fixture"
    echo ""
    echo "- **CPU:** $(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)"
    echo "- **Cores:** $(nproc) threads"
    echo "- **RAM:** $(free -h | awk '/Mem:/ {print $2}')"
    echo "- **Kernel:** $(uname -sr)"
    echo "- **OS:** $(cat /etc/os-release 2>/dev/null | grep PRETTY_NAME | cut -d= -f2 | tr -d '"' || echo 'unknown')"
    echo "- **Virtualized:** $(systemd-detect-virt 2>/dev/null || echo 'unknown')"
    echo ""
}

# Start a server in the background, wait for it to be ready
start_server() {
    local name="$1"
    shift
    log "Starting $name..."
    cd "$PROJECT_ROOT"
    "$@" >/dev/null 2>&1 &
    local pid=$!
    sleep 3  # Give server time to start

    # Check it's actually running
    if ! kill -0 "$pid" 2>/dev/null; then
        err "$name failed to start"
        return 1
    fi

    # Wait for port to be ready
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

# Stop a server
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

# Run a single benchmark
run_benchmark() {
    local name="$1"
    local url="$2"
    local method="${3:-GET}"
    local extra_args="${4:-}"

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

# Run benchmarks for a specific server
bench_server() {
    local server_name="$1"
    shift
    local start_cmd=("$@")

    echo ""
    echo "## Baseline: $server_name (recorded $(date +%Y-%m-%d))"
    echo ""

    local pid
    pid=$(start_server "$server_name" "${start_cmd[@]}") || return 1

    # Get peak RSS before benchmarks
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

    # Get peak RSS after benchmarks
    local rss_after
    rss_after=$(cat "/proc/$pid/status" 2>/dev/null | grep VmHWM | awk '{print $2, $3}' || echo "N/A")
    echo "Peak RSS (VmHWM): $rss_after"
    echo ""

    stop_server "$pid" "$server_name"
}

# Main
main() {
    log "Hyperion Baseline Benchmark Suite"
    log "================================="
    echo ""

    check_prereqs
    print_hardware

    # Uvicorn + raw ASGI
    if python3 -c "import uvicorn" 2>/dev/null; then
        bench_server "Uvicorn (raw ASGI)" \
            python3 -m uvicorn "$WORKLOAD_MODULE:app" \
            --host "$HOST" --port "$PORT" --workers 4
    else
        warn "Skipping Uvicorn (not installed)"
    fi

    # Uvicorn + FastAPI
    if python3 -c "import uvicorn; import fastapi" 2>/dev/null; then
        bench_server "Uvicorn+FastAPI" \
            python3 -m uvicorn "$FASTAPI_MODULE" \
            --host "$HOST" --port "$PORT" --workers 4
    else
        warn "Skipping Uvicorn+FastAPI (not installed)"
    fi

    # Granian
    if python3 -c "import granian" 2>/dev/null; then
        bench_server "Granian" \
            python3 -m granian --interface asgi "$WORKLOAD_MODULE:app" \
            --host "$HOST" --port "$PORT" --workers 4
    else
        warn "Skipping Granian (not installed)"
    fi

    log "Done. Copy results above into BENCHMARKS.md."
}

main "$@"
