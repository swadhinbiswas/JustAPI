# Agent-native demo

A single, runnable example that exercises justapi's three agent-native
differentiators — all owned by the Rust runtime, not Python:

1. **Native MCP tools** — `add`, `multiply`, `search_docs` are registered with
   `@app.tool`. Their JSON schemas are inferred from type hints and stored in
   Rust. They are listed at `GET /_system/tools` and callable via
   `POST /_system/tools/call` (MCP shape) or the bundled `app.run_mcp_stdio()`
   stdio server.
2. **Durable agent session state** — `app.enable_sessions()` turns on the
   Rust-backed session store. Declare `session: Session` on any handler to get
   per-client state with no plumbing: the id is read from the
   `justapi_session` cookie or `?session=` query param, and a known-but-new id
   is materialized automatically so state survives across requests.
3. **Streaming validated output** — `@app.stream_json` streams a generator of
   Python objects as NDJSON, validating every item against a JSON Schema *in
   Rust* before it is written to the socket. Invalid items abort the stream.

## Run

One command — starts the server, runs the full demo (tools list, tool call,
streaming agent run, session state, session persistence), and stops:

```bash
bash examples/agent_demo/run_demo.sh
```

Manually, step by step:

```bash
# from the repo root
uv run examples/agent_demo/app.py
# or:  python examples/agent_demo/app.py

curl -N 'http://127.0.0.1:8000/agent/run?session=demo&query=streaming'
curl    'http://127.0.0.1:8000/agent/state?session=demo'
curl    'http://127.0.0.1:8000/_system/tools'
curl -X POST 'http://127.0.0.1:8000/_system/tools/call' \
     -H 'content-type: application/json' \
     -d '{"name":"add","arguments":{"a":5,"b":7}}'
```

The same `app` object also serves an MCP stdio server via `app.run_mcp_stdio()`.
