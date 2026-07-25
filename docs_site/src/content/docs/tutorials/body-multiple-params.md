---
title: Body — Multiple Parameters
description: Declare multiple body parameters and mix path, query, and body parameters in JustAPI.
keywords: [JustAPI, body parameters, multiple parameters, Body, mixed parameters]
---

## Mixing Parameter Sources

JustAPI automatically determines where each parameter comes from:

```python
from pydantic import BaseModel
from justapi import JustAPIApp

class Item(BaseModel):
    name: str
    price: float

app = JustAPIApp()

@app.put("/items/{item_id}")
def update_item(
    item_id: int,              # path parameter
    item: Item,                # body parameter (Pydantic model)
    q: str = None,             # query parameter
):
    result = {"item_id": item_id, **item.model_dump()}
    if q:
        result["q"] = q
    return result
```

## Multiple Body Parameters

For multiple Pydantic models in the body, use `Body(...)`:

```python
from pydantic import BaseModel
from justapi import JustAPIApp, Body

class User(BaseModel):
    name: str
    email: str

class Item(BaseModel):
    name: str
    price: float

app = JustAPIApp()

@app.post("/users/{user_id}/items/")
def create_user_item(
    user_id: int,              # path
    user: User,                # body
    item: Item = Body(...),    # explicit body
):
    return {"user": user.model_dump(), "item": item.model_dump()}
```

## Singular Values in Body

Use `Body(...)` for non-model body values:

```python
@app.post("/items/")
def create_item(
    name: str = Body(...),
    price: float = Body(...),
    description: str = Body(None),
):
    return {"name": name, "price": price, "description": description}
```

## Embedding Body

By default, the body is read from the root of the JSON. To nest it:

```python
@app.post("/items/")
def create_item(
    item: Item = Body(..., embed=True),
):
    # Client sends: {"item": {"name": "foo", "price": 42.0}}
    return item.model_dump()
```

## See Also

- [Request Body](/tutorials/request-body/) — basic body usage
- [Body — Fields](/tutorials/body-fields/) — field-level validation
- [Body — Nested Models](/tutorials/body-nested-models/) — nested Pydantic models
