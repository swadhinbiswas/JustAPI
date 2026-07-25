---
title: Session
description: Manage per-user session state in JustAPI — create, read, update, delete sessions.
keywords: [JustAPI, session, state, agent, user session]
---

## Enable Sessions

```python
from justapi import JustAPIApp

app = JustAPIApp()
app.enable_sessions(cookie_name="session_id")
```

## Session Methods

| Method | Description |
|--------|-------------|
| `app.create_session(data, id=None)` | Create a new session |
| `app.get_session(id)` | Get session data by ID |
| `app.set_session(id, data)` | Replace session data |
| `app.update_session(id, **fields)` | Shallow merge fields |
| `app.delete_session(id)` | Delete a session |

## Example

```python
@app.post("/sessions")
def create_session():
    session_id = app.create_session({"user_id": 1, "role": "admin"})
    return {"session_id": session_id}

@app.get("/sessions/{session_id}")
def get_session(session_id: str):
    data = app.get_session(session_id)
    if data is None:
        raise HTTPException(status_code=404, detail="Session not found")
    return data

@app.patch("/sessions/{session_id}")
def update_session(session_id: str, fields: dict):
    app.update_session(session_id, **fields)
    return {"status": "updated"}

@app.delete("/sessions/{session_id}")
def delete_session(session_id: str):
    app.delete_session(session_id)
    return {"status": "deleted"}
```

## Agent Sessions

Sessions are the foundation for the agent system. The `Session` class wraps session state with a typed interface:

```python
from justapi import Session

@app.get("/agent")
def agent_endpoint(request):
    session = Session(request)
    session.set("context", {"topic": "weather"})
    context = session.get("context")
    return {"context": context}
```

## See Also

- [Agent System](/advanced/agent-system/) — MCP tools and agent sessions
- [Tool Registration](/advanced/agent-system/) — registering tools
