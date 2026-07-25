---
title: Path Operation Configuration
description: Configure OpenAPI metadata for path operations — tags, summary, description, deprecated, and more.
keywords: [JustAPI, path operation, OpenAPI, tags, summary, deprecated]
---

## Basic Configuration

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get(
    "/items/{item_id}",
    summary="Get an item",
    description="Retrieve a single item by its ID.",
    tags=["items"],
    response_description="The item",
)
def get_item(item_id: int):
    return {"item_id": item_id}
```

## Tags

Tags group endpoints in the docs:

```python
@app.get("/users/", tags=["users"])
def list_users():
    return []

@app.post("/items/", tags=["items"])
def create_item():
    return {}
```

## Deprecating Endpoints

```python
@app.get("/old-endpoint", deprecated=True)
def old_endpoint():
    return {"message": "this endpoint is deprecated"}
```

## Controlling Doc Visibility

```python
@app.get("/internal", include_in_schema=False)
def internal_only():
    return {"secret": "data"}
```

This hides the endpoint from OpenAPI documentation entirely.

## See Also

- [Metadata & Docs URLs](/tutorials/metadata/) — app-level metadata
- [OpenAPI Callbacks & Webhooks](/advanced/openapi-callbacks/) — advanced OpenAPI
