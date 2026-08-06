---
title: Migration Guide
description: Migrate from Robyn or Granian to JustAPI for better performance and features.
---

# Migration Guide: From Robyn/Granian to JustAPI

Migrate your existing Robyn or Granian applications to JustAPI for better performance and features.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Migration from Robyn](#2-migration-from-robyn)
3. [Migration from Granian](#3-migration-from-granian)
4. [Feature Comparison](#4-feature-comparison)
5. [Common Patterns](#5-common-patterns)
6. [Performance Gains](#6-performance-gains)

---

## 1. Overview

### Why Migrate to JustAPI?

| Feature | Robyn | Granian | JustAPI |
|---------|-------|---------|---------|
| Performance | Good | Good | **Best** |
| Rust core | Partial | Yes | **Yes** |
| Native fast path | No | No | **Yes** |
| WebSocket | Yes | No | **Yes** |
| gRPC | No | No | **Yes** |
| GraphQL | No | No | **Yes** |
| ML inference | No | No | **Yes** |
| Circuit breakers | No | No | **Yes** |
| Rate limiting | Basic | No | **Yes** |
| OpenAPI | Manual | No | **Auto** |

### Migration Effort

- **Robyn → JustAPI:** ~30 minutes (similar API)
- **Granian → JustAPI:** ~1-2 hours (different approach)

---

## 2. Migration from Robyn

### Step 1: Update Imports

```python
# Before (Robyn)
from robyn import Robyn, Request

# After (JustAPI)
from justapi import JustAPIApp, Request
```

### Step 2: Update App Creation

```python
# Before (Robyn)
app = Robyn(__name__)

# After (JustAPI)
app = JustAPIApp()
```

### Step 3: Update Route Decorators

```python
# Before (Robyn)
@app.get("/hello")
async def hello():
    return "Hello, World!"

# After (JustAPI) - identical!
@app.get("/hello")
async def hello():
    return {"message": "Hello, World!"}
```

### Step 4: Update Request Handling

```python
# Before (Robyn)
@app.get("/users/{id}")
async def get_user(request: Request):
    user_id = request.path_params["id"]
    return {"id": user_id}

# After (JustAPI)
@app.get("/users/{id}")
async def get_user(id: int):  # Auto-extracted!
    return {"id": id}
```

### Step 5: Update WebSocket Handlers

```python
# Before (Robyn)
@app.websocket("/ws")
async def websocket(request):
    await request.accept()
    while True:
        msg = await request.recv_text()
        await request.send_text(f"Echo: {msg}")

# After (JustAPI)
@app.websocket("/ws")
async def websocket(ws):
    await ws.accept()
    while True:
        msg = await ws.receive_text()
        await ws.send_text(f"Echo: {msg}")
```

### Step 6: Update Startup/Shutdown

```python
# Before (Robyn)
@app.startup
async def startup():
    print("Server starting")

@app.shutdown
async def shutdown():
    print("Server shutting down")

# After (JustAPI)
@app.on_startup
async def startup():
    print("Server starting")

@app.on_shutdown
async def shutdown():
    print("Server shutting down")
```

### Step 7: Run the Server

```python
# Before (Robyn)
app.start(port=8000)

# After (JustAPI)
app.run("0.0.0.0:8000")
```

---

## 3. Migration from Granian

### Step 1: Understand the Difference

Granian is an ASGI server (like Uvicorn), not a framework. You likely use it with FastAPI or Starlette.

JustAPI is a self-contained runtime: it hosts its own Rust HTTP server and does
not serve third-party ASGI applications. To move off Granian you migrate the
framework, not just the transport.

### Option A: Full Migration to JustAPI (recommended)

```python
# Before (FastAPI + Granian)
from fastapi import FastAPI
app = FastAPI()

@app.get("/hello")
async def hello():
    return {"message": "Hello"}

# After (JustAPI)
from justapi import JustAPIApp
app = JustAPIApp()

@app.get("/hello")
async def hello():
    return {"message": "Hello"}
```

### Option B: Run Both During Transition

If you need to keep a FastAPI app live while migrating, run JustAPI and the
legacy ASGI app side by side and route by path with a reverse proxy (see
[Behind a Proxy](/advanced/behind-a-proxy/)). Port route-by-route onto JustAPI
and move the proxy weights over as each route is migrated.

### Step 2: Update Server Configuration

```python
# Before (Granian)
import granian
granian.run(
    app,
    interface="asgi",
    host="0.0.0.0",
    port=8000,
    workers=4
)

# After (JustAPI)
app.run("0.0.0.0:8000")
```

### Step 3: Update Middleware

```python
# Before (Starlette middleware)
from starlette.middleware.cors import CORSMiddleware
app.add_middleware(CORSMiddleware, allow_origins=["*"])

# After (JustAPI)
app.add_cors(allow_origins=["*"])
```

---

## 4. Feature Comparison

### Request Handling

```python
# Robyn
@app.get("/users/{id}")
async def get_user(request):
    return {"id": request.path_params["id"]}

# Granian (with FastAPI)
@app.get("/users/{id}")
async def get_user(id: int):
    return {"id": id}

# JustAPI (auto-extraction like FastAPI)
@app.get("/users/{id}")
async def get_user(id: int):
    return {"id": id}
```

### Dependency Injection

```python
# Robyn - No built-in DI

# Granian (with FastAPI)
from fastapi import Depends
async def get_db():
    return db_pool
@app.get("/users")
async def get_users(db = Depends(get_db)):
    return await db.fetch_all()

# JustAPI (FastAPI-compatible)
from justapi import Depends
async def get_db():
    return db_pool
@app.get("/users")
async def get_users(db = Depends(get_db)):
    return await db.fetch_all()
```

### Background Tasks

```python
# Robyn - No built-in background tasks

# Granian (with FastAPI)
from fastapi import BackgroundTasks
@app.post("/users")
async def create_user(background_tasks: BackgroundTasks):
    background_tasks.add_task(send_email, user.email)
    return {"status": "created"}

# JustAPI (FastAPI-compatible)
from justapi import BackgroundTasks
@app.post("/users")
async def create_user(background_tasks: BackgroundTasks):
    background_tasks.add_task(send_email, user.email)
    return {"status": "created"}
```

---

## 5. Common Patterns

### File Uploads

```python
# Robyn
@app.post("/upload")
async def upload(request):
    file = await request.files("file")
    return {"filename": file.name}

# JustAPI
from justapi import UploadFile
@app.post("/upload")
async def upload(file: UploadFile):
    contents = await file.read()
    return {"filename": file.filename, "size": len(contents)}
```

### Error Handling

```python
# Robyn
@app.get("/error")
async def error():
    raise ValueError("Something went wrong")

# JustAPI
from justapi import HTTPException
@app.get("/error")
async def error():
    raise HTTPException(status_code=400, detail="Something went wrong")
```

### Authentication

```python
# Robyn - Manual implementation

# JustAPI - Built-in JWT (Rust-native middleware, validates every request)
from justapi import JustAPIApp

app = JustAPIApp()
app.set_jwt_auth(secret="your-secret-key")

@app.get("/protected")
async def protected_endpoint():
    return {"message": "Authenticated"}
```

---

## 6. Performance Gains

### Expected Improvements

| Metric | Robyn | Granian | JustAPI | Improvement |
|--------|-------|---------|---------|-------------|
| Hello-world RPS | 39,103 | 314,195 | **60,297** | 1.5x vs Robyn |
| JSON echo RPS | 36,899 | 144,502 | **47,415** | 1.3x vs Robyn |
| p99 latency | 11.47ms | 0.74ms | **1.10ms** | 10x vs Robyn |

### Native Fast Path

For schema-validated routes, JustAPI can serve entirely in Rust:

```python
from justapi import JustAPIApp, Schema

app = JustAPIApp()

class UserSchema(Schema):
    name: str
    age: int

# This runs entirely in Rust - no Python GIL!
@app.post("/users", schema=UserSchema, native=True)
async def create_user():
    return {"status": "created"}
```

**Performance:**
- Python handler: ~60,000 RPS
- Native fast path: ~700,000 RPS (**12x faster**)

---

## Need Help?

- **Documentation:** [docs/](../docs/)
- **Examples:** [examples/](../examples/)
- **GitHub Issues:** Report migration problems
