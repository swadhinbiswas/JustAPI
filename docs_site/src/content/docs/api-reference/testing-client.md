---
title: Testing Client
description: Test JustAPI applications with JustAPITestClient — in-process HTTP testing.
keywords: [JustAPI, test client, JustAPITestClient, testing, HTTP testing]
---

## Basic Usage

```python
from justapi import JustAPIApp, JustAPITestClient

app = JustAPIApp()

@app.get("/hello")
def hello():
    return {"message": "Hello!"}

client = JustAPITestClient(app)

def test_hello():
    response = client.get("/hello")
    assert response["status"] == 200
    assert response["body"] == b'{"message":"Hello!"}'
```

## Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `get(path)` | `GET path` | Send GET request |
| `post(path, body)` | `POST path` with JSON body | Send POST request |
| `post_with(path, body, headers)` | `POST path` with headers | POST with custom headers |
| `put(path, body)` | `PUT path` with JSON body | Send PUT request |
| `patch(path, body)` | `PATCH path` with JSON body | Send PATCH request |
| `delete(path)` | `DELETE path` | Send DELETE request |

## Response Format

All methods return a dict:

```python
{
    "status": 200,        # HTTP status code
    "headers": {...},     # Response headers
    "body": b"..."        # Raw body bytes
}
```

## POST with Headers

```python
def test_with_auth():
    response = client.post_with(
        "/items",
        {"name": "Widget"},
        headers={"Authorization": "Bearer my-token"},
    )
    assert response["status"] == 201
```

## With Database

```python
client = JustAPITestClient(app, database="sqlite:///test.db")
```

## See Also

- [Testing](/tutorials/testing/) — basic testing guide
- [Testing Utilities](/tutorials/testing-utilities/) — AsyncTestClient, Snapshot, assertions
- [Testing a Database](/how-to/testing-database/) — database test patterns
