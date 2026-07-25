---
title: Path Parameters and Numeric Validations
description: Add numeric validation constraints to path parameters — ge, le, gt, lt, multiple_of.
keywords: [JustAPI, path parameters, numeric validation, ge, le, gt, lt, Path]
---

## Using Path() for Validation

Use `Path()` to add numeric constraints:

```python
from justapi import JustAPIApp, Path

app = JustAPIApp()

@app.get("/items/{item_id}")
def get_item(item_id: int = Path(..., ge=1)):
    return {"item_id": item_id}
```

The `ge=1` constraint means `item_id` must be >= 1. Sending `/items/0` returns a validation error.

## Validation Options

| Parameter | Type | Description |
|-----------|------|-------------|
| `ge` | `int/float` | Greater than or equal |
| `le` | `int/float` | Less than or equal |
| `gt` | `int/float` | Greater than |
| `lt` | `int/float` | Less than |
| `multiple_of` | `int/float` | Must be a multiple of this value |

## Examples

```python
@app.get("/users/{user_id}")
def get_user(user_id: int = Path(..., gt=0)):
    """user_id must be positive"""
    return {"user_id": user_id}

@app.get("/products/{price}")
def get_product(price: float = Path(..., ge=0.01, le=9999.99)):
    """price between 0.01 and 9999.99"""
    return {"price": price}

@app.get("/grid/{position}")
def get_position(position: int = Path(..., multiple_of=10)):
    """position must be a multiple of 10"""
    return {"position": position}
```

## Combining with Type Conversion

```python
@app.get("/items/{item_id}")
def get_item(
    item_id: int = Path(..., ge=1, description="Item ID, must be >= 1"),
):
    return {"item_id": item_id}
```

The docs will show the constraint alongside the type.

## See Also

- [Path Parameters](/tutorials/path-params/) — basic path params
- [Query Parameters & String Validations](/tutorials/query-params-str-validations/) — string constraints
- [Schema & Validation](/api-reference/schema-validation/) — validation reference
