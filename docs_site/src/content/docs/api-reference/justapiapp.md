---
title: JustAPIApp
description: "API reference for JustAPIApp — the core application class of the FastAPI alternative JustAPI. Register routes, middleware, plugins, and run the server."
keywords: [justapiapp, fastapi alternative, justapi, application class, route registration, middleware]
---

`JustAPIApp` is the central class of any JustAPI application. It manages route registration, middleware chains, plugin lifecycle, database connections, and server startup.

## Constructor

```python
from justapi import JustAPIApp

app = JustAPIApp(
    title="My API",
    version="1.0.0",
    description="A high-performance API powered by JustAPI",
)
```

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `title` | `str` | `"JustAPI"` | API title for OpenAPI docs |
| `version` | `str` | `"1.0.0"` | API version for OpenAPI docs |
| `description` | `str` | `""` | API description for OpenAPI docs |

## Route Registration

| Method | Returns | Description |
|---|---|---|
| `get(path, dependencies, tags)` | decorator | Register GET route |
| `post(path, body_schema, schema, dependencies, tags, native)` | decorator | Register POST route |
| `put(path, body_schema, schema, dependencies, tags, native)` | decorator | Register PUT route |
| `patch(path, body_schema, schema, dependencies, tags, native)` | decorator | Register PATCH route |
| `delete(path, dependencies, tags)` | decorator | Register DELETE route |
| `websocket(path)` | decorator | Register WebSocket handler |
| `include_router(router, prefix)` | `None` | Include routes from APIRouter |

### Route Parameter Details

| Parameter | Type | Default | Description |
|---|---|---|---|
| `path` | `str` | required | URL path template (e.g., `/items/{id}`) |
| `body_schema` | `Schema` subclass | `None` | Schema for request body validation in Rust |
| `schema` | `Schema` subclass | `None` | Alias for `body_schema` |
| `dependencies` | `list[Depends]` | `None` | Route-level dependencies |
| `tags` | `list[str]` | `None` | OpenAPI tags |
| `native` | `bool` | `False` | Execute entirely in Rust (724k+ RPS) |

## Middleware & Security

| Method | Description |
|---|---|
| `middleware(type)` | Decorator to register HTTP middleware (`"http"`) |
| `add_cors(allow_origins, allow_methods, allow_headers, allow_credentials)` | Configure CORS |
| `enable_secure_headers(with_hsts)` | Add security headers |
| `add_middleware(middleware_cls, **kwargs)` | Add middleware class (Starlette parity) |

## Resilience

| Method | Description |
|---|---|
| `enable_circuit_breaker(failure_threshold, reset_timeout_ms)` | Enable circuit breaker pattern |
| `enable_request_coalescing(headers)` | Deduplicate concurrent identical requests |

## Database

| Method | Description |
|---|---|
| `set_database(url, init_sql, max_connections, request_acquire_timeout, wal, pragmas)` | Configure database connection pool |

## Plugins

| Method | Description |
|---|---|
| `use(plugin)` | Register a plugin (calls `build()`, `on_startup()`, `on_shutdown()`) |

## Agent System

| Method | Description |
|---|---|
| `tool` | Decorator to register an MCP tool |
| `stream_json(path, schema, mode)` | Decorator for validated JSON streaming |
| `enable_sessions()` | Enable Rust-backed agent session store |
| `enable_system_routes()` | Enable `/_system/tools` and `/_system/tools/call` |
| `run_mcp_stdio()` | Run MCP stdio server |

## Observability

| Method | Description |
|---|---|
| `register_health_check(name, callable)` | Register a custom health check |

## Server Startup

```python
app.run(
    addr="127.0.0.1:8000",
    max_body_size=50 * 1024 * 1024,  # 50 MiB
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `addr` | `str` | `"127.0.0.1:8000"` | Bind address |
| `max_body_size` | `int` | `52428800` | Max request body size (bytes) |

## Multi-Protocol

| Method | Description |
|---|---|
| `graphql(path)` | Mount GraphQL endpoint with GraphiQL UI |

## Exception Handlers

| Method | Description |
|---|---|
| `exception_handler(exc_class)` | Decorator to register custom exception handler |

## Example: Full Configuration

```python
from justapi import JustAPIApp, Depends, HTTPException, Schema
from pydantic import Field

app = JustAPIApp(
    title="Enterprise API",
    version="2.0.0",
    description="Production-grade API with all features",
)

# Database
app.set_database("postgres://localhost/mydb")

# Security
app.add_cors(allow_origins=["https://app.example.com"])
app.enable_secure_headers(with_hsts=True)

# Resilience
app.enable_circuit_breaker(failure_threshold=5, reset_timeout_ms=30000)

# Agent
app.enable_sessions()
app.enable_system_routes()

# Routes
@app.get("/")
def root(request):
    return {"status": "ok"}

if __name__ == "__main__":
    app.run("0.0.0.0:8080", max_body_size=1024 * 1024)
```
