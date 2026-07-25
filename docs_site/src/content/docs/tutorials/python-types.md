---
title: Python Types Intro
description: Introduction to Python type hints and how JustAPI uses them for automatic validation, serialization, and OpenAPI documentation.
keywords: [JustAPI, Python types, type hints, Pydantic, validation, FastAPI alternative]
---

JustAPI uses standard Python type hints to power request validation, serialization, and auto-generated documentation. This page explains the types JustAPI supports and how they map to API contracts.

## Basic Types

JustAPI accepts all standard Python built-in types as function parameters and Pydantic model fields:

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {"user_id": user_id}

@app.post("/items")
def create_item(name: str, price: float, in_stock: bool = True):
    return {"name": name, "price": price, "in_stock": in_stock}
```

JustAPI automatically converts the path parameter `user_id` from a string to an `int`. If the value is not a valid integer, the API returns a clear validation error instead of crashing.

| Type | Example | Validation |
|------|---------|------------|
| `int` | `42` | Rejects non-numeric strings |
| `float` | `3.14` | Rejects non-numeric strings |
| `str` | `"hello"` | Default string |
| `bool` | `True` / `False` | Accepts `true` / `false` / `1` / `0` |
| `bytes` | `b"data"` | Binary data |

## Collections

```python
from justapi import JustAPIApp
from pydantic import BaseModel

app = JustAPIApp()

@app.get("/filter")
def filter_items(tags: list[str] = None, limit: int = 10):
    return {"tags": tags, "limit": limit}
```

:::tip
Query parameters like `tags=a&tags=b&tags=c` are automatically parsed into `["a", "b", "c"]`.
:::

| Type | Use Case |
|------|----------|
| `list[str]` | Multiple query values or JSON array |
| `dict[str, str]` | JSON object with string keys and values |
| `set[str]` | Unique values |
| `tuple[int, str]` | Fixed-length JSON arrays |

## Optional vs Required

A parameter is **required** if it has no default value. It is **optional** if it has a default value or uses `Optional` / `None`:

```python
from typing import Optional
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/search")
def search(q: str, limit: int = 10, tag: Optional[str] = None):
    return {"q": q, "limit": limit, "tag": tag}
```

- `q: str` — required, no default
- `limit: int = 10` — optional, defaults to `10`
- `tag: Optional[str] = None` — optional, defaults to `None`

## Pydantic Models

The most powerful way to define structured data is with Pydantic `BaseModel`:

```python
from justapi import JustAPIApp
from pydantic import BaseModel
from typing import Optional

app = JustAPIApp()

class Item(BaseModel):
    name: str
    price: float
    description: Optional[str] = None
    tags: list[str] = []

@app.post("/items")
def create_item(item: Item):
    return item.model_dump()
```

JustAPI reads the request body as JSON, validates it against the model, and passes a typed `Item` object to your handler. Invalid data produces a clear error response with the exact field and issue.

## Advanced Types

JustAPI supports all Pydantic and Python standard library types:

```python
from datetime import datetime, date
from uuid import UUID
from decimal import Decimal
from enum import Enum
from typing import Literal

class Color(str, Enum):
    red = "red"
    green = "green"
    blue = "blue"

@app.post("/orders")
def create_order(
    order_id: UUID,
    created_at: datetime,
    color: Color,
    amount: Decimal,
):
    return {"order_id": str(order_id), "color": color.value}
```

| Type | Wire Format | Example |
|------|-------------|---------|
| `datetime` | ISO 8601 string | `"2026-07-25T10:30:00"` |
| `date` | ISO 8601 date | `"2026-07-25"` |
| `UUID` | String | `"550e8400-e29b-41d4-a716-446655440000"` |
| `Decimal` | String or number | `"3.14"` |
| `Enum` | String (value) | `"red"` |
| `Literal["a", "b"]` | One of the literals | `"a"` |

## Recap

- JustAPI uses standard Python type hints for validation, serialization, and documentation.
- Built-in types (`int`, `str`, `float`, `bool`) are auto-converted from strings in path/query params.
- `Optional[T]` or `T = None` makes a parameter optional.
- Pydantic `BaseModel` provides structured request body validation with nested models.
- Advanced types (`datetime`, `UUID`, `Decimal`, `Enum`) are fully supported.

## See Also

- [First Steps](/tutorials/hello-world/) — your first JustAPI app
- [Path Parameters](/tutorials/path-params/) — working with path parameters
- [Query Parameters](/tutorials/query-params/) — working with query parameters
- [Request Body](/tutorials/request-body/) — request body with Pydantic
