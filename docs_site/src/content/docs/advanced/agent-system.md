---
title: Agent System (MCP Tools & Sessions)
description: Build AI agents with native MCP tools, durable session state, and validated streaming in JustAPI, the FastAPI alternative built for AI.
keywords: JustAPI, FastAPI alternative, AI agents, MCP tools, sessions, streaming, agent system
---

JustAPI includes a first-class agent-native system that differentiates it from traditional web frameworks. It provides **MCP tools**, **durable sessions**, and **validated streaming** — all owned by Rust.

## MCP Tools (`@app.tool`)

Register Python functions as MCP-compatible tools. Schemas are inferred from type hints:

```python
from justapi import JustAPIApp

app = JustAPIApp()


@app.tool
def add(a: int, b: int) -> int:
    """Add two integers."""
    return a + b


@app.tool
def search_docs(query: str, top_k: int = 3) -> list:
    """Search documentation."""
    return [{"text": "JustAPI runs routing in Rust", "score": 0.95}]
```

### Tool Discovery & Invocation

Tools are automatically exposed via system routes:

```bash
# List all registered tools
GET /_system/tools

# Call a tool
POST /_system/tools/call
Content-Type: application/json

{"name": "add", "arguments": {"a": 5, "b": 7}}
```

### MCP Stdio Server

```python
app.run_mcp_stdio()  # Run an MCP stdio server for local AI tool use
```

## Durable Sessions (`app.enable_sessions()`)

Rust-backed per-client state storage:

```python
app.enable_sessions()


@app.get("/agent/state")
def agent_state(request, session: Session):
    prior = session.get().get("runs", 0)
    session.update(runs=prior + 1)
    return {"session_id": session.id, "runs": prior + 1}
```

Sessions are identified by the `justapi_session` cookie or `?session=` query parameter.

## Validated Streaming (`@app.stream_json`)

Stream generator output as NDJSON or JSON array, validating every item in Rust:

```python
STEP_SCHEMA = {
    "type": "object",
    "properties": {
        "step": {"type": "string"},
        "result": {},
    },
    "required": ["step", "result"],
}


@app.stream_json("/agent/run", schema=STEP_SCHEMA, mode="ndjson")
def agent_run(request, session: Session):
    yield {"step": "think", "result": "Analyzing..."}
    yield {"step": "act", "result": "Performing task..."}
    yield {"step": "observe", "result": "Task complete"}
```

Invalid items abort the stream — clients never see partial or invalid data.

## Complete Example

```python
from justapi import JustAPIApp, Session

app = JustAPIApp()
app.enable_sessions()
app.enable_system_routes()


@app.tool
def add(a: int, b: int) -> int:
    return a + b


@app.stream_json("/run", schema=STEP_SCHEMA, mode="ndjson")
def run(request, session: Session):
    result = add(2, 3)
    prior = session.get().get("runs", 0)
    session.update(runs=prior + 1)
    yield {"step": "compute", "result": result}


if __name__ == "__main__":
    app.run("127.0.0.1:8000")
```

## See Also

- [Session API](/api-reference/session/) — Session reference
- [Streaming Output](/advanced/streaming-output/) — Validated streaming deep dive
- [Dependency Injection API](/api-reference/dependency-injection/) — Session in DI
