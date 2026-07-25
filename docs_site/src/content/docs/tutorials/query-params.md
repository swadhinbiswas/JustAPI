---
title: Query Parameters
description: Declare and validate query parameters in JustAPI.
keywords: [JustAPI, query parameters, URL parameters, FastAPI alternative]
---

Query parameters are the key-value pairs after `?` in a URL. JustAPI extracts them automatically from function parameters.

## Basic Query Parameters

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/items/")
def list_items(skip: int = 0, limit: int = 10):
    return {"skip": skip, "limit": limit}
```

- `/items/` returns `{"skip": 0, "limit": 10}`
- `/items/?skip=20&limit=50` returns `{"skip": 20, "limit": 50}`

Any parameter that has a default value is treated as a query parameter.

## Optional vs Required

A query parameter is **required** if it has no default value:

```python
@app.get("/search")
def search(q: str):  # required — no default value
    return {"query": q}
```

`/search` without `?q=...` returns a validation error.

A query parameter is **optional** if it has a default value:

```python
@app.get("/search")
def search(q: str = None):  # optional — defaults to None
    return {"query": q}
```

`/search` without `?q=...` works and returns `{"query": null}`.

## Default Values

```python
@app.get("/products")
def list_products(
    category: str = "all",
    min_price: float = 0.0,
    max_price: float = 1000.0,
    sort: str = "name",
    order: str = "asc",
):
    return {
        "category": category,
        "price_range": [min_price, max_price],
        "sort": sort,
        "order": order,
    }
```

All parameters are optional with their defaults. Clients only need to override the ones they care about.

## Type Conversion

JustAPI converts query parameters from strings to the declared Python type:

```python
@app.get("/items/{item_id}")
def get_item(item_id: int, verbose: bool = False):
    return {"item_id": item_id, "verbose": verbose}
```

- `/items/5?verbose=true` — `verbose` is `True`
- `/items/5?verbose=1` — `verbose` is `True`
- `/items/5?verbose=false` — `verbose` is `False`

## Multiple Values for One Parameter

Query parameters can repeat:

```python
@app.get("/filter")
def filter_items(tags: list[str] = None):
    return {"tags": tags}
```

`/filter?tags=a&tags=b&tags=c` returns `{"tags": ["a", "b", "c"]}`.

## Recap

- Parameters with default values are query parameters.
- Parameters without default values are required.
- JustAPI converts types automatically (`str` to `int`, `"true"` to `True`, etc.).
- Use `list[T]` for repeating query parameters.

## See Also

- [Query Parameters & String Validations](/tutorials/query-params-str-validations/) — add `min_length`, `max_length`, `pattern` constraints
- [Path Parameters](/tutorials/path-params/) — URL path parameters
- [Request Body](/tutorials/request-body/) — JSON request bodies
