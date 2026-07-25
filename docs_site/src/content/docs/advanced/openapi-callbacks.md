---
title: OpenAPI Callbacks and Webhooks
description: Define OpenAPI callbacks and webhooks for outbound request schemas in JustAPI.
keywords: [JustAPI, OpenAPI, callbacks, webhooks, outbound requests]
---

## OpenAPI Callbacks

Callbacks define the schema for outbound requests your API makes:

```python
from justapi import JustAPIApp
from pydantic import BaseModel

class Order(BaseModel):
    id: int
    status: str

app = JustAPIApp()

@app.post("/orders", callbacks={
    "on_order_complete": {
        "http://localhost:8000/webhook": {
            "post": {
                "requestBody": {
                    "content": {
                        "application/json": {
                            "schema": Order.model_jsonschema()
                        }
                    }
                }
            }
        }
    }
})
def create_order(order: Order):
    return {"id": 1, "status": "pending"}
```

## OpenAPI Webhooks

Define outbound webhook schemas:

```python
@app.webhooks.post("new-order")
def new_order_webhook(order: Order):
    """
    This webhook is triggered when a new order is created.
    """
    pass
```

## See Also

- [Additional Responses in OpenAPI](/advanced/additional-responses/) — error responses
- [Metadata & Docs URLs](/tutorials/metadata/) — app metadata
- [OpenAPI Callbacks Reference](/advanced/openapi-callbacks/) — full reference
