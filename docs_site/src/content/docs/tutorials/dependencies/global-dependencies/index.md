---
title: Global Dependencies
description: Apply dependencies to every route in your JustAPI application.
keywords: [JustAPI, global dependencies, app-wide, middleware]
---

## App-Level Dependencies

Declare dependencies that apply to all routes:

```python
from justapi import JustAPIApp, Depends

def verify_token():
    token = request.headers.get("Authorization", "")
    if not token:
        raise HTTPException(status_code=401)
    return True

app = JustAPIApp(dependencies=[Depends(verify_token)])

@app.get("/public")
def public_route():
    # verify_token still runs
    return {"message": "This requires auth too"}
```

## Router-Level Dependencies

Apply to all routes in a specific router:

```python
from justapi import APIRouter

router = APIRouter(dependencies=[Depends(verify_token)])

@router.get("/items/")
def list_items():
    return []

@router.post("/items/")
def create_item():
    return {}

app.include_router(router, prefix="/api")
```

## Mixing Global + Local

```python
def require_auth():
    # authentication check
    pass

def require_admin():
    # admin check
    pass

# App-wide auth
app = JustAPIApp(dependencies=[Depends(require_auth)])

# This route also requires admin
@app.get("/admin/users", dependencies=[Depends(require_admin)])
def admin_users():
    return []
```

## See Also

- [Dependencies in Path Ops](/tutorials/dependencies/dependencies-in-path-ops/) — route-level deps
- [Security](/tutorials/security/first-steps/) — authentication patterns
