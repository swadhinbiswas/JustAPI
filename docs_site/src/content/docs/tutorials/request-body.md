---
title: Request Body & Validation
description: Parse and validate JSON request bodies with Pydantic schemas and JustAPI's native Rust fast path — a high-performance FastAPI alternative.
keywords: request body, JSON validation, Pydantic, Rust validation, JustAPI, FastAPI alternative, schema
---

JustAPI validates request bodies against schemas defined with Pydantic models or the built-in `Schema` class. Validation runs in Rust by default, keeping the GIL released.

## Using Pydantic Models

```python
from justapi import JustAPIApp
from pydantic import BaseModel

app = JustAPIApp()


class Item(BaseModel):
    name: str
    description: str | None = None
    price: float
    tax: float | None = None


@app.post("/items/")
def create_item(request, item: Item):
    item_dict = item.model_dump()
    if item.tax:
        item_dict["price_with_tax"] = item.price + item.tax
    return item_dict
```

```bash
curl -X POST http://127.0.0.1:8000/items/ \
  -H "Content-Type: application/json" \
  -d '{"name": "Laptop", "price": 999.99, "tax": 50.0}'
# Output: {"name":"Laptop","description":null,"price":999.99,"tax":50.0,"price_with_tax":1049.99}
```

### Validation Errors

Invalid data returns 422 with details:

```bash
curl -X POST http://127.0.0.1:8000/items/ \
  -H "Content-Type: application/json" \
  -d '{"name": "Laptop", "price": "free"}'
# Output: {"detail":"validation error for price: invalid type"}
```

## Using `body_schema` for Rust-Side Validation

For maximum performance, pass a schema explicitly via `body_schema`. Validation runs entirely in Rust before Python is called:

```python
from justapi import JustAPIApp, Schema
from pydantic import Field


class ProductCreate(Schema):
    name: str = Field(..., min_length=1, max_length=100)
    price: float = Field(..., gt=0, le=100000)
    quantity: int = Field(0, ge=0)


@app.post("/products/", body_schema=ProductCreate)
def create_product(request):
    data = request.json()
    return {"status": "created", "product": data}
```

## The Native Fast Path (`native=True`)

For the absolute fastest path (**724,000+ RPS**), use `native=True`. The entire handler runs in Rust — no Python bytecode execution:

```python
@app.post("/fast-items/", body_schema=ProductCreate, native=True)
def fast_create(request):
    return {"status": "ok"}
```

### Performance Comparison

| Approach | RPS | p50 Latency |
|---|---|---|
| Python handler + Pydantic | ~60,000 | 0.8 ms |
| `body_schema` (Rust validates) | ~280,000 | 0.2 ms |
| `native=True` (full Rust) | ~724,000 | 0.07 ms |

## Using JustAPI's Native `Schema` Class

The built-in `Schema` class supports field constraints with `Field()`:

```python
from justapi import JustAPIApp, Schema
from pydantic import Field


class UserCreate(Schema):
    username: str = Field(..., min_length=3, max_length=50, regex=r"^\w+$")
    email: str = Field(..., format="email")
    age: int = Field(..., ge=18, le=120)
    role: str = Field("user", enum=["user", "admin", "moderator"])
```

### Supported Field Constraints

| Constraint | Type | Description |
|---|---|---|
| `gt` | number | Greater than |
| `ge` | number | Greater than or equal |
| `lt` | number | Less than |
| `le` | number | Less than or equal |
| `min_length` | int | Minimum string length |
| `max_length` | int | Maximum string length |
| `regex` | str | Regex pattern match |
| `format` | str | Named format (`email`, `uri`, `uuid`, `date`, `date-time`, `hostname`, `ipv4`) |
| `enum` | list | Allowed values |
| `default` | any | Default value |
| `description` | str | Field description for OpenAPI |

## Nested Models

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

The nested `Address` schema is lifted into `$defs` and referenced via `$ref` in the generated JSON Schema. Validation runs entirely in Rust.

## Request Body Size Limits

By default, request bodies are limited to 50 MiB. You can configure this:

```python
app.run("0.0.0.0:8000", max_body_size=1024 * 1024)  # 1 MiB cap
```

Bodies exceeding the limit receive **413 Payload Too Large** immediately.

## Next Steps

- [Dependency Injection](/tutorials/dependency-injection/) — Access request components with Depends
- [Error Handling](/tutorials/error-handling/) — Custom validation error responses
- [Native Fast Path](/advanced/native-fast-path/) — Deep dive into Rust-native execution
