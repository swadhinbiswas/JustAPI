---
title: JSON Compatible Encoder
description: Convert Pydantic models and complex types to JSON-compatible dictionaries in JustAPI.
keywords: [JustAPI, encoder, jsonable_encoder, serialization, Pydantic]
---

## The Problem

When you need to convert Pydantic models to JSON-compatible dictionaries (for example, before passing to a database driver), you can't just call `model_dump()` — it doesn't handle `datetime`, `UUID`, or other non-JSON types.

## JustAPI's Encoder

```python
from justapi import JustAPIApp
from pydantic import BaseModel
from datetime import datetime

class Event(BaseModel):
    name: str
    starts: datetime

app = JustAPIApp()

@app.post("/events")
def create_event(event: Event):
    # Convert to JSON-compatible dict
    data = event.model_dump()
    # starts is a datetime object — need to serialize it
    return {"event_name": data["name"], "starts": data["starts"].isoformat()}
```

## Model_dump with mode

Pydantic v2 provides `model_dump()` with serialization modes:

```python
@app.post("/events")
def create_event(event: Event):
    data = event.model_dump(mode="json")
    # data["starts"] is now an ISO string
    return data
```

## See Also

- [Request Body](/tutorials/request-body/) — Pydantic models
- [Response Model](/tutorials/response-model/) — filtering output
