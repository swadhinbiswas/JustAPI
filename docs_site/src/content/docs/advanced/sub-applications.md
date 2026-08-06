---
title: Sub Applications — Mounts
description: Mount sub-applications and routers in JustAPI for modular application architecture.
keywords: [JustAPI, sub-applications, mounts, routers, modular, APIRouter]
---

## Using APIRouter

Split your application into multiple files:

```python
# routers/users.py
from justapi import APIRouter

router = APIRouter(prefix="/users", tags=["users"])

@router.get("/")
def list_users():
    return []

@router.get("/{user_id}")
def get_user(user_id: int):
    return {"user_id": user_id}
```

```python
# main.py
from justapi import JustAPIApp
from routers.users import router as users_router

app = JustAPIApp()
app.include_router(users_router)
```

## Prefix and Tags

```python
app.include_router(users_router, prefix="/api/v1", tags=["users"])
```

All routes in `users_router` now start with `/api/v1/users/`.

## Mounting Sub-Applications

Mount an `APIRouter` at a path prefix:

```python
from justapi import APIRouter, JustAPIApp

app = JustAPIApp()
users_router = APIRouter(prefix="/users")

@users_router.get("/{user_id}")
def get_user(user_id: int):
    return {"user_id": user_id}

app.mount("/api/v2", users_router)
```

`mount()` accepts either an `APIRouter` (or any object exposing `.routes`) or a
static directory path:

```python
# Mount a static directory
app.mount("/static", "static", name="static")
```

> **Note:** JustAPI does not mount third-party ASGI/Starlette applications —
> the runtime is a native Rust pipeline with its own routing and middleware.
> For sub-app structure use `APIRouter` + `mount()` (or `include_router`).
> Run existing Starlette/FastAPI apps unchanged by placing JustAPI behind a
> proxy that routes by path (see [Behind a Proxy](/advanced/behind-a-proxy/)).

## See Also

- [Routing & Sub-routers](/tutorials/routing-subrouters/) — router basics
- [Bigger Applications](/tutorials/routing-subrouters/) — multi-file structure
- [Behind a Proxy](/advanced/behind-a-proxy/) — reverse proxy setup
