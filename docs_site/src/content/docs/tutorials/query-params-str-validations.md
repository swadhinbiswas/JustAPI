---
title: Query Parameters and String Validations
description: Add string validation constraints to query parameters — min_length, max_length, regex patterns, metadata.
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

`Query(...)` means the parameter is required (the `...` is Python's Ellipsis, meaning "no default"). `Query(None)` makes it optional.

## Required vs Optional

```python
# Required — no default value
@app.get("/search")
def search(q: str = Query(...)):
    return {"q": q}

# Optional — default value provided
@app.get("/search")
def search(q: str = Query(None)):
    return {"q": q}

# Optional with explicit default
@app.get("/search")
def search(q: str = Query("default")):
    return {"q": q}
```

:::note
`Query(...)` and `Query(min_length=1)` are both required. The `...` just means "no default value" — the constraint is separate.
:::

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
| `include_in_schema` | `bool` | Set `False` to hide from docs |

## Regex Patterns

```python
@app.get("/users")
def search_users(
    email: str = Query(None, pattern=r"^[\w.-]+@[\w.-]+\.\w+$"),
    phone: str = Query(None, pattern=r"^\+?\d{10,15}$"),
):
    return {"email": email, "phone": phone}
```

If the pattern doesn't match, the API returns a validation error:

```json
{
  "detail": [
    {
      "type": "string_pattern_mismatch",
      "loc": ["query", "email"],
      "msg": "String should match pattern '^[\\w.-]+@[\\w.-]+\\.\\w+$'",
      "input": "not-an-email"
    }
  ]
}
```

:::tip
Use raw strings (`r"..."`) for regex patterns to avoid escaping issues with backslashes.
:::

## Multiple Values for One Parameter

Repeat the parameter in the query string:

```python
@app.get("/filter")
def filter_items(tags: list[str] = Query(None)):
    return {"tags": tags}
```

`/filter?tags=a&tags=b&tags=c` returns `{"tags": ["a", "b", "c"]}`.

With validation:

```python
@app.get("/filter")
def filter_items(
    tags: list[str] = Query(None, min_length=1, max_length=10),
):
    return {"tags": tags}
```

Each tag must be between 1 and 10 characters.

## Metadata in Docs

Add title and description to show in the OpenAPI schema:

```python
@app.get("/search")
def search(
    q: str = Query(
        ...,
        title="Search Query",
        description="The search term to look for",
        min_length=1,
        max_length=200,
    ),
):
    return {"q": q}
```

The Swagger UI will show the title and description next to the parameter.

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
def search(
    q: str = Query(...),
    old_format: bool = Query(False, deprecated=True),
):
    return {"q": q, "old_format": old_format}
```

The `old_format` parameter appears in the docs with a deprecation notice.

## Hiding Parameters from Docs

```python
@app.get("/search")
def search(
    q: str = Query(...),
    internal_flag: str = Query(None, include_in_schema=False),
):
    return {"q": q}
```

`internal_flag` works but doesn't appear in the OpenAPI schema.

## Recap

- `Query(...)` makes a parameter required, `Query(None)` makes it optional
- Use `min_length`, `max_length`, `pattern` for string validation
- Use `title` and `description` for better docs
- Use `alias` for alternative parameter names
- Use `deprecated=True` to mark parameters as deprecated
- Use `list[str]` for repeating query parameters

## See Also

- [Query Parameters](/tutorials/query-params/) — basic query params
- [Path Parameters & Numeric Validations](/tutorials/path-params-numeric-validations/) — numeric constraints
- [Body — Fields](/tutorials/body-fields/) — field-level validation
