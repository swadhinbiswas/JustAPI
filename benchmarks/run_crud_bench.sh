#!/usr/bin/env bash
# benchmarks/run_crud_bench.sh — Real DB-backed CRUD benchmark.
#
# Compares JustAPI (Python-handler + Rust-native), FastAPI+SQLAlchemy(async),
# and Robyn on single-row INSERT/SELECT/UPDATE/DELETE against a SQLite file
# (WAL, 10-connection pool) under `oha -c 50`.
#
# Prereqs: `cargo install oha` and the benchmark venv with justapi + fastapi
# + robyn + sqlalchemy + aiosqlite + uvicorn installed.
#
# Usage: bash benchmarks/run_crud_bench.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$HERE")"
VENV_PY="$ROOT/crates/justapi-py/.venv/bin/python"
HOST="127.0.0.1"
DUR="5s"
WARM="2s"
CONC=20
DATE="$(date +%Y-%m-%d)"

log()  { echo -e "\033[0;32m[crud-bench]\033[0m $*" >&2; }
warn() { echo -e "\033[1;33m[warn]\033[0m $*" >&2; }
err()  { echo -e "\033[0;31m[error]\033[0m $*" >&2; }

command -v oha >/dev/null || { err "oha not found (cargo install oha)"; exit 1; }

# Pick a free base port per server to avoid collisions.
JA_PORT=8131
FA_PORT=8132
RO_PORT=8133

