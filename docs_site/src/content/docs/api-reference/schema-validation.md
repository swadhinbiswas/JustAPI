---
title: Schema & Validation API
description: "API reference for schema validation in JustAPI, the FastAPI alternative — Schema, Field, and validation functions."
keywords: [schema validation, fastapi alternative, justapi, schema class, field, validation, pydantic]
---

JustAPI provides two validation paths: the built-in `Schema` class (validated in Rust) and Pydantic models (validated in Python).

## `Schema` Class

Define a validation schema using the built-in `Schema` class:

```python
from justapi import JustAPIApp, Schema
from pydantic import Field

class ProductCreate(Schema):
    name: str = Field(..., min_length=1, max_length=100)
    price: float = Field(..., gt=0, le=100000)
    quantity: int = Field(0, ge=0)
```

Schema subclasses can be used with `body_schema` to validate in Rust.

## `Field()` Constraints

```python
from pydantic import Field

Field(
    default=...,
    gt=None,           # Greater than
    ge=None,           # Greater than or equal
    lt=None,           # Less than
    le=None,           # Less than or equal
    min_length=None,   # Min string length
    max_length=None,   # Max string length
    regex=None,        # Regular expression pattern
    format=None,       # Named format (email, uri, uuid, date-time, date, hostname, ipv4)
    enum=None,         # List of allowed values
    description=None,  # Field description for OpenAPI docs
)
```

### Supported Formats

| Format | Valid Example | Invalid Example |
|---|---|---|
| `email` | `user@example.com` | `not-an-email` |
| `uri` | `https://example.com` | `not-a-uri` |
| `uuid` | `550e8400-...` | `not-a-uuid` |
| `date` | `2026-07-24` | `07/24/2026` |
| `date-time` | `2026-07-24T12:00:00Z` | `tomorrow` |
| `hostname` | `example.com` | `not a hostname` |
| `ipv4` | `192.168.1.1` | `256.0.0.1` |

## Using with `body_schema`

```python
@app.post("/products", body_schema=ProductCreate)
def create_product(request):
    data = request.json()
    return {"status": "created", "product": data}
```

When `body_schema` is set:
1. The request body is parsed and validated in Rust against the schema
2. Invalid bodies receive 422 without Python involvement
3. Valid bodies are passed to the handler as parsed JSON

## Native Fast Path

With `native=True`, the entire handler executes in Rust:

```python
@app.post("/fast-products", body_schema=ProductCreate, native=True)
def fast_create(request):
    return {"status": "ok"}
```

## Nested Schemas

```python
class Address(Schema):
    street: str
    city: str
    zip_code: str = Field(..., regex=r"^\d{5}(-\d{4})?$")

class Customer(Schema):
    name: str
    address: Address
    tags: list[str]
```

Nested schemas are compiled into a single JSON Schema with `$defs` and `$ref` references.

## `validate_value()` Function

Validate arbitrary data against a schema at runtime:

```python
from justapi._justapi import validate_value

schema = {
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "age": {"type": "integer", "minimum": 0},
    },
    "required": ["name", "age"],
}

result = validate_value({"name": "Alice", "age": 30}, schema)
# Returns: {"valid": true, "errors": []}
```

## Pydantic Bridge

Standard Pydantic models work with JustAPI routes without modification:

```python
from pydantic import BaseModel

class Item(BaseModel):
    name: str
    price: float

@app.post("/items")
def create_item(request, item: Item):
    return item.model_dump()
```

## See Also

- [Request Body & Validation Tutorial](/tutorials/request-body/) — Usage guide
- [Native Fast Path](/advanced/native-fast-path/) — Rust-native execution
- [JustAPIApp](/api-reference/justapiapp/) — body_schema parameter on route decorators
