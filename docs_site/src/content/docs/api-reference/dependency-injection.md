---
title: Dependency Injection API
description: "API reference for dependency injection in JustAPI, the FastAPI alternative — Depends, Path, Query, Header, Cookie, Body, File, and Form extractors."
keywords: [dependency injection, fastapi alternative, justapi, depends, path, query, header, request extraction]
---

## `Depends()`

Declare a dependency that is resolved before the handler executes:

```python
from justapi import Depends

def common_params(q: str | None = None, skip: int = 0):
    return {"q": q, "skip": skip}

@app.get("/items")
def list_items(request, params: dict = Depends(common_params)):
    ...
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `dependency` | callable | required | The dependency function or class |
| `use_cache` | `bool` | `True` | Cache result for reuse across nested dependencies |

## Parameter Extractors

These functions extract specific parts of the request:

### `Path()`

```python
from justapi import Path

@app.get("/items/{item_id}")
def read_item(
    request,
    item_id: int = Path(..., description="The item ID"),
):
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `default` | any | `...` | `...` means required |
| `description` | `str` | `None` | OpenAPI description |

### `Query()`

```python
from justapi import Query

@app.get("/items")
def list_items(
    request,
    q: str = Query(None, max_length=50, description="Search query"),
):
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `default` | any | `...` | Default value (`...` = required) |
| `description` | `str` | `None` | OpenAPI description |
| `max_length` | `int` | `None` | Max string length |
| `min_length` | `int` | `None` | Min string length |
| `regex` | `str` | `None` | Regex pattern |

### `Header()`

```python
from justapi import Header

@app.get("/protected")
def protected(
    request,
    authorization: str = Header(..., description="Bearer token"),
):
```

### `Cookie()`

```python
from justapi import Cookie

@app.get("/")
def home(
    request,
    session_id: str = Cookie(None, description="Session cookie"),
):
```

### `Body()`

```python
from justapi import Body

@app.post("/items")
def create_item(
    request,
    data: dict = Body(..., description="Request body"),
):
```

### `File()`

```python
from justapi import File, UploadFile

@app.post("/upload")
def upload(
    request,
    file: UploadFile = File(..., description="File to upload"),
):
```

### `Form()`

```python
from justapi import Form

@app.post("/submit")
def submit(
    request,
    name: str = Form(..., description="User name"),
):
```

## `Session` (Agent)

```python
from justapi import Session

@app.get("/agent/state")
def agent_state(request, session: Session):
    return {"session_id": session.id, **session.get()}
```

| Attribute/Method | Description |
|---|---|
| `session.id` | Unique session identifier |
| `session.get()` | Get all session data as dict |
| `session.update(**kwargs)` | Update session data |

## See Also

- [Dependency Injection Tutorial](/tutorials/dependency-injection/) — Usage guide
- [Session API](/api-reference/session/) — Agent session reference
- [JustAPIApp](/api-reference/justapiapp/) — App configuration
