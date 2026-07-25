---
title: Body — Nested Models
description: Use nested Pydantic models for complex JSON structures in JustAPI — sets, lists, dicts, special types.
keywords: [JustAPI, nested models, Pydantic, complex JSON, sets, lists, dicts, HttpUrl]
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

## Set Types

Use `set` to deduplicate values:

```python
class Item(BaseModel):
    name: str
    tags: set[str] = set()

@app.post("/items")
def create_item(item: Item):
    return item.model_dump()
```

```json
{
  "name": "Widget",
  "tags": ["hardware", "sale", "hardware"]
}
```

The response deduplicates:

```json
{
  "name": "Widget",
  "tags": ["hardware", "sale"]
}
```

## Bodies of Pure Lists

The entire request body can be a list:

```python
class Image(BaseModel):
    url: str
    caption: str = ""

@app.post("/images")
def create_images(images: list[Image]):
    return {"count": len(images), "images": images}
```

```json
[
  {"url": "https://example.com/img1.jpg", "caption": "Photo 1"},
  {"url": "https://example.com/img2.jpg"}
]
```

## Bodies of Arbitrary Dicts

Use `dict` for dynamic keys:

```python
@app.post("/metrics")
def record_metrics(metrics: dict[str, float]):
    return {"metrics": metrics}
```

```json
{
  "cpu": 75.5,
  "memory": 62.3,
  "disk": 45.1
}
```

Typed keys:

```python
@app.post("/config")
def update_config(config: dict[str, str]):
    return {"config": config}
```

## Special Types

Pydantic includes types for common formats:

```python
from pydantic import BaseModel, HttpUrl, EmailStr

class Website(BaseModel):
    url: HttpUrl
    contact_email: EmailStr

@app.post("/websites")
def create_website(website: Website):
    return {"url": str(website.url), "email": str(website.contact_email)}
```

| Type | Validates |
|------|-----------|
| `HttpUrl` | Valid HTTP/HTTPS URL |
| `EmailStr` | Valid email address |
| `AnyUrl` | Any valid URL |
| `IPvAnyAddress` | Valid IP address |
| `AnyHttpUrl` | HTTP or HTTPS URL |

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