# measure <name> <method> <url> [body]
measure() {
  local name="$1" method="$2" url="$3" body="${4:-}"
  log "  $name ($method $url)"
  # warmup
  if [[ "$method" == "POST" || "$method" == "PUT" || "$method" == "DELETE" ]]; then
    oha -z "$WARM" -c "$CONC" -m "$method" -H "Content-Type: application/json" \
        ${body:+-d "$body"} "$url" >/dev/null 2>&1 || true
  else
    oha -z "$WARM" -c "$CONC" "$url" >/dev/null 2>&1 || true
  fi
  local jsonf
  jsonf=$(mktemp /tmp/oha.XXXXXX.json)
  if [[ "$method" == "POST" || "$method" == "PUT" || "$method" == "DELETE" ]]; then
    oha -z "$DUR" -c "$CONC" -m "$method" -H "Content-Type: application/json" \
        --output-format json ${body:+-d "$body"} "$url" > "$jsonf" 2>/dev/null || true
  else
    oha -z "$DUR" -c "$CONC" --output-format json "$url" > "$jsonf" 2>/dev/null || true
  fi
  local rps p50 p99
  rps=$("$VENV_PY" -c "import json,sys; 
try:
    d=json.load(open('$jsonf')); print(round(d['summary']['requestsPerSec'],1))
except Exception as e: print('NA')" 2>/dev/null)
  p50=$("$VENV_PY" -c "import json,sys
try:
    d=json.load(open('$jsonf')); print(round(d['latencyPercentiles']['p50']/1e6,2),'ms')
except Exception: print('NA')" 2>/dev/null)
  p99=$("$VENV_PY" -c "import json,sys
try:
    d=json.load(open('$jsonf')); print(round(d['latencyPercentiles']['p99']/1e6,2),'ms')
except Exception: print('NA')" 2>/dev/null)
  rm -f "$jsonf"
  echo "| $name | $rps | $p50 | $p99 |"
}

start_server() {
  local name="$1" port="$2"; shift 2
  log "Starting $name on :$port"
  cd "$ROOT"
  local logf="/tmp/crud_bench_${name//[^a-zA-Z0-9]/_}.log"
  "$@" >"$logf" 2>&1 &
  local pid=$!
  sleep 3
  local tries=20
  while ! curl -s "http://$HOST:$port/items/1" >/dev/null 2>&1; do
    tries=$((tries-1))
    if [[ $tries -le 0 || ! -d "/proc/$pid" ]]; then
      err "$name failed to start (see $logf)"; tail -5 "$logf" >&2
      kill "$pid" 2>/dev/null || true; return 1
    fi
    sleep 1
  done
  echo "$pid"
}

stop_server() {
  local pid="$1" name="$2"
  [[ -d "/proc/$pid" ]] && { kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 1; }
  log "Stopped $name"
}

BODY='{"name":"widget","qty":7}'

echo ""
echo "## Real DB-backed CRUD benchmark (recorded $DATE)"
echo ""
echo "- **Workload:** single-row INSERT / SELECT / UPDATE / DELETE, SQLite file (WAL, busy_timeout=5000, 10-conn pool)"
echo "- **Note:** single-file SQLite write-serializes (one writer at a time), so write"
echo "  RPS is SQLite-bound (~hundreds) and similar across frameworks; SELECT is the"
echo "  framework-differentiated metric."
echo "- **Tool:** \`oha -c $CONC -z $DUR\`"
echo "- **CPU:** $(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs) ($(nproc) threads)"
echo "- **Kernel:** $(uname -sr)"
echo ""

# ---- JustAPI (Python-handler) ----
PID=$(start_server "JustAPI(py)" "$JA_PORT" "$VENV_PY" "$HERE/crud_justapi.py" python "$JA_PORT") || true
if [[ -n "${PID:-}" ]]; then
  echo "### JustAPI — Python-handler CRUD"
  echo ""
  echo "| Operation | RPS | p50 | p99 |"
  echo "|-----------|-----|-----|-----|"
  measure "INSERT" POST "http://$HOST:$JA_PORT/items" "$BODY"
  measure "SELECT" GET  "http://$HOST:$JA_PORT/items/1"
  measure "UPDATE" PUT  "http://$HOST:$JA_PORT/items/1" "$BODY"
  measure "DELETE" DELETE "http://$HOST:$JA_PORT/items/1"
  echo ""
  stop_server "$PID" "JustAPI(py)"
fi

# ---- JustAPI (Rust-native) ----
PID=$(start_server "JustAPI(native)" "$JA_PORT" "$VENV_PY" "$HERE/crud_justapi.py" native "$JA_PORT") || true
if [[ -n "${PID:-}" ]]; then
  echo "### JustAPI — Rust-native CRUD (fast path)"
  echo ""
  echo "| Operation | RPS | p50 | p99 |"
  echo "|-----------|-----|-----|-----|"
  measure "INSERT" POST "http://$HOST:$JA_PORT/items" "$BODY"
  measure "SELECT" GET  "http://$HOST:$JA_PORT/items/1"
  measure "UPDATE" PUT  "http://$HOST:$JA_PORT/items/1" "$BODY"
  measure "DELETE" DELETE "http://$HOST:$JA_PORT/items/1"
  echo ""
  stop_server "$PID" "JustAPI(native)"
fi

# ---- FastAPI + SQLAlchemy ----
PID=$(start_server "FastAPI" "$FA_PORT" "$VENV_PY" "$HERE/crud_fastapi.py" "$FA_PORT") || true
if [[ -n "${PID:-}" ]]; then
  echo "### FastAPI + SQLAlchemy (async)"
  echo ""
  echo "| Operation | RPS | p50 | p99 |"
  echo "|-----------|-----|-----|-----|"
  measure "INSERT" POST "http://$HOST:$FA_PORT/items" "$BODY"
  measure "SELECT" GET  "http://$HOST:$FA_PORT/items/1"
  measure "UPDATE" PUT  "http://$HOST:$FA_PORT/items/1" "$BODY"
  measure "DELETE" DELETE "http://$HOST:$FA_PORT/items/1"
  echo ""
  stop_server "$PID" "FastAPI"
fi

# ---- Robyn ----
PID=$(start_server "Robyn" "$RO_PORT" "$VENV_PY" "$HERE/crud_robyn.py" "$RO_PORT") || true
if [[ -n "${PID:-}" ]]; then
  echo "### Robyn (sync handler, sqlite3)"
  echo ""
  echo "| Operation | RPS | p50 | p99 |"
  echo "|-----------|-----|-----|-----|"
  measure "INSERT" POST "http://$HOST:$RO_PORT/items" "$BODY"
  measure "SELECT" GET  "http://$HOST:$RO_PORT/items/1"
  measure "UPDATE" PUT  "http://$HOST:$RO_PORT/items/1" "$BODY"
  measure "DELETE" DELETE "http://$HOST:$RO_PORT/items/1"
  echo ""
  stop_server "$PID" "Robyn"
fi

# Cleanup bench db
for ext in "" "-wal" "-shm"; do rm -f "$HERE/crud_bench.sqlite$ext"; done
log "Done. Paste the tables above into BENCHMARKS.md."
