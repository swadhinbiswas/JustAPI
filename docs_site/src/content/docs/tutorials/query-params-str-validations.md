---
title: Query Parameters and String Validations
description: Add string validation constraints to query parameters — min_length, max_length, regex patterns.
keywords: [JustAPI, query parameters, validation, min_length, max_length, regex, Query]
---

## Using Query() for Validation

Use `Query()` to add constraints to query parameters:

```python
from justapi import JustAPIApp, Query

app = JustAPIApp()

@app.get("/search")
def search(
    q: str = Query(..., min_length=1, max_length=100),
    tag: str = Query(None, max_length=50),
):
    return {"q": q, "tag": tag}
```

## Validation Options

| Parameter | Type | Description |
|-----------|------|-------------|
| `min_length` | `int` | Minimum string length |
| `max_length` | `int` | Maximum string length |
| `pattern` | `str` | Regex pattern the string must match |
| `title` | `str` | Title shown in the docs |
| `description` | `str` | Description shown in the docs |
| `deprecated` | `bool` | Mark as deprecated in the docs |
| `alias` | `str` | Alternative parameter name |

## Regex Patterns

```python
@app.get("/users")
def search_users(
    email: str = Query(None, pattern=r"^[\w.-]+@[\w.-]+\.\w+$"),
    phone: str = Query(None, pattern=r"^\+?\d{10,15}$"),
):
    return {"email": email, "phone": phone}
```

:::tip
Use raw strings (`r"..."`) for regex patterns to avoid escaping issues.
:::

## Aliasing

Use `alias` when the parameter name differs from the Python variable name:

```python
@app.get("/items")
def list_items(item_name: str = Query(..., alias="item-name")):
    return {"item_name": item_name}
```

The client sends `?item-name=foo` but your handler receives `item_name`.

## Deprecating Parameters

```python
@app.get("/search")
def search(q: str = Query(..., deprecated=True)):
    """Use the new search endpoint instead."""
    return {"q": q}
```

The parameter appears in the docs with a deprecation notice.

## See Also

- [Query Parameters](/tutorials/query-params/) — basic query params
- [Path Parameters & Numeric Validations](/tutorials/path-params-numeric-validations/) — numeric constraints
- [Body — Fields](/tutorials/body-fields/) — field-level validation
