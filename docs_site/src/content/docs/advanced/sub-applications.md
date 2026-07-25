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

Mount an entire ASGI app at a sub-path:

```python
from justapi import JustAPIApp
from starlette.applications import Starlette

app = JustAPIApp()
sub_app = Starlette()
app.mount("/api/v2", sub_app)
```

## See Also

- [Routing & Sub-routers](/tutorials/routing-subrouters/) — router basics
- [Bigger Applications](/tutorials/routing-subrouters/) — multi-file structure
- [Behind a Proxy](/advanced/behind-a-proxy/) — reverse proxy setup
