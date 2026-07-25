---
title: Body — Updates
description: Implement partial updates (PATCH) with Pydantic models in JustAPI.
keywords: [JustAPI, PATCH, partial update, body updates, Pydantic]
---

## PUT vs PATCH

- **PUT** — full replacement. The client sends the entire resource.
- **PATCH** — partial update. The client sends only the fields to change.

## Full Update with PUT

```python
from pydantic import BaseModel
from justapi import JustAPIApp

class Item(BaseModel):
    name: str
    price: float
    description: str = ""

app = JustAPIApp()

@app.put("/items/{item_id}")
def update_item(item_id: int, item: Item):
    return {"item_id": item_id, **item.model_dump()}
```

## Partial Update with PATCH

Use `exclude_unset=True` to get only the fields the client explicitly sent:

```python
@app.patch("/items/{item_id}")
def partial_update(item_id: int, item: Item):
    update_data = item.model_dump(exclude_unset=True)
    # update_data only contains fields the client sent
    return {"item_id": item_id, "updates": update_data}
```

If the client sends `{"name": "New Name"}`, only `name` is in `update_data`.

## Optional Model

```python
from typing import Optional

class ItemUpdate(BaseModel):
    name: Optional[str] = None
    price: Optional[float] = None
    description: Optional[str] = None

@app.patch("/items/{item_id}")
def partial_update(item_id: int, item: ItemUpdate):
    update_data = item.model_dump(exclude_unset=True)
    return {"item_id": item_id, "updates": update_data}
```

## See Also

- [Request Body](/tutorials/request-body/) — full request body usage
- [Body — Multiple Parameters](/tutorials/body-multiple-params/) — mixing parameter sources
