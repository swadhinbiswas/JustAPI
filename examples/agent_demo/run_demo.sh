#!/usr/bin/env bash
# examples/agent_demo/run_demo.sh — one-command agent-native demo.
#
# Starts the JustAPI agent demo, exercises all three differentiators
# (native MCP tools, durable sessions, Rust-validated streaming), shows the
# output, then stops the server. No manual steps.
#
# Usage:
#   bash examples/agent_demo/run_demo.sh [port]
#
# Prereqs: the justapi package must be importable (e.g. a venv with
# `pip install justapi` or the repo's `.venv`).

set -euo pipefail

PORT="${1:-8051}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$(dirname "$HERE")")"

# Pick a python that can import justapi.
if python -c "import justapi" 2>/dev/null; then
    PY=python
elif "$ROOT/crates/justapi-py/.venv/bin/python" -c "import justapi" 2>/dev/null; then
    PY="$ROOT/crates/justapi-py/.venv/bin/python"
elif "$ROOT/.venv/bin/python" -c "import justapi" 2>/dev/null; then
    PY="$ROOT/.venv/bin/python"
else
    echo "[demo] no python with 'justapi' importable found." >&2
    echo "[demo] install it first: pip install justapi   (or rebuild the wheel)" >&2
    exit 1
fi

BASE="http://127.0.0.1:$PORT"
LOG="$(mktemp /tmp/justapi_agent_demo.XXXXXX.log)"

echo "[demo] starting JustAPI agent demo on $BASE (python: $PY)"
"$PY" "$HERE/app.py" "$PORT" >"$LOG" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

# Wait for the server to come up.
for _ in $(seq 1 30); do
    if curl -sf "$BASE/" >/dev/null 2>&1; then break; fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "[demo] server exited early; log tail:" >&2
        tail -20 "$LOG" >&2
        exit 1
    fi
    sleep 0.5
done

sep() { printf '\n────────────────────────────────────────────────────────\n'; }

echo ""
echo "=== 1. Native MCP tools (schemas inferred in Rust) ==="
curl -s "$BASE/_system/tools" | "$PY" -m json.tool | head -20
sep

echo "=== 2. Call a tool through the native call path ==="
curl -s -X POST "$BASE/_system/tools/call" \
    -H 'content-type: application/json' \
    -d '{"name":"add","arguments":{"a":5,"b":7}}'
echo ""
sep

echo "=== 3. Streaming agent run (NDJSON, validated item-by-item in Rust) ==="
curl -sN "$BASE/agent/run?session=demo&query=streaming"
echo ""
sep

echo "=== 4. Durable session state (persisted across requests) ==="
curl -s "$BASE/agent/state?session=demo"
echo ""
echo ""
echo "=== 5. Same session again → run counter incremented ==="
curl -sN "$BASE/agent/run?session=demo&query=streaming" >/dev/null
curl -s "$BASE/agent/state?session=demo"
echo ""
sep

echo ""
echo "[demo] done. Server log: $LOG"
echo "[demo] rerun:  bash examples/agent_demo/run_demo.sh"
