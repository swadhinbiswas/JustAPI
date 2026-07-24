---
title: APIRouter
description: Modular router for grouping related routes in separate files.
---

`APIRouter` lets you organize routes into modular groups, then include them in the main application.

## Constructor

```python
from justapi import APIRouter

router = APIRouter(
    prefix="/api/v1",
    tags=["API v1"],
)
```

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `prefix` | `str` | `""` | URL prefix applied to all routes |
| `tags` | `list[str]` | `None` | OpenAPI tags for all routes |

## Methods

`APIRouter` supports the same route decorators as `JustAPIApp`:

| Method | Signature |
|---|---|
| `get(path, dependencies, tags)` | Same as `JustAPIApp.get()` |
| `post(path, body_schema, schema, dependencies, tags, native)` | Same as `JustAPIApp.post()` |
| `put(path, body_schema, schema, dependencies, tags, native)` | Same as `JustAPIApp.put()` |
| `patch(path, body_schema, schema, dependencies, tags, native)` | Same as `JustAPIApp.patch()` |
| `delete(path, dependencies, tags)` | Same as `JustAPIApp.delete()` |
| `websocket(path)` | Same as `JustAPIApp.websocket()` |

## Including Routers

```python
from justapi import JustAPIApp
from app.routers.users import router as users_router
from app.routers.products import router as products_router

app = JustAPIApp()
app.include_router(users_router, prefix="/api/v1")
```

### `include_router()` Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `router` | `APIRouter` | required | The router to include |
| `prefix` | `str` | `""` | Additional prefix applied on top of the router's own prefix |

## URL Construction

The final URL for each route is: `include_router_prefix` + `router.prefix` + `route_path`

```python
router = APIRouter(prefix="/users")

@router.get("/{user_id}")
def get_user(request, user_id: int): ...

app.include_router(router, prefix="/api/v1")

# Final URL: /api/v1/users/{user_id}
```

## Router-Level Dependencies

```python
from justapi import APIRouter, Depends, Header, HTTPException

def require_token(authorization: str = Header(...)):
    if not authorization.startswith("Bearer "):
        raise HTTPException(401)

admin_router = APIRouter(prefix="/admin", tags=["Admin"])

@admin_router.get("/users", dependencies=[Depends(require_token)])
def list_users(request):
    return [{"id": 1}]
```

## See Also

- [Routing API](/api-reference/routing/) — Route decorator reference
- [JustAPIApp](/api-reference/justapiapp/) — Main application reference
- [Routing & Sub-routers Tutorial](/tutorials/routing-subrouters/) — Step-by-step guide
