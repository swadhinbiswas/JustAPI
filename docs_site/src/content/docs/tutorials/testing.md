---
title: Testing
description: Test JustAPI applications using the built-in JustAPITestClient for unit and integration tests.
keywords: [JustAPI, testing, test client, pytest, assertions, CRUD]
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
    assert response.status_code == 200
    assert response.json() == {"message": "Hello!"}
```

## Testing POST Endpoints

```python
def test_create_item():
    response = client.post("/items", body={"name": "Widget", "price": 9.99})
    assert response.status_code == 200
    assert response.json()["name"] == "Widget"
```

## Testing with Headers

```python
@app.get("/protected")
def protected(x_token: str = Header(...)):
    return {"token": x_token}

def test_with_header():
    response = client.get("/protected", headers={"X-Token": "secret"})
    assert response.status_code == 200
    assert response.json() == {"token": "secret"}
```

## Testing Error Responses

```python
def test_not_found():
    response = client.get("/nonexistent")
    assert response.status_code == 404

def test_validation_error():
    response = client.post("/items", body={"name": "x"})
    assert response.status_code == 422
    assert "detail" in response.json()
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

## Full CRUD Example

Here's a complete example with a mini CRUD API and tests:

```python
from justapi import JustAPIApp, JustAPITestClient, HTTPException

app = JustAPIApp()
items_db = {}

@app.post("/items", status_code=201)
def create_item(item: dict):
    item_id = len(items_db) + 1
    items_db[item_id] = item
    return {"id": item_id, **item}

@app.get("/items/{item_id}")
def get_item(item_id: int):
    if item_id not in items_db:
        raise HTTPException(status_code=404, detail="Not found")
    return {"id": item_id, **items_db[item_id]}

@app.get("/items")
def list_items():
    return [{"id": k, **v} for k, v in items_db.items()]

@app.delete("/items/{item_id}", status_code=204)
def delete_item(item_id: int):
    if item_id not in items_db:
        raise HTTPException(status_code=404, detail="Not found")
    del items_db[item_id]
```

Tests:

```python
client = JustAPITestClient(app)

def test_create_and_get():
    # Create
    response = client.post("/items", body={"name": "Widget", "price": 9.99})
    assert response.status_code == 201
    item_id = response.json()["id"]

    # Get
    response = client.get(f"/items/{item_id}")
    assert response.status_code == 200
    assert response.json()["name"] == "Widget"

def test_list_items():
    response = client.get("/items")
    assert response.status_code == 200
    assert isinstance(response.json(), list)

def test_get_nonexistent():
    response = client.get("/items/99999")
    assert response.status_code == 404

def test_delete_item():
    # Create first
    response = client.post("/items", body={"name": "To Delete"})
    item_id = response.json()["id"]

    # Delete
    response = client.delete(f"/items/{item_id}")
    assert response.status_code == 204

    # Confirm deleted
    response = client.get(f"/items/{item_id}")
    assert response.status_code == 404
```

## Running Tests

```bash
# Run with pytest
pytest tests/ -v

# Run with uv
uv run pytest tests/ -v

# Single test file
pytest tests/test_app.py -v

# Run with coverage
pytest --cov=myapp tests/
```

## Test File Layout

```
tests/
├── test_items.py
├── test_users.py
├── test_auth.py
└── conftest.py
```

## See Also

- [Testing Client API](/api-reference/testing-client/) — full client reference
- [Debugging](/tutorials/debugging/) — debugging techniques
- [Testing a Database](/how-to/testing-database/) — database testing patterns
