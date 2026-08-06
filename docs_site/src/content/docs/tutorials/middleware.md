---
title: Middleware
description: Intercept requests and responses with JustAPI middleware for logging, timing, CORS, and custom processing.
keywords: middleware, CORS, request interception, JustAPI, Rust middleware
---

Middleware lets you run code before and after every request. JustAPI has two
middleware layers:

- **Native Rust middleware** (CORS, JWT, security headers) — configured with
  dedicated methods, runs on every request including the native fast path.
- **Python middleware** — your own `async def mw(request, call_next)` callables,
  registered with `@app.middleware("http")` or per-route `middlewares=`.

## Basic HTTP Middleware

Use the `@app.middleware("http")` decorator. Middleware is **async** and must
`await call_next(request)`:

```python
import time
from justapi import JustAPIApp

app = JustAPIApp()


@app.middleware("http")
async def timing_middleware(request, call_next):
    start = time.time()
    response = await call_next(request)
    elapsed = time.time() - start
    response.headers.append(
        (b"X-Process-Time", str(round(elapsed, 4)).encode())
    )
    return response
```

`call_next` dispatches the request down the chain; the return value is the
handler's response object.

> **Performance:** Python middleware runs on the Python side of the boundary and
> is bypassed (must be) by the native fast path. Prefer native middleware
> (`add_cors`, `enable_secure_headers`, `set_jwt_auth`) when possible. Keep
> Python middleware to logic that must run in Python.

## CORS Middleware

JustAPI has a built-in, Rust-native CORS configuration method:

```python
app.add_cors(
    allow_origins=["https://example.com", "https://api.example.com"],
    allow_methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["*"],
    allow_credentials=True,
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `allow_origins` | list[str] | `["*"]` | Allowed origins |
| `allow_methods` | list[str] | All methods | Allowed HTTP methods |
| `allow_headers` | list[str] | `["*"]` | Allowed headers |
| `allow_credentials` | bool | `False` | Allow cookies/auth headers |

## Secure Headers Middleware

Add safe HTTP security headers (Rust-native):

```python
app.enable_secure_headers(with_hsts=True)
```

This adds:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `Strict-Transport-Security` (when `with_hsts=True`)
- `Content-Security-Policy`
- `X-XSS-Protection: 0`

## Multiple Middleware

Middlewares are executed in registration order (first added is outermost):

```python
@app.middleware("http")
async def middleware_one(request, call_next):
    print("Before 1")
    response = await call_next(request)
    print("After 1")
    return response


@app.middleware("http")
async def middleware_two(request, call_next):
    print("Before 2")
    response = await call_next(request)
    print("After 2")
    return response
```

Output for a single request:

```
Before 1
Before 2
[handler executes]
After 2
After 1
```

## Route-Level Middleware

Attach middleware to individual routes:

```python
async def auth_middleware(request, call_next):
    if not (request.get("headers") or {}).get("x-token"):
        return {"status": 401, "body": {"detail": "unauthorized"}}
    return await call_next(request)

@app.get("/protected", middlewares=[auth_middleware])
async def protected(request):
    return {"message": "top secret"}
```

## Real-World Patterns

### Request Logging Middleware

```python
import logging

logger = logging.getLogger("justapi")


@app.middleware("http")
async def log_requests(request, call_next):
    method = request.get("method")
    path = request.get("path")
    logger.info("→ %s %s", method, path)
    response = await call_next(request)
    status = getattr(response, "status_code", None) or 200
    logger.info("← %s %s → %s", method, path, status)
    return response
```

### Authorization via Dependency

```python
from justapi import Depends, HTTPException, Header


def require_admin(authorization: str = Header(...)):
    if authorization != "Bearer admin-token":
        raise HTTPException(403, "Admin access required")


@app.get("/admin/users", dependencies=[Depends(require_admin)])
def list_all_users(request):
    return {"users": []}
```

## Native Fast-Path Note

The Rust native fast path (`native=True`) is a validate-and-echo shortcut that
does **not** execute Python middleware or dependencies. Routes
using `native=True` must not declare Python middleware/dependencies — the
framework rejects that combination at registration time. Native middleware
(`add_cors`, `enable_secure_headers`, `set_jwt_auth`) runs for all routes.

## How It Works Internally

1. **Native** middleware (CORS, security headers, JWT, rate limit) runs in the
   Rust middleware chain with zero GIL overhead.
2. **Python** middleware runs as an async wrapper around your handler on the
   Python side.
3. `call_next` dispatches down the middleware chain to the handler.
4. The handler's result flows back through each middleware's post-processing.

## Next Steps

- [Error Handling](/tutorials/error-handling/) — Custom error responses
- [Dependency Injection](/tutorials/dependency-injection/) — Reusable components
- [Resilience Patterns](/advanced/resilience-patterns/) — Circuit breakers, rate limiting