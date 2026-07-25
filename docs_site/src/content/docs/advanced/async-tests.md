---
title: Async Tests
description: Write async test functions with httpx.AsyncClient for JustAPI applications.
keywords: [JustAPI, async tests, httpx, AsyncClient, pytest-asyncio]
---

## Async Test Client

Use `httpx.AsyncClient` for async testing:

```python
import pytest
import httpx
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/hello")
async def hello():
    return {"message": "Hello!"}

@pytest.mark.anyio
async def test_hello():
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/hello")
        assert response.status_code == 200
        assert response.json() == {"message": "Hello!"}
```

## Testing with Dependencies

```python
def override_get_db():
    return TestDB()

app.dependency_overrides[get_db] = override_get_db

@pytest.mark.anyio
async def test_with_override():
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/items/")
        assert response.status_code == 200
```

## Setup and Teardown

```python
@pytest.fixture
async def client():
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://test") as c:
        yield c

@pytest.mark.anyio
async def test_example(client):
    response = await client.get("/hello")
    assert response.status_code == 200
```

## See Also

- [Testing](/tutorials/testing/) — sync testing with JustAPITestClient
- [Testing with Overrides](/advanced/async-tests/) — dependency overrides
- [Debugging](/tutorials/debugging/) — debugging techniques
