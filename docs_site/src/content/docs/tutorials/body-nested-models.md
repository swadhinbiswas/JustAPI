---
title: Body — Nested Models
description: Use nested Pydantic models for complex JSON structures in JustAPI.
keywords: [JustAPI, nested models, Pydantic, complex JSON, deeply nested]
---

## Basic Nested Models

```python
from pydantic import BaseModel
from typing import Optional
from justapi import JustAPIApp

class Address(BaseModel):
    street: str
    city: str
    state: str
    zip_code: str

class User(BaseModel):
    name: str
    email: str
    address: Address
    secondary_address: Optional[Address] = None

app = JustAPIApp()

@app.post("/users")
def create_user(user: User):
    return user.model_dump()
```

The client sends:

```json
{
  "name": "Alice",
  "email": "alice@example.com",
  "address": {
    "street": "123 Main St",
    "city": "Springfield",
    "state": "IL",
    "zip_code": "62701"
  }
}
```

## Lists of Nested Models

```python
class Tag(BaseModel):
    name: str
    color: str = "gray"

class Item(BaseModel):
    name: str
    tags: list[Tag] = []

@app.post("/items")
def create_item(item: Item):
    return item.model_dump()
```

```json
{
  "name": "Widget",
  "tags": [
    {"name": "hardware", "color": "blue"},
    {"name": "sale", "color": "red"}
  ]
}
```

## Deeply Nested Structures

```python
class Comment(BaseModel):
    author: str
    text: str
    replies: list["Comment"] = []

class Post(BaseModel):
    title: str
    content: str
    comments: list[Comment] = []

@app.post("/posts")
def create_post(post: Post):
    return post.model_dump()
```

## Validation at Every Level

JustAPI validates the entire nested structure. Invalid data at any level returns a clear error:

```json
{
  "detail": [
    {
      "type": "string_type",
      "loc": ["body", "address", "zip_code"],
      "msg": "Input should be a valid string",
      "input": 12345
    }
  ]
}
```

## See Also

- [Request Body](/tutorials/request-body/) — basic Pydantic usage
- [Body — Fields](/tutorials/body-fields/) — field-level validation
- [Extra Models](/tutorials/extra-models/) — multiple related models
