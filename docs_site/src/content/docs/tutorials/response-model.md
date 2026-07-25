---
title: Response Model — Return Type
description: Control the response schema and filter output fields in JustAPI with response_model.
keywords: [JustAPI, response model, return type, filtering, schema, exclude_unset]
---

## Basic Response Model

Declare the response type with `response_model`:

```python
from pydantic import BaseModel
from justapi import JustAPIApp

class User(BaseModel):
    id: int
    name: str
    email: str
    password: str

app = JustAPIApp()

@app.get("/users/{user_id}", response_model=User)
def get_user(user_id: int):
    return {"id": user_id, "name": "Alice", "email": "alice@example.com", "password": "secret"}
```

The `password` field is automatically excluded from the response because it's not in the response model. This is a security feature — you don't accidentally leak sensitive data.

## Filtering Response Fields

Use a separate model for output:

```python
class UserOut(BaseModel):
    id: int
    name: str

@app.get("/users/{user_id}", response_model=UserOut)
def get_user(user_id: int):
    return {"id": user_id, "name": "Alice", "email": "alice@example.com"}
```

Only `id` and `name` appear in the response.

## Exclude Fields

```python
@app.get(
    "/users/{user_id}",
    response_model=User,
    response_model_exclude={"password"},
)
def get_user(user_id: int):
    return {"id": user_id, "name": "Alice", "email": "alice@example.com", "password": "secret"}
```

## Include Fields

```python
@app.get(
    "/users/{user_id}",
    response_model=UserOut,
    response_model_include={"id", "name"},
)
def get_user(user_id: int):
    return {"id": user_id, "name": "Alice"}
```

## response_model_exclude_unset

This is useful when working with databases that return optional fields. It excludes fields that were not explicitly set:

```python
from pydantic import BaseModel
from typing import Optional

class User(BaseModel):
    id: int
    name: str
    email: Optional[str] = None
    bio: Optional[str] = None

app = JustAPIApp()

@app.get("/users/{user_id}", response_model=User, response_model_exclude_unset=True)
def get_user(user_id: int):
    # Database returns only set fields
    return {"id": 1, "name": "Alice"}
    # email and bio are excluded because they weren't set
```

Without `exclude_unset`, the response would include `"email": null, "bio": null`. With it, only `id` and `name` appear.

## response_model_exclude_defaults

Excludes fields that have their default value:

```python
@app.get("/users/{user_id}", response_model=User, response_model_exclude_defaults=True)
def get_user(user_id: int):
    return {"id": 1, "name": "Alice", "email": None, "bio": None}
    # email and bio are excluded because None is the default
```

## response_model_exclude_none

Excludes fields that are `None`:

```python
@app.get("/users/{user_id}", response_model=User, response_model_exclude_none=True)
def get_user(user_id: int):
    return {"id": 1, "name": "Alice", "email": None, "bio": None}
    # email and bio are excluded because they're None
```

## Disable Response Model

```python
@app.get("/raw", response_model=None)
def raw():
    return {"anything": "goes"}
```

With `response_model=None`, no response filtering is applied.

## Multiple Response Models

```python
from typing import Union

@app.get(
    "/users/{user_id}",
    response_model=Union[UserOut, dict],
    responses={404: {"model": dict}},
)
def get_user(user_id: int):
    if user_id == 0:
        return {"error": "not found"}
    return {"id": user_id, "name": "Alice"}
```

## Return Type Annotation

You can also use the Python return type annotation:

```python
@app.get("/users/{user_id}")
def get_user(user_id: int) -> UserOut:
    return {"id": user_id, "name": "Alice"}
```

JustAPI uses the return type as the response model automatically.

## See Also

- [Extra Models](/tutorials/extra-models/) — multiple related models
- [Response Status Code](/tutorials/response-status-code/) — custom status codes
- [Custom Response Classes](/advanced/custom-response/) — HTML, stream, file responses
