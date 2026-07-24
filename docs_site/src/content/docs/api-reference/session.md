---
title: Session API (Agent System)
description: "API reference for session management in JustAPI, the FastAPI alternative — Rust-backed agent session state management."
keywords: [session, fastapi alternative, justapi, session management, agent system, state management]
---

## `Session` Object

Sessions provide durable per-client state storage backed by the Rust runtime. Sessions are identified by the `justapi_session` cookie or `?session=` query parameter.

```python
from justapi import Session
```

### Properties & Methods

| Method | Returns | Description |
|---|---|---|
| `session.id` | `str` | Unique session identifier |
| `session.get()` | `dict` | Get all session data |
| `session.update(**kwargs)` | `None` | Update session data with key-value pairs |

### Usage

```python
from justapi import JustAPIApp, Session

app = JustAPIApp()
app.enable_sessions()


@app.get("/agent/state")
def agent_state(request, session: Session):
    prior = session.get().get("visits", 0)
    session.update(visits=prior + 1)
    return {
        "session_id": session.id,
        "visits": prior + 1,
        "data": session.get(),
    }
```

## Enabling Sessions

```python
app.enable_sessions()
```

This activates the Rust-backed session store. Sessions are:
- Created on first use (new session ID generated)
- Persisted across requests via cookie or query param
- Available in any handler by declaring `session: Session`

## System Routes

```python
app.enable_system_routes()
```

Enables:
- `GET /_system/tools` — List registered MCP tools
- `POST /_system/tools/call` — Call an MCP tool

## See Also

- [Agent System Guide](/advanced/agent-system/) — Deep dive into agent-native features
- [Dependency Injection API](/api-reference/dependency-injection/) — Session in DI
- [JustAPIApp](/api-reference/justapiapp/) — enable_sessions() and enable_system_routes()
