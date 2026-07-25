---
title: Hello World in 2 Minutes
description: Build and run your first JustAPI application from scratch — the high-performance Rust-powered FastAPI alternative.
keywords: [JustAPI, hello world, quick start, first app, tutorial]
---

This tutorial walks through creating a complete JustAPI application, step by step. By the end, you'll have a running API server with multiple endpoints.

## Prerequisites

- Python 3.11+
- JustAPI installed (`pip install justapi`)

## 1. Create the App

Create a file named `main.py`:

```python
from justapi import JustAPIApp

app = JustAPIApp(title="Hello World", version="0.1.0")


@app.get("/")
def root(request):
    return {"message": "Hello, World!"}
```

## 2. Run It

```bash
# Using Python directly
python main.py

# Or with UV (faster startup)
uv run main.py
```

Expected output:

```
[Rust Tokio Runtime] Initialized with 12 worker threads
[Rust Tokio Runtime] Listening on http://127.0.0.1:8000
[Rust Tokio Runtime] OpenAPI docs at http://127.0.0.1:8000/docs
```

## 3. Test It

```bash
curl http://127.0.0.1:8000/
```

Expected response:

```json
{"message":"Hello, World!"}
```

## 4. Add More Endpoints

Extend your `main.py` with a parameterized route:

```python
@app.get("/hello/{name}")
def hello_name(request, name: str):
    return {"greeting": f"Hello, {name}!"}
```

Test it:

```bash
curl http://127.0.0.1:8000/hello/JustAPI
# Output: {"greeting":"Hello, JustAPI!"}
```

## 5. Add a POST Endpoint

```python
from pydantic import BaseModel

class Message(BaseModel):
    content: str
    author: str

@app.post("/messages/")
def create_message(request, msg: Message):
    return {
        "received": msg.content,
        "from": msg.author,
        "length": len(msg.content),
    }
```

Test with curl:

```bash
curl -X POST http://127.0.0.1:8000/messages/ \
  -H "Content-Type: application/json" \
  -d '{"content": "Hello from JustAPI!", "author": "Alice"}'
```

Expected response:

```json
{"received":"Hello from JustAPI!","from":"Alice","length":20}
```

## Complete Application

```python
from justapi import JustAPIApp
from pydantic import BaseModel

app = JustAPIApp(title="Hello World", version="0.1.0")


class Message(BaseModel):
    content: str
    author: str


@app.get("/")
def root(request):
    return {"message": "Hello, World!"}


@app.get("/hello/{name}")
def hello_name(request, name: str):
    return {"greeting": f"Hello, {name}!"}


@app.post("/messages/")
def create_message(request, msg: Message):
    return {
        "received": msg.content,
        "from": msg.author,
        "length": len(msg.content),
    }


if __name__ == "__main__":
    app.run("127.0.0.1:8000")
```

## Key Concepts

- **`JustAPIApp()`** — Creates your application and initializes the Rust runtime
- **`@app.get()` / `@app.post()`** — Route decorators that register handlers in the radix-trie router
- **Path parameters** — `{name}` in the path becomes a function parameter
- **Pydantic models** — Auto-validated request bodies with zero-copy parsing
- **`app.run()`** — Starts the built-in Rust HTTP server (no uvicorn needed)

## Next Steps

- [Path & Query Parameters](/tutorials/path-query-params/) — URL parameters, query strings, type validation
- [Request Body & Validation](/tutorials/request-body/) — Deep dive into schema validation
- [Dependency Injection](/tutorials/dependency-injection/) — Reusable components with Depends
