"""Agent-native demo: native MCP tools + Rust-backed session state + streaming
validated structured output.

Run:
    uv run examples/agent_demo/app.py          # or: python examples/agent_demo/app.py
    curl -N http://127.0.0.1:8000/agent/run?session=demo

What it shows
-------------
1. ``@app.tool`` registers handler functions as native MCP tools that show up
   at ``GET /_system/tools`` and can be invoked via ``POST /_system/tools/call``
   or the stdio MCP server (``app.run_mcp_stdio``). Tool schemas are inferred
   from type hints and kept on the Rust side.
2. ``app.enable_sessions()`` turns on the Rust-backed agent session store.
   Declare a ``session: Session`` parameter on any handler to get per-client
   state with no extra plumbing (id read from the ``justapi_session`` cookie
   or ``?session=`` query param; a fresh session is created on first use).
3. ``@app.stream_json`` streams a generator of Python objects as NDJSON (or a
   single JSON array), validating every item against a JSON Schema *in Rust*
   before it is written to the socket. Invalid items abort the stream.

This is the "differentiator" trio that separates justapi from plain FastAPI:
tools, durable agent state, and validated streaming — all owned by Rust.
"""

from justapi import JustAPIApp, Session

app = JustAPIApp()
app.enable_sessions()
app.enable_system_routes()


# --- 1. Native MCP tools -----------------------------------------------------
# Pure functions; schemas are inferred from annotations and stored in Rust.


@app.tool
def add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b


@app.tool
def multiply(a: int, b: int) -> int:
    """Multiply two integers."""
    return a * b


@app.tool
def search_docs(query: str, top_k: int = 3) -> list:
    """Pretend to search a document index and return the top-k snippets."""
    corpus = [
        ("justapi runs routing in Rust", 0.9),
        ("streaming output is validated token-by-token", 0.8),
        ("sessions persist agent state across requests", 0.7),
        ("tools are exposed over MCP", 0.6),
    ]
    hits = [c for c in corpus if query.lower() in c[0].lower()] or corpus
    return [{"text": t, "score": s} for t, s in hits[: max(1, top_k)]]


# --- 2. Streaming validated output + session state ---------------------------
# Every yielded object is validated against this schema in Rust before being
# flushed to the client.

STEP_SCHEMA = {
    "type": "object",
    "properties": {
        "step": {"type": "string"},
        "tool": {"type": "string"},
        "result": {},
    },
    "required": ["step", "tool", "result"],
}


@app.stream_json("/agent/run", schema=STEP_SCHEMA, mode="ndjson")
def agent_run(session: Session, query: str = "justapi"):
    # Use a native tool (schema-validated call path) and record the step.
    base = add(2, 3)
    yield {"step": "sum inputs", "tool": "add", "result": base}

    scaled = multiply(base, 4)
    yield {"step": "scale", "tool": "multiply", "result": scaled}

    docs = search_docs(query, top_k=2)
    yield {"step": "retrieve", "tool": "search_docs", "result": docs}

    # Persist the final answer into the durable agent session.
    prior = session.get().get("runs", 0)
    session.update(runs=prior + 1, last_result=scaled, last_query=query)
    yield {"step": "commit", "tool": "session", "result": {"runs": prior + 1}}


@app.get("/agent/state")
def agent_state(session: Session):
    """Return the current session's accumulated state."""
    return {"session_id": session.id, **session.get()}


@app.get("/")
def root():
    return {
        "message": "justapi agent-native demo",
        "tools": "/_system/tools",
        "run": "/agent/run?session=demo",
        "state": "/agent/state?session=demo",
    }


if __name__ == "__main__":
    # HTTP server (streaming + sessions) and an MCP stdio server are both
    # available from the same app object. Optional argv[1] = port.
    import sys

    port = sys.argv[1] if len(sys.argv) > 1 else "8000"
    app.run(f"127.0.0.1:{port}")
