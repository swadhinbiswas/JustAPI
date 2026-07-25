---
title: Concurrency and Async / Await
description: Understanding async and await in Python and when to use async def vs def in JustAPI handlers.
keywords: [JustAPI, async, await, concurrency, Python async, FastAPI alternative]
---

JustAPI supports both regular functions (`def`) and asynchronous functions (`async def`) as request handlers. This page explains the difference and when to use each.

## Synchronous vs Asynchronous

```python
from justapi import JustAPIApp

app = JustAPIApp()

# Synchronous handler (def)
@app.get("/sync")
def sync_handler(request):
    return {"message": "Synchronous"}

# Asynchronous handler (async def)
@app.get("/async")
async def async_handler(request):
    return {"message": "Asynchronous"}
```

Both work identically from the user's perspective. The difference is internal.

### `def` — Synchronous

JustAPI runs synchronous handlers in a dedicated thread pool so they don't block the Rust event loop. This is handled automatically by the framework.

Use `def` when your handler does CPU-bound work or calls blocking libraries:

```python
import time

@app.get("/cpu-work")
def cpu_work(request):
    # Simulate CPU-bound work
    result = sum(i * i for i in range(10_000))
    return {"result": result}
```

### `async def` — Asynchronous

Async handlers run directly on the Rust async runtime. They can `await` other async operations without blocking.

Use `async def` when your handler does I/O-bound work (network calls, database queries, file I/O):

```python
import httpx

@app.get("/fetch")
async def fetch_data(request):
    async with httpx.AsyncClient() as client:
        response = await client.get("https://api.example.com/data")
        return response.json()
```

## When to Use Which

| Scenario | Use | Why |
|----------|-----|-----|
| Calling a database ORM (sqlx, SQLAlchemy async) | `async def` | Avoids blocking the runtime |
| HTTP calls to external APIs | `async def` | Non-blocking I/O |
| Simple data transformation | `def` | Cleaner code, no async overhead |
| CPU-bound computation | `def` | Thread pool handles it |
| Mixed I/O + computation | `async def` | Better concurrency |
| Using blocking libraries | `def` | Thread pool prevents blocking |

:::tip
If you're unsure, use `async def`. It's the modern default and works for all cases. Only use `def` if you're calling blocking code directly.
:::

## JustAPI's Concurrency Model

JustAPI uses a Rust async runtime (tokio) for all request handling. The Python handler boundary is designed to avoid GIL contention:

- **`async def`** handlers run on the tokio async runtime directly.
- **`def`** handlers run in a dedicated Python worker thread pool (GIL pool).
- The native fast path (`native=True`) bypasses Python entirely — no GIL, no threads.

This means you get true concurrency for async handlers even on standard CPython, because the GIL is released during Python handler execution.

## Async Database Access

```python
import sqlite3
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/users")
async def list_users(request):
    # Use an async database client
    import aiosqlite
    async with aiosqlite.connect("users.db") as db:
        async with db.execute("SELECT * FROM users") as cursor:
            users = await cursor.fetchall()
            return {"users": users}
```

## Parallel Async Operations

Async handlers can run multiple I/O operations concurrently:

```python
import httpx

@app.get("/aggregate")
async def aggregate(request):
    async with httpx.AsyncClient() as client:
        # Run both requests concurrently
        users_task = client.get("https://api.example.com/users")
        items_task = client.get("https://api.example.com/items")
        
        users_response = await users_task
        items_response = await items_task
        
        return {
            "users": users_response.json(),
            "items": items_response.json(),
        }
```

## Summary

- Use `def` for synchronous, blocking, or CPU-bound work.
- Use `async def` for I/O-bound work that benefits from concurrency.
- JustAPI handles both correctly — you don't need to worry about blocking the runtime.
- For maximum throughput, consider the native fast path (`native=True`) which bypasses Python entirely.

## See Also

- [First Steps](/tutorials/hello-world/) — your first JustAPI app
- [Python Types Intro](/tutorials/python-types/) — type hints in JustAPI
- [Background Tasks](/tutorials/background-tasks/) — post-response async work
