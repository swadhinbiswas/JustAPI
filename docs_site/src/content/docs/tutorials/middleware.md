---
title: Middleware
description: Intercept requests and responses with middleware for logging, timing, CORS, and custom processing.
---

Middleware lets you run code before and after every request. JustAPI supports the same middleware pattern as FastAPI and Starlette.

## Basic HTTP Middleware

Use the `@app.middleware("http")` decorator:

```python
import time
from justapi import JustAPIApp

app = JustAPIApp()


@app.middleware("http")
def timing_middleware(request, call_next):
    start = time.time()
    response = call_next(request)
    elapsed = time.time() - start
    response["headers"] = response.get("headers", []) + [
        (b"X-Process-Time", str(elapsed).encode())
    ]
    return response
```

The middleware receives the `request` and a `call_next` function that dispatches the request down the chain. The return value is a response dict.

## CORS Middleware

JustAPI has a built-in CORS configuration method:

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

```python
app.enable_secure_headers(with_hsts=True)
```

This adds:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `Strict-Transport-Security` (when `with_hsts=True`)
- `Content-Security-Policy`
- `X-XSS-Protection: 0`
- `Referrer-Policy`

## Multiple Middleware

Middlewares are executed in registration order:

```python
@app.middleware("http")
def middleware_one(request, call_next):
    print("Before 1")
    response = call_next(request)
    print("After 1")
    return response


@app.middleware("http")
def middleware_two(request, call_next):
    print("Before 2")
    response = call_next(request)
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

You can add middleware to specific routes or routers using dependencies:

```python
from justapi import Depends, HTTPException, Header


def require_admin(authorization: str = Header(...)):
    if authorization != "Bearer admin-token":
        raise HTTPException(403, "Admin access required")


@router.get("/admin/users", dependencies=[Depends(require_admin)])
def list_all_users(request):
    return {"users": [...]}


app.include_router(router, prefix="/api/v1")
```

## Real-World Patterns

### Request Logging Middleware

```python
import logging

logger = logging.getLogger("justapi")


@app.middleware("http")
def log_requests(request, call_next):
    method = request.method
    path = request.path
    logger.info(f"→ {method} {path}")
    response = call_next(request)
    status = response.get("status", 200)
    logger.info(f"← {method} {path} → {status}")
    return response
```

### Rate Limiting Middleware (via Dependency)

```python
from justapi import Depends, HTTPException, Header
import time

request_counts = {}

def rate_limiter(ip: str = Header("X-Forwarded-For")):
    now = time.time()
    window = 60  # 1 minute
    max_requests = 100

    # Clean old entries
    request_counts[ip] = [t for t in request_counts.get(ip, []) if now - t < window]
    request_counts[ip].append(now)

    if len(request_counts[ip]) > max_requests:
        raise HTTPException(429, "Rate limit exceeded")


@app.get("/api/", dependencies=[Depends(rate_limiter)])
def api_endpoint(request):
    return {"data": "ok"}
```

## How It Works Internally

1. Middleware runs in the Rust middleware chain, not Python
2. `call_next` dispatches through the chain to the handler
3. The response dict flows back through each middleware's post-processing
4. Built-in middleware (CORS, rate limiting) runs in Rust with zero GIL overhead

## Next Steps

- [Error Handling](/tutorials/error-handling/) — Custom error responses
- [Dependency Injection](/tutorials/dependency-injection/) — Reusable components
- [Resilience Patterns](/advanced/resilience-patterns/) — Circuit breakers, rate limiting
