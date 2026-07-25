---
title: Extra Data Types
description: Advanced Python data types supported by JustAPI — UUID, datetime, Enum, and more.
keywords: [JustAPI, data types, UUID, datetime, Enum, Decimal, extra types]
---

Beyond basic types, JustAPI supports all standard library and Pydantic types:

## UUID

```python
from uuid import UUID
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/users/{user_id}")
def get_user(user_id: UUID):
    return {"user_id": str(user_id)}
```

`GET /users/550e8400-e29b-41d4-a716-446655440000` validates the UUID format.

## datetime and date

```python
from datetime import datetime, date
from justapi import JustAPIApp

app = JustAPIApp()

@app.post("/events")
def create_event(start: datetime, end: date = None):
    return {"start": start.isoformat(), "end": end.isoformat() if end else None}
```

Accepts ISO 8601 strings:
- `datetime`: `"2026-07-25T10:30:00Z"`
- `date`: `"2026-07-25"`

## Enum

```python
from enum import Enum
from justapi import JustAPIApp

class Priority(str, Enum):
    low = "low"
    medium = "medium"
    high = "high"

app = JustAPIApp()

@app.post("/tasks")
def create_task(priority: Priority):
    return {"priority": priority.value}
```

The docs will show a dropdown with the allowed values.

## Decimal

```python
from decimal import Decimal
from justapi import JustAPIApp

app = JustAPIApp()

@app.post("/payments")
def process_payment(amount: Decimal):
    return {"amount": str(amount)}
```

## Literal Types

```python
from typing import Literal
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/status")
def get_status(status: Literal["active", "inactive"]):
    return {"status": status}
```

## See Also

- [Python Types Intro](/tutorials/python-types/) — basic types
- [Query Parameters & String Validations](/tutorials/query-params-str-validations/) — string validation
- [Path Parameters & Numeric Validations](/tutorials/path-params-numeric-validations/) — numeric validation
