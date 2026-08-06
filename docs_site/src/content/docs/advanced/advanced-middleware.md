---
title: Advanced Middleware
description: Add security headers, CORS, JWT, request coalescing, and custom Python pipeline middleware in JustAPI.
keywords: [JustAPI, middleware, security headers, CORS, JWT, GZip]
---

JustAPI's middleware runs in two layers:

1. **Native Rust middleware** — CORS, JWT, security headers, and request
   coalescing execute in the Rust chain for every request (covering the Rust
   native fast path too). These are enabled with dedicated `enable_*` / `add_*`
   methods.
2. **Python middleware** — your own request `(request, call_next)` callables for
   cross-cutting logic (timing, auth checks, header injection). Registered with
   `@app.middleware("http")` or on a per-route `middlewares=` list.

## Performance note

Python middleware runs on the Python side of the boundary. For maximum
throughput, prefer the native Rust middleware (CORS, security headers, JWT,
rate limiting) or the native fast path — use Python middleware only for logic
that must run in Python.

## Security Headers

Apply safe HTTP response headers (`X-Content-Type-Options: nosniff`,
`X-Frame-Options: DENY`, `Content-Security-Policy: default-src 'self'`,
`X-XSS-Protection: 0`):

```python
from justapi import JustAPIApp

app = JustAPIApp()

# Omit HSTS by default (plaintext local termination). Pass with_hsts=True only
# if you terminate TLS in-process.
app.enable_secure_headers()
```

> Native (Rust-side) gzip response compression is available as a server option
> (see `justapi serve --compress`). If you need per-route compression, wrap it
> in Python middleware or place JustAPI behind a reverse proxy that handles TLS
> and compression (see [Behind a Proxy](/advanced/behind-a-proxy/)).

## CORS

Rust-native CORS, covering all routes including the fast path and 404s:

```python
app.add_cors(
    allow_origins=["https://example.com"],
    allow_methods=["*"],
    allow_headers=["*"],
    allow_credentials=True,
)
```

## JWT Authentication (Rust-native)

```python
app.set_jwt_auth(secret="your-secret", algorithm="HS256")
```

## Request Coalescing (Singleflight)

Fuse concurrent identical requests into one upstream call:

```python
app.enable_request_coalescing(headers=["x-user-id"])
```

## Custom Python Middleware

Write your own request/response middleware as a plain callable `(request, next)`.

```python
import time
from justapi import JustAPIApp

app = JustAPIApp()

@app.middleware("http")
async def timing_middleware(request, call_next):
    start = time.time()
    response = await call_next(request)
    response.headers["X-Process-Time"] = str(round(time.time() - start, 4))
    return response
```

Or attach it to a single route:

```python
async def auth_middleware(request, call_next):
    if not (request.get("headers") or {}).get("x-token"):
        return {"status": 401, "body": {"detail": "unauthorized"}}
    return await call_next(request)

@app.get("/protected", middlewares=[auth_middleware])
async def protected(request):
    return {"message": "top secret"}
```

> **Note:** The Rust native fast path is a validate-and-echo shortcut that does
> not execute Python middleware or dependencies. If a route uses `native=True`,
> it must not declare dependencies or Python middleware (the framework refuses
> that combination at registration time).

## See Also

- [Middleware](/tutorials/middleware/) — basic middleware usage
- [Behind a Proxy](/advanced/behind-a-proxy/) — reverse proxy configuration
- [JWT & Auth](/security/authentication/) — authentication middleware