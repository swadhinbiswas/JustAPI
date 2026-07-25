---
title: Additional Responses in OpenAPI
description: Document additional error responses and custom status codes in OpenAPI for JustAPI.
keywords: [JustAPI, OpenAPI, additional responses, error responses, documentation]
---

## Declaring Error Responses

Document additional responses that your handler might return:

```python
from justapi import JustAPIApp, HTTPException
from pydantic import BaseModel

class ErrorResponse(BaseModel):
    detail: str

app = JustAPIApp()

@app.get(
    "/items/{item_id}",
    responses={
        404: {"model": ErrorResponse, "description": "Item not found"},
        500: {"description": "Internal server error"},
    },
)
def get_item(item_id: int):
    if item_id == 0:
        raise HTTPException(status_code=404, detail="Item not found")
    return {"item_id": item_id}
```

## Multiple Response Models

```python
@app.get(
    "/users/{user_id}",
    response_model=UserOut,
    responses={
        200: {"description": "User found"},
        404: {"model": ErrorResponse, "description": "User not found"},
        403: {"description": "Access denied"},
    },
)
def get_user(user_id: int):
    return {"user_id": user_id}
```

## Per-Media-Type Responses

```python
@app.get(
    "/data",
    responses={
        200: {
            "content": {
                "application/json": {"schema": {"type": "object"}},
                "text/plain": {"schema": {"type": "string"}},
            }
        }
    },
)
def get_data():
    return {"data": "value"}
```

## See Also

- [OpenAPI Callbacks & Webhooks](/advanced/openapi-callbacks/) — advanced OpenAPI features
- [Metadata & Docs URLs](/tutorials/metadata/) — app-level metadata
- [API Reference](/api-reference/) — full reference
