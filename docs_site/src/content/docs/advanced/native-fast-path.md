---
title: Native Fast Path
description: Execute routes entirely in Rust for maximum performance (724k+ RPS).
---

The **Native Fast Path** is JustAPI's most powerful performance feature. With `native=True`, a route executes **entirely in Rust** — no Python bytecode, no GIL overhead.

## When to Use

| Scenario | Use Native? | Expected RPS |
|---|---|---|
| High-throughput CRUD | ✓ Yes | 724,000+ |
| Schema-validated writes | ✓ Yes | 280,000+ |
| Complex business logic | ✗ No (use Python) | 60,000 |
| AI inference | ✗ No | Varies |

## Usage

```python
from justapi import JustAPIApp, Schema
from pydantic import Field


class ProductCreate(Schema):
    name: str = Field(..., min_length=1)
    price: float = Field(..., gt=0)
    quantity: int = Field(0, ge=0)


app = JustAPIApp()


@app.post("/fast-products", body_schema=ProductCreate, native=True)
def create_product(request):
    # This handler is compiled to Rust native code
    return {"status": "created"}
```

## How It Works

1. Route registration: The schema is compiled to a Rust JSON Schema validator
2. Request arrives: Rust validates the body against the schema (zero GIL)
3. Route executes: The handler function is called via a pre-compiled Rust callback
4. Response: Rust serializes the response with `serde_json`

The Python function is still written in Python syntax, but the execution path avoids the GIL entirely.

## Performance Breakdown

| Step | Time (native) | Time (Python) |
|---|---|---|
| Socket read | 0.01 ms | 0.01 ms |
| HTTP parse | 0.02 ms | 0.02 ms |
| Route match | 0.001 ms | 0.001 ms |
| JSON parse | 0.01 ms | 0.15 ms |
| Schema validate | 0.02 ms | 0.30 ms |
| Handler exec | 0.003 ms | 0.20 ms |
| JSON serialize | 0.01 ms | 0.10 ms |
| **Total** | **~0.07 ms** | **~0.78 ms** |

## Constraints

Native fast path routes:
- Must use `body_schema` with a `Schema` class (not Pydantic `BaseModel`)
- Must return a plain dict
- Cannot use Python dependencies (files, network, complex types)
- Cannot call other Python functions

## When to Use Python Handler

```python
@app.post("/complex")
def complex_handler(request, item: Item):
    # Complex business logic, external API calls, file processing
    processed = await process_external(item)
    send_notification(processed)
    return {"result": processed}
```

## See Also

- [Schema & Validation API](/api-reference/schema-validation/) — `Schema` class reference
- [Performance Tuning](/advanced/performance-tuning/) — Optimize your application
- [Zero-GIL Architecture](/advanced/zero-gil-architecture/) — Concurrency model
