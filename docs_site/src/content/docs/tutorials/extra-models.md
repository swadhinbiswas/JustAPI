---
title: Extra Models
description: Define multiple related Pydantic models for different API operations in JustAPI.
keywords: [JustAPI, Pydantic, multiple models, inheritance, schemas]
---

## Why Multiple Models?

Different operations need different data shapes:
- **Create** — input model with required fields
- **Read** — output model with ID and timestamps
- **Update** — optional fields for partial updates

```python
from pydantic import BaseModel
from typing import Optional

class ItemBase(BaseModel):
    name: str
    description: Optional[str] = None

class ItemCreate(ItemBase):
    price: float

class ItemRead(ItemBase):
    id: int
    in_stock: bool

class ItemUpdate(BaseModel):
    name: Optional[str] = None
    price: Optional[float] = None
```

## Using Different Models

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.post("/items", response_model=ItemRead, status_code=201)
def create_item(item: ItemCreate):
    # In production, save to database
    return {**item.model_dump(), "id": 1, "in_stock": True}

@app.get("/items/{item_id}", response_model=ItemRead)
def get_item(item_id: int):
    return {"id": item_id, "name": "Widget", "price": 9.99, "in_stock": True}

@app.patch("/items/{item_id}", response_model=ItemRead)
def update_item(item_id: int, item: ItemUpdate):
    return {**item.model_dump(), "id": item_id, "in_stock": True}
```

## See Also

- [Response Model](/tutorials/response-model/) — filtering output
- [Body — Nested Models](/tutorials/body-nested-models/) — nested Pydantic models
- [Request Body](/tutorials/request-body/) — Pydantic model usage
