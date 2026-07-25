---
title: Body — Fields
description: Use Field() for Pydantic model field-level validation, defaults, and descriptions in JustAPI.
keywords: [JustAPI, Field, Pydantic, validation, field-level, model fields]
---

## Using Field()

Use `Field()` to add metadata and constraints to Pydantic model fields:

```python
from pydantic import BaseModel, Field
from typing import Optional
from justapi import JustAPIApp

class Item(BaseModel):
    name: str = Field(..., min_length=1, max_length=100)
    price: float = Field(..., gt=0)
    description: Optional[str] = Field(None, max_length=500)
    tags: list[str] = Field(default_factory=list)

app = JustAPIApp()

@app.post("/items")
def create_item(item: Item):
    return item.model_dump()
```

## Field Options

| Option | Type | Description |
|--------|------|-------------|
| `default` | `Any` | Default value |
| `alias` | `str` | Alternative field name |
| `title` | `str` | Title in OpenAPI docs |
| `description` | `str` | Description in OpenAPI docs |
| `examples` | `list` | Example values |
| `min_length` | `int` | Minimum string length |
| `max_length` | `int` | Maximum string length |
| `gt` | `Number` | Greater than |
| `ge` | `Number` | Greater than or equal |
| `lt` | `Number` | Less than |
| `le` | `Number` | Less than or equal |
| `pattern` | `str` | Regex pattern |

## Examples

```python
class CreateUser(BaseModel):
    username: str = Field(
        ...,
        min_length=3,
        max_length=50,
        pattern=r"^[a-zA-Z0-9_]+$",
        title="Username",
        description="Alphanumeric characters and underscores only",
        examples=["alice_123"],
    )
    email: str = Field(
        ...,
        pattern=r"^[\w.-]+@[\w.-]+\.\w+$",
        title="Email address",
    )
    age: int = Field(
        ...,
        ge=13,
        le=120,
        title="Age",
        description="Must be between 13 and 120",
    )
```

## See Also

- [Schema & Validation](/api-reference/schema-validation/) — validation reference
- [Body — Multiple Parameters](/tutorials/body-multiple-params/) — multiple body params
- [Body — Nested Models](/tutorials/body-nested-models/) — nested models
