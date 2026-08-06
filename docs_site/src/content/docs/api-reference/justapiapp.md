---
title: JustAPIApp
description: API reference for JustAPIApp — the core application class of the FastAPI alternative JustAPI. Register routes, middleware, plugins, and run the server.
keywords: [justapiapp, fastapi alternative, justapi, application class, route registration, middleware, add_api_route, on_event, mount, app.state]
---

`JustAPIApp` is the central class of any JustAPI application. It manages route registration, middleware chains, plugin lifecycle, database connections, and server startup.

## Constructor

```python
from justapi import JustAPIApp

app = JustAPIApp(
    title="My API",
    version="1.0.0",
    description="A high-performance API powered by JustAPI",
    servers=None,
    contact=None,
    license_info=None,
    terms_of_service=None,
    openapi_url="/openapi.json",
    docs_url="/docs",
    redoc_url="/redoc",
    scalar_url="/scalar",
)
```

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `title` | `str` | `"JustAPI"` | API title for OpenAPI docs |
| `version` | `str` | `"1.0.0"` | API version for OpenAPI docs |
| `description` | `str` | `""` | API description |
| `servers` | `list[dict]` | `None` | OpenAPI servers array |
| `contact` | `dict` | `None` | OpenAPI contact info `{"name": ..., "url": ..., "email": ...}` |
| `license_info` | `dict` | `None` | OpenAPI license info `{"name": ..., "url": ...}` |
| `terms_of_service` | `str` | `None` | OpenAPI terms of service URL |
| `openapi_url` | `str` | `"/openapi.json"` | OpenAPI schema URL |
| `docs_url` | `str` | `"/docs"` | Swagger UI URL |
| `redoc_url` | `str` | `"/redoc"` | ReDoc URL |
| `scalar_url` | `str` | `"/scalar"` | Scalar UI URL |

## Application State

Use `app.state` to store application-level data (FastAPI parity):

```python
app.state.db_pool = create_pool()
app.state.cache = RedisCache()
```

## Route Registration

### Decorator Style

| Method | Description |
|---|---|
| `get(path, ...)` | Register GET route |
| `post(path, body_schema, ...)` | Register POST route |
| `put(path, body_schema, ...)` | Register PUT route |
| `patch(path, body_schema, ...)` | Register PATCH route |
| `delete(path, ...)` | Register DELETE route |
| `head(path, ...)` | Register HEAD route |
| `options(path, ...)` | Register OPTIONS route |
| `trace(path, ...)` | Register TRACE route |
| `query(path, body_schema, ...)` | Register QUERY method (RFC 10008) |
| `websocket(path)` | Register WebSocket handler |
| `sse(path)` | Register Server-Sent Events route |
| `route(path, methods)` | Register multi-method route |

### Non-Decorator Style (FastAPI Parity)

Register routes programmatically without decorators:

```python
def my_handler(request):
    return {"ok": True}

app.add_api_route("/items", my_handler, methods=["GET", "POST"])
app.add_api_websocket_route("/ws", my_ws_handler)
```

### Route Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `path` | `str` | required | URL path template (e.g., `/items/{id}`) |
| `body_schema` / `schema` | `Schema` subclass | `None` | Schema for request body validation in Rust |
| `dependencies` | `list[Depends]` | `None` | Route-level dependencies |
| `middlewares` | `list[Callable]` | `None` | Route-level middlewares |
| `tags` | `list[str]` | `None` | OpenAPI tags |
| `summary` | `str` | `None` | OpenAPI operation summary |
| `description` | `str` | `None` | OpenAPI operation description |
| `deprecated` | `bool` | `False` | Mark as deprecated in OpenAPI |
| `status_code` | `int` | `None` | Default response status code |
| `responses` | `dict` | `None` | Additional OpenAPI responses |
| `operation_id` | `str` | `None` | OpenAPI operation ID |
| `openapi_extra` | `dict` | `None` | Extra OpenAPI metadata |
| `include_in_schema` | `bool` | `True` | Exclude from OpenAPI |
| `name` | `str` | `None` | Named route (for `url_for()`) |
| `native` | `bool` | `False` | Execute entirely in Rust (724k+ RPS) |

## Sub-Applications

Mount APIRouter instances or static directories:

```python
# Mount an APIRouter
app.mount("/api/v1", users_router)

# Mount a static directory
app.mount("/static", "static", name="static")
```

