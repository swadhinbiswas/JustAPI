---
title: Custom Request and Route Classes
description: Extend the Request and APIRoute classes in JustAPI for custom behavior.
keywords: [JustAPI, custom request, custom route, APIRoute, extend]
---

## Custom Route Class

Override the default `APIRoute` to add custom behavior:

```python
from starlette.routing import APIRoute
from justapi import JustAPIApp, Request

class TimingRoute(APIRoute):
    def get_route_handler(self):
        original_handler = super().get_route_handler()

        async def custom_handler(request: Request):
            import time
            start = time.time()
            response = await original_handler(request)
            duration = time.time() - start
            response.headers["X-Process-Time"] = str(round(duration, 4))
            return response

        return custom_handler

app = JustAPIApp(route_class=TimingRoute)

@app.get("/items/")
def list_items():
    return {"items": []}
```

## Custom Request Class

```python
from starlette.requests import Request

class CustomRequest(Request):
    async def json(self):
        body = await super().json()
        # Transform the body before validation
        return body

@app.post("/items")
async def create_item(request: CustomRequest):
    data = await request.json()
    return data
```

## See Also

- [Using the Request Directly](/advanced/using-request-directly/) — raw request access
- [Advanced Middleware](/advanced/advanced-middleware/) — custom middleware
- [Advanced Dependencies](/advanced/advanced-dependencies/) — custom deps
