---
title: Path Parameters
description: Declare and validate path parameters in JustAPI using Python type hints.
keywords: [JustAPI, path parameters, URL parameters, validation, FastAPI alternative]
---

Path parameters (also called URL variables) are values embedded in the URL path. JustAPI extracts them automatically based on your function signature.

## Basic Path Parameters

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {"user_id": user_id}

@app.get("/items/{item_id}")
def get_item(item_id: str):
    return {"item_id": item_id}
```

The `{user_id}` and `{item_id}` in the path are matched to the function parameters. JustAPI validates the type automatically:

- `/users/42` works — `user_id` is `42` (int)
- `/users/foo` returns a validation error — `"foo"` is not a valid integer

## Data Conversion

JustAPI converts path parameters from strings to the declared Python type:

```python
@app.get("/items/{item_id}")
def get_item(item_id: int):
    # item_id is an int, not a string
    return {"item_id": item_id, "double": item_id * 2}
```

- `/items/3` returns `{"item_id": 3, "double": 6}`
- `/items/3.14` returns a validation error — `"3.14"` is not a valid `int`

## Supported Types

| Type | Example URL | Value |
|------|-------------|-------|
| `int` | `/items/42` | `42` |
| `float` | `/items/3.14` | `3.14` |
| `str` | `/items/foo` | `"foo"` |
| `bool` | `/items/true` | `True` |
| `UUID` | `/items/550e8400-...` | `UUID(...)` |
| `Enum` | `/models/alexnet` | `ModelName.alexnet` |

## Predefined Values with Enum

Restrict a path parameter to a fixed set of values using Python `Enum`:

```python
from enum import Enum
from justapi import JustAPIApp

class ModelName(str, Enum):
    alexnet = "alexnet"
    resnet = "resnet"
    lenet = "lenet"

app = JustAPIApp()

@app.get("/models/{model_name}")
def get_model(model_name: ModelName):
    if model_name is ModelName.alexnet:
        return {"model_name": model_name, "message": "Deep Learning FTW!"}
    return {"model_name": model_name, "message": "Have some residuals"}
```

The OpenAPI docs will show the available values in a dropdown.

## Path Parameters Containing Paths

For parameters that must match a full path (like file paths), use the `:path` converter:

```python
@app.get("/files/{file_path:path}")
def read_file(file_path: str):
    return {"file_path": file_path}
```

This matches URLs like `/files/home/user/file.txt`.

:::note
OpenAPI does not support the `:path` converter. The docs will not document that the parameter should contain a path, but the route will work correctly.
:::

## Order Matters

When defining routes, put fixed paths before parameterized paths:

```python
@app.get("/users/me")
def read_user_me():
    return {"user_id": "the current user"}

@app.get("/users/{user_id}")
def read_user(user_id: str):
    return {"user_id": user_id}
```

If `/users/{user_id}` were declared first, it would also match `/users/me`.

## Recap

- Use `{parameter_name}` in the path to declare a path parameter.
- Declare the type in your function signature — JustAPI validates and converts it.
- Use `Enum` for predefined values.
- Use `:path` for parameters containing full paths.
- Put fixed routes before parameterized routes.

## See Also

- [Path Parameters & Numeric Validations](/tutorials/path-params-numeric-validations/) — add `ge`, `le`, `gt`, `lt` constraints
- [Query Parameters](/tutorials/query-params/) — parameters in the query string
- [Request Body](/tutorials/request-body/) — JSON request bodies
