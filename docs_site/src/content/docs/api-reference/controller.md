---
title: Controller
description: Class-based route controllers in JustAPI — organize related routes in a single class.
keywords: [JustAPI, controller, class-based routes, route decorators]
---

## Basic Controller

Use `@controller` and `@route_get/post/etc` to group related routes in a class:

```python
from justapi import JustAPIApp, Controller, controller, route_get, route_post

app = JustAPIApp()

@controller("/users")
class UserController:
    @route_get("/")
    def list_users(self):
        return []

    @route_get("/{user_id}")
    def get_user(self, user_id: int):
        return {"user_id": user_id}

    @route_post("/")
    def create_user(self, body: dict):
        return body

app.include_controller(UserController)
```

All routes are prefixed with `/users`.

## Route Decorators

| Decorator | HTTP Method |
|-----------|-------------|
| `@route_get(path)` | GET |
| `@route_post(path)` | POST |
| `@route_put(path)` | PUT |
| `@route_patch(path)` | PATCH |
| `@route_delete(path)` | DELETE |
| `@route_query(path)` | QUERY (RFC 10008) |
| `@route_sse(path)` | GET (SSE stream) |
| `@route_websocket(path)` | WebSocket |

## Controller with Dependencies

```python
from justapi import Depends, Security

def verify_auth(token: str = Security(oauth2_scheme)):
    return {"user_id": 1}

@controller("/admin", dependencies=[Depends(verify_auth)])
class AdminController:
    @route_get("/dashboard")
    def dashboard(self):
        return {"message": "admin only"}
```

## See Also

- [Routing & Sub-routers](/tutorials/routing-subrouters/) — APIRouter usage
- [APIRouter](/api-reference/apirouter/) — router reference
