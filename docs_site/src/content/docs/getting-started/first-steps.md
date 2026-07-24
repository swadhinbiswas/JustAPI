---
title: First Steps with JustAPI
description: Create your first API endpoint in under 2 minutes.
---

This guide walks you through building and running your first JustAPI application from scratch.

## 1. Create a `main.py` File

```python
from justapi import JustAPIApp

app = JustAPIApp(title="My First App", version="1.0.0")


@app.get("/")
def read_root(request):
    return {"status": "ok", "message": "Welcome to JustAPI!"}


@app.get("/users/{user_id}")
def read_user(request, user_id: int):
    return {"user_id": user_id, "active": True}
```

### What's happening here?

- `JustAPIApp()` initializes the Rust runtime — no separate server process needed
- `@app.get("/")` registers a route in the Rust radix-trie router (matchit)
- The `request` parameter is a zero-copy view into the HTTP request (no serialization overhead)
- Return a dict — JustAPI serializes it to JSON via `serde_json` in Rust

## 2. Run the Application

Run directly using Python:

```bash
python main.py
```

You should see output similar to:

```
[Rust Tokio Runtime] Listening on 127.0.0.1:8000
[Rust Tokio Runtime] Worker threads: 12
[Rust Tokio Runtime] OpenAPI docs at http://127.0.0.1:8000/docs
```

Or run using the hot-reloading dev server:

```bash
justapi serve --reload
```

## 3. Test the API

Open a new terminal and test with `curl`:

```bash
curl http://127.0.0.1:8000/
# Output: {"status":"ok","message":"Welcome to JustAPI!"}

curl http://127.0.0.1:8000/users/42
# Output: {"user_id":42,"active":true}

curl http://127.0.0.1:8000/users/abc
# Output: {"detail":"validation error"}
```

Notice the third request returns a **422 validation error** automatically — the Rust router rejected the non-integer `user_id` without ever calling Python.

## 4. Explore Interactive Documentation

JustAPI auto-generates OpenAPI 3.1 documentation. Open your browser to:

| URL | Description |
|---|---|
| `http://127.0.0.1:8000/docs` | **Swagger UI** — Interactive API explorer |
| `http://127.0.0.1:8000/redoc` | **ReDoc** — Alternative documentation viewer |
| `http://127.0.0.1:8000/scalar` | **Scalar UI** — Modern API client |
| `http://127.0.0.1:8000/openapi.json` | Raw OpenAPI 3.1 spec |

## 5. Add a POST Endpoint

```python
from pydantic import BaseModel

class Item(BaseModel):
    name: str
    price: float
    is_offer: bool = None

@app.post("/items/")
def create_item(request, item: Item):
    return {"name": item.name, "price": item.price}
```

## What JustAPI Does Differently

| Aspect | Other Frameworks | JustAPI |
|---|---|---|
| **Route matching** | Python dict iteration | Rust radix trie (O(1)) |
| **JSON parsing** | Python `json.loads()` | Rust `serde_json` (GIL released) |
| **Validation** | Python runtime | Rust `jsonschema-rs` (GIL released) |
| **Response body** | Python bytes | Rust zero-copy buffer |
| **Concurrency** | GIL-bound async | True parallel via `py.allow_threads` |

## Next Steps

- [Path & Query Parameters](/tutorials/path-query-params/) — Deep dive into routing
- [Request Body & Validation](/tutorials/request-body/) — Schema validation with Pydantic
- [CLI Project Scaffolder](/getting-started/cli-scaffolder/) — Generate complete project templates
