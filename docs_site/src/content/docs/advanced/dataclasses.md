---
title: Using Dataclasses
description: Use Python dataclasses as request models and response models in JustAPI.
keywords: [JustAPI, dataclasses, request model, Python dataclass]
---

## Dataclasses as Request Models

JustAPI supports Python `dataclasses` as an alternative to Pydantic:

```python
from dataclasses import dataclass
from typing import Optional
from justapi import JustAPIApp

@dataclass
class Item:
    name: str
    price: float
    description: Optional[str] = None

app = JustAPIApp()

@app.post("/items")
def create_item(item: Item):
    return {"name": item.name, "price": item.price}
```

## Dataclasses as Response Models

```python
@dataclass
class ItemResponse:
    id: int
    name: str
    price: float

@app.get("/items/{item_id}", response_model=ItemResponse)
def get_item(item_id: int):
    return ItemResponse(id=item_id, name="Widget", price=9.99)
```

## See Also

- [Request Body](/tutorials/request-body/) — Pydantic model usage
- [Using the Request Directly](/advanced/using-request-directly/) — raw request access
- [Response Model](/tutorials/response-model/) — filtering output