## Lifecycle Events

Register startup and shutdown handlers (FastAPI parity):

```python
@app.on_event("startup")
def init_db():
    app.state.db = create_connection()

@app.on_event("shutdown")
def close_db():
    app.state.db.close()
```

## Including Routers

```python
app.include_router(router, prefix="/api/v1", tags=["API"])
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `router` | `APIRouter` | required | The router to include |
| `prefix` | `str` | `""` | Additional URL prefix |
| `tags` | `list[str]` | `None` | Additional OpenAPI tags |

## Controller Classes

```python
from justapi import Controller, controller, route_get

@controller("/users")
class UserController(Controller):
    @route_get("/{user_id}")
    def get_user(self, request, user_id: int):
        return {"user_id": user_id}

app.include_controller(UserController)
```

## Middleware & Security

| Method | Description |
|---|---|
| `middleware(type)` | Decorator to register HTTP middleware (`"http"`) |
| `add_cors(allow_origins, ...)` | Configure CORS |
| `enable_secure_headers(with_hsts)` | Add security headers |
| `add_middleware(middleware_cls, **kwargs)` | Add a middleware class; CORS classes are translated to the Rust-native CORS middleware (`add_cors`) |
| `add_exception_handler(exc_class, handler)` | Register custom exception handler |

## Resilience

| Method | Description |
|---|---|
| `enable_circuit_breaker(failure_threshold, reset_timeout_ms)` | Enable circuit breaker |
| `enable_request_coalescing(headers)` | Deduplicate concurrent identical requests |
| `enable_http3(cert_path, key_path)` | Serve the app over HTTP/3 (QUIC) on the same port — requires the `http3` build feature and PEM TLS files |

## Database

| Method | Description |
|---|---|
| `set_database(url, ...)` | Configure database connection pool |
| `connect_database()` | Eagerly connect the pool |
| `db` (property) | Resolved `DbPool` handle |

## Multi-Protocol

| Method | Description |
|---|---|
| `graphql(path)` | Mount GraphQL endpoint |
| `set_grpc_addr(addr)` | Set gRPC listen address |
| `add_grpc_service(servicer, add_func)` | Register gRPC service |

## Scheduling

| Method | Description |
|---|---|
| `schedule(cron_expr, func)` | Register cron job (5-field UTC) |
| `every(seconds, func)` | Register interval job |

## Agent System

| Method | Description |
|---|---|
| `tool(func, name, description)` | Register MCP tool |
| `stream_json(path, schema, mode)` | Validated JSON streaming |
| `enable_sessions(cookie_name)` | Enable session store |
| `enable_system_routes()` | Enable `/_system/*` endpoints |

## Observability

| Method | Description |
|---|---|
| `register_health_check(name, callable)` | Register custom health check |

## Utilities

| Method | Description |
|---|---|
| `url_for(name, **path_params)` | Build URL for named route |
| `frontend(path, directory)` | Serve static SPA |
| `use(plugin)` | Register a plugin |

## Server Startup

```python
app.run(
    addr="127.0.0.1:8000",
    max_body_size=50 * 1024 * 1024,
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `addr` | `str` | `"127.0.0.1:8000"` | Bind address |
| `max_body_size` | `int` | `52428800` | Max request body size (bytes) |

## Example: Full Configuration

```python
from justapi import JustAPIApp, Depends, HTTPException, Schema, APIRouter

app = JustAPIApp(
    title="Enterprise API",
    version="2.0.0",
    servers=[{"url": "https://api.example.com"}],
    contact={"name": "Support", "email": "support@example.com"},
)

app.state.cache = {}

@app.on_event("startup")
def startup():
    app.state.db = connect()

app.set_database("postgres://localhost/mydb")
app.add_cors(allow_origins=["https://app.example.com"])
app.enable_circuit_breaker()

sub = APIRouter(prefix="/admin")
@sub.get("/health")
def health(request):
    return {"status": "ok"}

app.mount("/admin", sub)

if __name__ == "__main__":
    app.run("0.0.0.0:8080")
```

## See Also

- [Routing API](/api-reference/routing/) — Route decorator reference
- [APIRouter](/api-reference/apirouter/) — Modular route grouping
- [JustAPIApp Tutorial](/tutorials/routing-subrouters/) — Step-by-step guide
