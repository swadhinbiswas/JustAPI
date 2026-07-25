---
title: Advanced Middleware
description: Add ASGI middleware, HTTPS redirect, trusted hosts, and GZip compression in JustAPI.
keywords: [JustAPI, middleware, ASGI, HTTPS redirect, trusted host, GZip]
---

## Adding ASGI Middleware

JustAPI supports any ASGI middleware:

```python
from justapi import JustAPIApp
from starlette.middleware.httpsredirect import HTTPSRedirectMiddleware
from starlette.middleware.trustedhost import TrustedHostMiddleware

app = JustAPIApp()

app.add_middleware(HTTPSRedirectMiddleware)
```

## Trusted Host Middleware

Guard against HTTP host header attacks:

```python
app.add_middleware(
    TrustedHostMiddleware,
    allowed_hosts=["example.com", "*.example.com"],
)
```

## GZip Compression

```python
from starlette.middleware.gzip import GZipMiddleware

app.add_middleware(GZipMiddleware, minimum_size=500, compresslevel=9)
```

## Custom ASGI Middleware

Write your own middleware class:

```python
import time
from starlette.middleware.base import BaseHTTPMiddleware

class TimingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        start = time.time()
        response = await call_next(request)
        duration = time.time() - start
        response.headers["X-Process-Time"] = str(round(duration, 4))
        return response

app.add_middleware(TimingMiddleware)
```

## See Also

- [Middleware](/tutorials/middleware/) — basic middleware usage
- [Behind a Proxy](/advanced/behind-a-proxy/) — reverse proxy configuration
- [Sub Applications](/advanced/sub-applications/) — mounting sub-apps
