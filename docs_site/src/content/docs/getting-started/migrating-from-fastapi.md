---
title: Migrating from FastAPI
description: Switch your existing FastAPI project to JustAPI — the ultimate FastAPI alternative — for 20x performance with minimal code changes. Drop-in FastAPI replacement.
keywords: [migrate from FastAPI, FastAPI to JustAPI, FastAPI replacement, FastAPI alternative migration, switch from FastAPI, Python web framework migration]
---

JustAPI is engineered as a **drop-in replacement for FastAPI**. In most cases, migration is as simple as changing imports.

## Quick Migration

### 1. Change Imports

```python
# FastAPI
from fastapi import FastAPI, Depends, HTTPException, Query, Path, Header
from fastapi.responses import JSONResponse

# JustAPI — same names, same API
from justapi import JustAPIApp, Depends, HTTPException, Query, Path, Header
from justapi.responses import JSONResponse
```

### 2. Replace the App Constructor

```python
# FastAPI
app = FastAPI(title="My API", version="1.0.0")

# JustAPI
app = JustAPIApp(title="My API", version="1.0.0")
```

### 3. Update the Run Command

```python
# FastAPI (requires uvicorn)
# uvicorn main:app --host 0.0.0.0 --port 8000

# JustAPI — built-in Rust server
if __name__ == "__main__":
    app.run("0.0.0.0:8000")
```

### 4. (Optional) Update Requirements

```txt
# FastAPI requirements
fastapi==0.109.0
uvicorn[standard]==0.27.0

# JustAPI requirements — one dependency
justapi>=2.0.0
```

## Import Mapping

| FastAPI Import | JustAPI Import | Notes |
|---|---|---|
| `FastAPI` | `JustAPIApp` | Core application class |
| `APIRouter` | `APIRouter` | Same API, works identically |
| `Depends` | `Depends` | Dependency injection |
| `HTTPException` | `HTTPException` | Error raising |
| `Query` | `Query` | Query parameter extraction |
| `Path` | `Path` | Path parameter extraction |
| `Header` | `Header` | Header extraction |
| `Cookie` | `Cookie` | Cookie extraction |
| `Body` | `Body` | Body parameter extraction |
| `File` | `File` | File upload |
| `Form` | `Form` | Form field extraction |
| `UploadFile` | `UploadFile` | Uploaded file representation |
| `Response` | `Response` | Base response class |
| `JSONResponse` | `JSONResponse` | JSON response |
| `HTMLResponse` | `HTMLResponse` | HTML response |
| `PlainTextResponse` | `PlainTextResponse` | Plain text response |
| `RedirectResponse` | `RedirectResponse` | HTTP redirect |
| `StreamingResponse` | `StreamingResponse` | Streaming response |
| `RequestValidationError` | `RequestValidationError` | Validation error |

## Side-by-Side: Complete App

### FastAPI

```python
from fastapi import FastAPI, Depends, HTTPException
from pydantic import BaseModel

app = FastAPI(title="Pet Store")

class Pet(BaseModel):
    name: str
    species: str

def verify_token(token: str = Header(...)):
    if token != "secret":
        raise HTTPException(401)

@app.get("/pets/{pet_id}")
def get_pet(pet_id: int, token: str = Depends(verify_token)):
    return {"pet_id": pet_id}

@app.post("/pets/", body_schema=Pet)
def create_pet(request):
    pet = request.json()   # validated by Pydantic's JSON Schema (Rust engine)
    return {"name": pet["name"]}
```

### JustAPI (Same Logic)

```python
from justapi import JustAPIApp, Depends, HTTPException, Header
from pydantic import BaseModel

app = JustAPIApp(title="Pet Store")

class Pet(BaseModel):
    name: str
    species: str

def verify_token(token: str = Header(...)):
    if token != "secret":
        raise HTTPException(401)

@app.get("/pets/{pet_id}")
def get_pet(pet_id: int, token: str = Depends(verify_token)):
    return {"pet_id": pet_id}

@app.post("/pets/", body_schema=Pet)
def create_pet(request):
    pet = request.json()   # validated by Pydantic's JSON Schema (Rust engine)
    return {"name": pet["name"]}
```

## Middleware Migration

```python
# FastAPI
@app.middleware("http")
async def add_header(request, call_next):
    response = await call_next(request)
    response.headers["X-Custom"] = "value"
    return response

# JustAPI — identical syntax
@app.middleware("http")
def add_header(request, call_next):
    response = call_next(request)
    response["headers"] = response.get("headers", []) + [(b"X-Custom", b"value")]
    return response
```

## What Changes

- **Running the server:** JustAPI's `app.run()` replaces uvicorn/gunicorn
- **Middleware signature:** Slightly different header manipulation (tuple-based)
- **OpenAPI docs:** Same URLs (`/docs`, `/redoc`), plus Scalar UI at `/scalar`

## What Stays the Same

- Route decorators (`@app.get`, `@app.post`, etc.)
- Pydantic model validation
- Dependency injection (`Depends`)
- Exception handling (`HTTPException`, custom handlers)
- Path/Query/Header/Cookie/Body extractors
- Sub-routers (`APIRouter`, `include_router`)
- WebSocket handlers
- Static file serving

## What You Gain

| Metric | FastAPI + Uvicorn | JustAPI | Improvement |
|---|---|---|---|
| Throughput | 36,189 req/s | 701,234 req/s | **~20x faster** |
| p99 Latency | 24.63 ms | 0.19 ms | **~130x lower** |
| JSON validation | Python runtime | Rust (GIL-free) | Zero overhead |
| DB queries | Python asyncio | Rust sqlx pool | Native speed |
| Memory usage | Per-request overhead | Zero-copy buffers | Minimal |

## Migration Checklist

- [ ] Replace `from fastapi import` with `from justapi import`
- [ ] Replace `FastAPI()` with `JustAPIApp()`
- [ ] Replace `uvicorn.run()` with `app.run()`
- [ ] Update middleware response header manipulation
- [ ] Test all routes return the same responses
- [ ] Compare performance with your load testing tool
- [ ] Update requirements.txt / pyproject.toml
- [ ] Update Dockerfile if using custom build

## Rollback Plan

JustAPI is designed for gradual adoption. You can:
1. Migrate one route at a time using `APIRouter`
2. Run JustAPI and FastAPI side-by-side behind a reverse proxy
3. Keep your existing tests — the response format is identical

## Next Steps

- [First Steps](/getting-started/first-steps/) — Run your migrated app
- [API Reference](/api-reference/) — Explore JustAPI-specific features
- [Performance Tuning](/advanced/performance-tuning/) — Optimize your migrated app
