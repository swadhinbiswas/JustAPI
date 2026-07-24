---
title: Path & Query Parameters
description: Declare and type-validate path and query parameters in JustAPI, the Rust-powered FastAPI alternative.
keywords: path parameters, query parameters, URL params, type validation, JustAPI routing, FastAPI alternative
---

Path and query parameters let you extract variables from the HTTP request URL. JustAPI handles type conversion and validation automatically using Python type hints.

## Path Parameters

Path parameters are declared as part of the URL path using `{param_name}` syntax:

```python
from justapi import JustAPIApp

app = JustAPIApp()


@app.get("/items/{item_id}")
def read_item(request, item_id: int):
    return {"item_id": item_id}
```

```bash
curl http://127.0.0.1:8000/items/42
# Output: {"item_id":42}
```

### Type Conversion

JustAPI automatically converts path parameters based on type hints. If conversion fails, it returns **422 Unprocessable Entity** without invoking your handler:

```python
from uuid import UUID


@app.get("/users/{user_id}")
def read_user(request, user_id: UUID):
    return {"user_id": str(user_id)}
```

```bash
curl http://127.0.0.1:8000/users/550e8400-e29b-41d4-a716-446655440000
# Output: {"user_id":"550e8400-e29b-41d4-a716-446655440000"}

curl http://127.0.0.1:8000/users/not-a-uuid
# Output: {"detail":"validation error"}
```

### Supported Types

| Type | Example | Valid Input | Invalid Input |
|---|---|---|---|
| `int` | `/items/{id}` | `42` | `abc` → 422 |
| `float` | `/price/{val}` | `19.99` | `abc` → 422 |
| `str` | `/hello/{name}` | any string | — |
| `bool` | `/flag/{val}` | `true`, `1`, `yes` | `maybe` → 422 |
| `UUID` | `/users/{uid}` | `550e8400-...` | `bad-uuid` → 422 |
| `datetime` | `/events/{ts}` | `2026-07-24T12:00:00Z` | `not-a-date` → 422 |

## Query Parameters

Query parameters are extracted from the URL query string (`?key=value&key2=value2`). They're declared as function parameters with default values:

```python
from typing import Optional


@app.get("/items/")
def list_items(
    request,
    skip: int = 0,
    limit: int = 10,
    q: Optional[str] = None,
):
    return {"skip": skip, "limit": limit, "q": q}
```

```bash
curl "http://127.0.0.1:8000/items/?skip=0&limit=5&q=search"
# Output: {"skip":0,"limit":5,"q":"search"}
```

### Optional vs Required

Parameters with default values are optional query parameters. Parameters without defaults are **required**:

```python
@app.get("/search/")
def search(request, query: str, page: int = 1):
    # `query` is required (no default)
    # `page` is optional (defaults to 1)
    return {"query": query, "page": page}
```

```bash
curl "http://127.0.0.1:8000/search/?query=rust"
# Output: {"query":"rust","page":1}

curl "http://127.0.0.1:8000/search/"
# Output: {"detail":"validation error"}  (missing required query)
```

## Combining Path and Query Parameters

```python
@app.get("/users/{user_id}/items/")
def list_user_items(
    request,
    user_id: int,
    skip: int = 0,
    limit: int = 10,
):
    return {"user_id": user_id, "skip": skip, "limit": limit}
```

```bash
curl "http://127.0.0.1:8000/users/42/items/?limit=3"
# Output: {"user_id":42,"skip":0,"limit":3}
```

## Using `Path()` and `Query()` for Explicit Control

For more control, use the `Path()` and `Query()` extractors:

```python
from justapi import Path, Query


@app.get("/items/{item_id}")
def read_item(
    request,
    item_id: int = Path(..., description="The item ID"),
    q: str = Query(None, max_length=50, description="Search query"),
):
    return {"item_id": item_id, "q": q}
```

### `Path()` Options

| Parameter | Type | Description |
|---|---|---|
| `default` | any | Default value (`...` means required) |
| `description` | str | Field description for OpenAPI docs |

### `Query()` Options

| Parameter | Type | Description |
|---|---|---|
| `default` | any | Default value |
| `description` | str | Field description for OpenAPI docs |
| `max_length` | int | Maximum string length |
| `min_length` | int | Minimum string length |
| `regex` | str | Regular expression pattern |

## How It Works Internally

1. JustAPI's radix-trie router (matchit) extracts path parameters during route matching — O(1) time
2. Type conversion runs in Rust before Python is called
3. Query parameters are parsed in Rust via `serde_urlencoded`
4. Invalid types return 422 without GIL acquisition

## Next Steps

- [Request Body & Validation](/tutorials/request-body/) — Validate JSON request bodies
- [Dependency Injection](/tutorials/dependency-injection/) — Extract parameters with Depends
- [Error Handling](/tutorials/error-handling/) — Custom error responses
