---
title: Custom Request Class
description: Extend the Request class in JustAPI for custom request behavior.
keywords: [JustAPI, custom request, extend, Request]
---

## Custom Request Class

JustAPI's routing is fully native (Rust `matchit`), so there is no
replaceable `APIRoute`/`route_class` mechanism — routes are registered on
`JustAPIApp`. You *can* subclass `justapi.Request` and accept it as a parameter
to customize how a handler reads the request.

```python
from justapi import JustAPIApp, Request

class CustomRequest(Request):
    async def json(self):
        body = await super().json()
        # pre-process the body before application logic
        return body

app = JustAPIApp()

@app.post("/items")
async def create_item(request: CustomRequest):
    data = await request.json()
    return data
```

For per-route cross-cutting behavior (timing, auth, header injection), use the
native middleware mechanisms instead:

```python
import time
from justapi import JustAPIApp

app = JustAPIApp()

@app.middleware("http")
async def timing(request, call_next):
    start = time.time()
    response = await call_next(request)
    response.headers["X-Process-Time"] = str(round(time.time() - start, 4))
    return response
```

## See Also

- [Using the Request Directly](/advanced/using-request-directly/) — raw request access
- [Advanced Middleware](/advanced/advanced-middleware/) — custom middleware
- [Advanced Dependencies](/advanced/advanced-dependencies/) — custom deps