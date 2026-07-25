---
title: Response Model — Return Type
description: Control the response schema and filter output fields in JustAPI with response_model.
keywords: [JustAPI, response model, return type, filtering, schema]
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

The `password` field is automatically excluded from the response because it's not declared in the response model.

## Filtering Response Fields

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

## See Also

- [Extra Models](/tutorials/extra-models/) — multiple related models
- [Response Status Code](/tutorials/response-status-code/) — custom status codes
- [Custom Response Classes](/advanced/custom-response/) — HTML, stream, file responses
