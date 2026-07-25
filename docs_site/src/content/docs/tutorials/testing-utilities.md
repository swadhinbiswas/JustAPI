---
title: Testing Utilities
description: Advanced testing tools — AsyncTestClient, ManagedDb, Snapshot testing, and assertion helpers.
keywords: [JustAPI, testing, AsyncTestClient, ManagedDb, Snapshot, assertions]
---

## AsyncTestClient

For async tests with pytest-asyncio:

```python
import pytest
from justapi.testing import AsyncTestClient
from myapp import app

@pytest.fixture
async def client():
    async with AsyncTestClient(app) as c:
        yield c

@pytest.mark.anyio
async def test_hello(client):
    response = await client.get("/hello")
    assert response["status"] == 200
```

## ManagedDb

Test database with automatic cleanup:

```python
from justapi.testing import ManagedDb

db = ManagedDb("sqlite:///test.db", migrations_dir="migrations/")

async def setup():
    await db.run_migrations()

async def teardown():
    await db.run_sql("DELETE FROM users")
```

## db_client

Create a test client with a managed database:

```python
from justapi.testing import db_client

async def test_with_db():
    async with db_client(app, "sqlite:///test.db") as client:
        response = await client.get("/users")
        assert response["status"] == 200
```

## Snapshot Testing

Assert responses match saved snapshots:

```python
from justapi.testing import Snapshot

snapshot = Snapshot()

def test_api_response():
    response = client.get("/items")
    snapshot.assert_response(response, "items_list")
    # First run: saves snapshot
    # Subsequent runs: compares against saved snapshot
```

Update snapshots by setting `SNAPSHOT_UPDATE=1`.

## Assertion Helpers

```python
from justapi.testing import assert_ok, assert_status, assert_json, assert_header

def test_helpers():
    response = client.get("/hello")

    assert_ok(response)                          # status == 200
    assert_status(response, 201)                 # status == 201
    assert_json(response, {"message": "Hello"})  # JSON body matches
    assert_header(response, "X-Custom", "value") # header exists with value
```

## See Also

- [Testing](/tutorials/testing/) — basic testing with JustAPITestClient
- [Testing a Database](/how-to/testing-database/) — database testing patterns
