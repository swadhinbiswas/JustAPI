---
title: APIRouter
description: API reference for APIRouter in JustAPI, the FastAPI alternative — modular router for grouping related routes in separate files, with sub-router support and include_router.
keywords: [apirouter, fastapi alternative, justapi, modular router, route groups, sub-routers, include_router, url_for]
---

`APIRouter` lets you organize routes into modular groups, then include them in the main application.

## Constructor

```python
from justapi import APIRouter

router = APIRouter(
    prefix="/api/v1",
    tags=["API v1"],
    dependencies=None,
    middlewares=None,
    responses=None,
    deprecated=False,
    name=None,
    include_in_schema=True,
)
```

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `prefix` | `str` | `""` | URL prefix applied to all routes |
| `tags` | `list[str]` | `None` | OpenAPI tags for all routes |
| `dependencies` | `list[Depends]` | `None` | Router-level dependencies (applied to all routes) |
| `middlewares` | `list[Callable]` | `None` | Router-level middleware functions |
| `responses` | `dict` | `None` | Default OpenAPI responses |
| `deprecated` | `bool` | `False` | Mark all routes as deprecated |
| `name` | `str` | `None` | Router name |
| `include_in_schema` | `bool` | `True` | Include routes in OpenAPI |

## Methods

`APIRouter` supports the same route decorators as `JustAPIApp`:

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
| `sse(path)` | Register Server-Sent Events |
| `route(path, methods)` | Register multi-method route |
| `frontend(path, directory)` | Static frontend mount |
| `url_for(name, **path_params)` | Build URL from named route |

Each method accepts the same parameters as the corresponding `JustAPIApp` method (dependencies, tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name, etc.).

## Including Routers

### In the Main App

```python
from justapi import JustAPIApp
from app.routers.users import router as users_router

app = JustAPIApp()
app.include_router(users_router, prefix="/api/v1", tags=["Users"])
```

### `include_router()` Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `router` | `APIRouter` | required | The router to include |
| `prefix` | `str` | `""` | Additional prefix on top of the router's own prefix |
| `tags` | `list[str]` | `None` | Tags prepended to each route's existing tags |

### Sub-Routers (Nested)

APIRouter instances can include other APIRouter instances, enabling nested route hierarchies:

```python
from justapi import APIRouter

users_router = APIRouter(prefix="/users")

@users_router.get("/{user_id}")
def get_user(request, user_id: int): ...

admin_router = APIRouter(prefix="/admin")
admin_router.include_router(users_router)

# Final URL: /admin/users/{user_id}
```

## URL Construction

The final URL for each route follows FastAPI merge rules:

```
include_router_prefix + router.prefix + route_path
```

Router-level tags and dependencies are merged with route-level ones.

## Router-Level Dependencies and Middleware

Dependencies and middleware declared on the router apply to all routes within it:

```python
from justapi import APIRouter, Depends, Header, HTTPException

def require_token(authorization: str = Header(...)):
    if not authorization.startswith("Bearer "):
        raise HTTPException(401)

admin_router = APIRouter(
    prefix="/admin",
    tags=["Admin"],
    dependencies=[Depends(require_token)],
)

@admin_router.get("/users")
def list_users(request):
    return [{"id": 1}]
```

## See Also

- [Routing API](/api-reference/routing/) — Route decorator reference
- [JustAPIApp](/api-reference/justapiapp/) — Main application reference
- [Routing & Sub-routers Tutorial](/tutorials/routing-subrouters/) — Step-by-step guide
