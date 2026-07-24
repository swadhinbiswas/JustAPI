---
title: Dependency Injection
description: Use Depends to inject reusable components into your route handlers.
---

Dependency Injection (DI) lets you declare reusable components that are automatically resolved and injected into your route handlers. JustAPI's DI system mirrors FastAPI's, making migration seamless.

## Basic Dependency

A dependency is any callable that returns a value:

```python
from justapi import JustAPIApp, Depends

app = JustAPIApp()


def common_params(q: str | None = None, skip: int = 0, limit: int = 100):
    return {"q": q, "skip": skip, "limit": limit}


@app.get("/items/")
def list_items(request, params: dict = Depends(common_params)):
    return {"endpoint": "items", **params}


@app.get("/users/")
def list_users(request, params: dict = Depends(common_params)):
    return {"endpoint": "users", **params}
```

```bash
curl "http://127.0.0.1:8000/items/?skip=10&limit=5"
# Output: {"endpoint":"items","q":null,"skip":10,"limit":5}
```

## Authentication Dependency

Dependencies can raise `HTTPException` to reject requests:

```python
from justapi import JustAPIApp, Depends, HTTPException, Header

app = JustAPIApp()


def verify_token(authorization: str = Header(...)):
    if not authorization.startswith("Bearer "):
        raise HTTPException(status_code=401, detail="Invalid authorization header")
    token = authorization.removeprefix("Bearer ")
    if token != "secret-token":
        raise HTTPException(status_code=401, detail="Invalid token")
    return {"user_id": 42, "role": "admin"}


@app.get("/protected")
def protected_route(request, user: dict = Depends(verify_token)):
    return {"message": "Access granted", "user": user}
```

```bash
curl http://127.0.0.1:8000/protected
# Output: {"detail":"Invalid authorization header"}

curl http://127.0.0.1:8000/protected \
  -H "Authorization: Bearer secret-token"
# Output: {"message":"Access granted","user":{"user_id":42,"role":"admin"}}
```

## Database Session Dependency

```python
def get_db(request):
    db = app.db
    try:
        yield db
    finally:
        pass  # Connection pooling handles cleanup


@app.get("/users")
def list_users(request, db=Depends(get_db)):
    return db.query("SELECT * FROM users")
```

## Class-Based Dependencies

```python
class Pagination:
    def __init__(self, skip: int = 0, limit: int = 100):
        self.skip = skip
        self.limit = limit


@app.get("/items/")
def list_items(request, pagination: Pagination = Depends(Pagination)):
    return {"skip": pagination.skip, "limit": pagination.limit}
```

## Dependency Scope & Caching

By default, dependencies are called once per request and the result is cached. All route handlers and nested dependencies share the same instance:

```python
def get_db(request):
    print("Creating DB session")  # Called once per request
    return {"connection": "db-1"}

def get_user(request, db=Depends(get_db)):
    return {"user": "alice", "db": db}

@app.get("/profile")
def profile(request, user=Depends(get_user), db=Depends(get_db)):
    return {"user": user, "db_again": db}
```

In this example, `get_db` is called **once** and its result is reused by `get_user` and `profile`.

## Dependency with `Path()`, `Query()`, `Header()`

```python
from justapi import Path, Query, Header


def pagination(
    skip: int = Query(0, description="Number of items to skip"),
    limit: int = Query(10, description="Max items per page"),
):
    return {"skip": skip, "limit": limit}


def auth(
    user_id: int = Path(..., description="User ID"),
    authorization: str = Header(..., description="Bearer token"),
):
    if authorization != "Bearer secret":
        raise HTTPException(401)
    return {"user_id": user_id}


@app.get("/users/{user_id}/items/")
def list_items(
    request,
    auth_data: dict = Depends(auth),
    pagination_data: dict = Depends(pagination),
):
    return {**auth_data, **pagination_data}
```

## Nested Dependencies

Dependencies can depend on other dependencies:

```python
def get_config(request):
    return {"db_url": "postgres://localhost/mydb"}


def get_db(request, config: dict = Depends(get_config)):
    return {"connection": f"Connected to {config['db_url']}"}


@app.get("/status")
def status(request, db: dict = Depends(get_db)):
    return {"db_status": db["connection"]}
```

## How It Works Internally

1. JustAPI inspects function signatures at registration time
2. Parameters with `Depends()` are resolved before the handler runs
3. Dependencies execute in order (innermost first)
4. Results are cached to avoid redundant calls
5. `HTTPException` raised in a dependency short-circuits the request

## Next Steps

- [Middleware](/tutorials/middleware/) — Request/response interception
- [Error Handling](/tutorials/error-handling/) — Custom exception handlers
- [Database Integration](/tutorials/database-integration/) — Real database dependencies
