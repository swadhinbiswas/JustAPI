---
title: MCP Server
description: Built-in MCP (Model Context Protocol) server for AI agent integration in JustAPI.
keywords: [JustAPI, MCP, agent, AI, tools, Model Context Protocol]
---

## Register Tools

Expose your routes as MCP tools:

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.tool
def get_weather(city: str) -> dict:
    """Get current weather for a city."""
    return {"city": city, "temp": 72, "condition": "sunny"}

@app.tool(name="search_db", description="Search the database")
def search(query: str) -> list:
    return db.search(query)
```

## List Tools

```python
tools = app.list_tools()
# Returns list of MCP tool descriptors
```

## Call Tools

```python
result = app.call_tool("get_weather", {"city": "London"})
# Returns: {"city": "London", "temp": 72, "condition": "sunny"}
```

## Built-in MCP Tools

JustAPI registers these introspection tools automatically:

| Tool | Description |
|------|-------------|
| `list_routes` | List all registered API routes |
| `get_signature` | Full signature for one route |
| `explain_endpoint` | Natural-language explanation of an endpoint |
| `generate_snippet` | Client code snippet for an endpoint |

## MCP Server CLI

Run the MCP stdio server for agent integration:

```bash
python -m justapi.mcp_server --base-url http://localhost:8000
```

## System Routes

With `app.enable_system_routes()`, these routes expose tool data:

| Route | Description |
|-------|-------------|
| `/_system/tools` | List registered tools |
| `/_system/tools/call` | Invoke a tool by name |
| `/_system/help` | Rich route descriptors |
| `/_system/help/{name}` | Detailed help for one route |

## See Also

- [Agent System](/advanced/agent-system/) — full agent guide
- [System Routes](/api-reference/system-routes/) — introspection routes
- [Session](/api-reference/session/) — agent session state
