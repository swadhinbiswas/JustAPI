---
title: Path Parameters
description: Declare and type-validate path parameters in JustAPI.
---

Path parameters allow you to extract variables directly from the HTTP request URL path.

## Basic Path Parameters

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/items/{item_id}")
def read_item(request, item_id: int):
    return {"item_id": item_id}
```

## Type Conversion & Automatic Validation

JustAPI automatically converts and validates path parameter types based on Python type hints (`int`, `float`, `str`, `bool`, `UUID`):

```python
from uuid import UUID

@app.get("/users/{user_id}")
def read_user(request, user_id: UUID):
    return {"user_id": str(user_id)}
```

If a client sends `/users/invalid-uuid`, JustAPI returns an automatic **HTTP 422 Unprocessable Entity** JSON error without invoking your Python code.
