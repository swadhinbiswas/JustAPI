---
title: Testing a Database
description: Use in-memory SQLite and transaction rollback for fast database tests in JustAPI.
keywords: [JustAPI, testing, database, SQLite, in-memory, transactions]
---

## In-Memory SQLite

Use in-memory SQLite for fast, isolated tests:

```python
import sqlite3
from justapi import JustAPIApp, JustAPITestClient

def get_test_db():
    conn = sqlite3.connect(":memory:")
    conn.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
    return conn

app = JustAPIApp()

@app.get("/items")
def list_items():
    db = get_test_db()
    cursor = db.execute("SELECT * FROM items")
    return {"items": cursor.fetchall()}

def test_list_items_empty():
    client = JustAPITestClient(app)
    response = client.get("/items")
    assert response.status_code == 200
    assert response.json() == {"items": []}
```

## Transaction Rollback

Wrap each test in a transaction and rollback:

```python
import pytest

@pytest.fixture
def db():
    conn = sqlite3.connect(":memory:")
    conn.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
    yield conn
    conn.rollback()  # undo all changes
    conn.close()

def test_create_item(db):
    db.execute("INSERT INTO items (name) VALUES (?)", ("Widget",))
    cursor = db.execute("SELECT * FROM items")
    assert cursor.fetchone() is not None
```

## Dependency Override for Testing

```python
from justapi import Depends

def get_db():
    return production_db

def override_get_db():
    return sqlite3.connect(":memory:")

app = JustAPIApp()
app.dependency_overrides[get_db] = override_get_db

def test_handler():
    client = JustAPITestClient(app)
    response = client.get("/items/")
    assert response.status_code == 200
```

## See Also

- [Testing](/tutorials/testing/) — basic testing
- [SQL Databases](/tutorials/database-integration/) — database setup
- [Advanced Dependencies](/advanced/advanced-dependencies/) — dependency overrides
