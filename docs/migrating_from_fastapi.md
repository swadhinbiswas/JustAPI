# Migrating from FastAPI

JustAPI aims for high compatibility with FastAPI. In most cases, migration is as simple as changing imports.

## Import Changes

```python
# FastAPI
from fastapi import FastAPI, Depends, HTTPException

# JustAPI
from justapi import JustAPIApp, Depends, HTTPException
```

## The Application Object

Replace `FastAPI()` with `JustAPIApp()`:

```python
# FastAPI
app = FastAPI()

# JustAPI
app = JustAPIApp()
```

## Middlewares

Middleware in JustAPI acts exactly like FastAPI middlewares using the `http` interceptor:

```python
@app.middleware("http")
async def process_time_header(request, call_next):
    # Your logic
    response = await call_next(request)
    return response
```

## Performance Differences

Once you switch, you should immediately notice a dramatic drop in CPU overhead for routing and connection handling, translating to much higher throughput under load.
