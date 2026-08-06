---
title: Async Tests
description: Write async tests with AsyncTestClient for JustAPI applications.
keywords: [JustAPI, async tests, pytest-asyncio, AsyncTestClient, TestClient]
---

JustAPI ships its own in-process test client — no external ASGI transport needed.
Use `AsyncTestClient` for async tests and `JustAPITestClient` for sync tests.

## Async Test Client

```python
import pytest
from justapi import JustAPIApp
from justapi.testing import AsyncTestClient

app = JustAPIApp()

@app.get("/hello")
async def hello():
    return {"message": "Hello!"}

@pytest.mark.asyncio
async def test_hello():
    async with AsyncTestClient(app) as client:
        resp = await client.get("/hello")
        assert resp.status == 200
        assert resp.json() == {"message": "Hello!"}
```

The client runs the full Rust pipeline (routing, middleware, native fast path)
in-process, so tests exercise the same code paths as production.

## Testing Database-Backed Routes

```python
@pytest.mark.asyncio
async def test_db():
    async with AsyncTestClient(app, database="sqlite::memory:") as client:
        resp = await client.post("/items", b'{"name":"widget"}')
        assert resp.status == 200
```

## Fixtures

```python
import pytest
from justapi.testing import AsyncTestClient

@pytest.fixture
async def client():
    async with AsyncTestClient(app) as c:
        yield c

@pytest.mark.asyncio
async def test_example(client):
    resp = await client.get("/hello")
    assert resp.status == 200
```

## Supported Methods

`get`, `post`, `put`, `patch`, `delete` — each returns a response object with
`.status` and `.json()` (mirroring the sync `JustAPITestClient`).

## See Also

- [Testing](/tutorials/testing/) — sync testing with `JustAPITestClient`
- [Debugging](/tutorials/debugging/) — debugging techniques