---
title: Testing
description: Test JustAPI applications using the built-in JustAPITestClient for unit and integration tests.
keywords: [JustAPI, testing, test client, pytest, assertions]
---

## JustAPITestClient

JustAPI includes `JustAPITestClient` for testing without running a server. It processes requests in-process using the Rust runtime.

```python
from justapi import JustAPIApp, JustAPITestClient

app = JustAPIApp()

@app.get("/hello")
def hello():
    return {"message": "Hello!"}

client = JustAPITestClient(app)

def test_hello():
    response = client.get("/hello")
    assert response.status == 200
    assert response.json() == {"message": "Hello!"}
```

## Testing POST Endpoints

```python
def test_create_item():
    response = client.post("/items", body={"name": "Widget", "price": 9.99})
    assert response.status == 200
    assert response.json()["name"] == "Widget"
```

## Testing Error Responses

```python
def test_not_found():
    response = client.get("/nonexistent")
    assert response.status == 404

def test_validation_error():
    response = client.post("/items", body={"name": "x"})
    assert response.status == 422  # validation error
```

## Testing with Path Parameters

```python
@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {"user_id": user_id}

def test_get_user():
    response = client.get("/users/42")
    assert response.json() == {"user_id": 42}
```

## Testing with Query Parameters

```python
@app.get("/search")
def search(q: str, limit: int = 10):
    return {"q": q, "limit": limit}

def test_search_with_defaults():
    response = client.get("/search?q=foo")
    assert response.json() == {"q": "foo", "limit": 10}

def test_search_with_custom_limit():
    response = client.get("/search?q=foo&limit=5")
    assert response.json() == {"q": "foo", "limit": 5}
```

## Running Tests

```bash
# Run with pytest
pytest tests/ -v

# Run with uv
uv run pytest tests/ -v

# Single test file
pytest tests/test_app.py -v
```

## See Also

- [Testing Client API](/api-reference/testing-client/) — full client reference
- [Debugging](/tutorials/debugging/) — debugging techniques
- [Testing Guide](/contributing/testing-guide/) — writing and running tests
