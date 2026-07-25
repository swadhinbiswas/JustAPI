---
title: Lifespan Events
description: Run startup and shutdown code in JustAPI using the lifespan context manager.
keywords: [JustAPI, lifespan, startup, shutdown, events, lifecycle]
---

## Lifespan Context Manager

Use `@asynccontextmanager` for startup/shutdown:

```python
from contextlib import asynccontextmanager
from justapi import JustAPIApp

@asynccontextmanager
async def lifespan(app):
    # Startup: run before the server starts
    print("Starting up...")
    db = await create_connection()
    app.state.db = db
    yield
    # Shutdown: run after the server stops
    print("Shutting down...")
    await db.close()

app = JustAPIApp(lifespan=lifespan)
```

## Accessing State

```python
@app.get("/health")
def health(request):
    db = request.app.state.db
    return {"status": "ok" if db else "error"}
```

## Multiple Resources

```python
@asynccontextmanager
async def lifespan(app):
    # Start all resources
    app.state.db = await create_db()
    app.state.cache = await create_cache()
    app.state.queue = await create_queue()
    yield
    # Stop all resources
    await app.state.queue.close()
    await app.state.cache.close()
    await app.state.db.close()

app = JustAPIApp(lifespan=lifespan)
```

## See Also

- [Settings & Environment Variables](/advanced/settings/) — configuration
- [Health Checks](/observability/health-checks/) — monitoring
- [SQL Databases](/tutorials/database-integration/) — database lifecycle
