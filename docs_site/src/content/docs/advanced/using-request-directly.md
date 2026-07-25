---
title: Using the Request Directly
description: Access the raw Starlette Request object for headers, body, and client info in JustAPI.
keywords: [JustAPI, request object, raw request, headers, body, client]
---

## Accessing the Request

Any handler can receive the raw `Request` object as a parameter:

```python
from starlette.requests import Request
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/info")
def get_info(request: Request):
    return {
        "method": request.method,
        "url": str(request.url),
        "client": request.client.host if request.client else None,
    }
```

## Reading Headers

```python
@app.get("/headers")
def get_headers(request: Request):
    return {
        "user_agent": request.headers.get("user-agent"),
        "accept": request.headers.get("accept"),
        "authorization": request.headers.get("authorization", "none"),
    }
```

## Reading Body

```python
@app.post("/raw-body")
async def raw_body(request: Request):
    body = await request.body()
    return {"size": len(body), "content": body.decode()}
```

## Path and Query Info

```python
@app.get("/request-info")
def request_info(request: Request):
    return {
        "path": request.url.path,
        "query": str(request.query_params),
        "path_params": dict(request.path_params),
    }
```

## See Also

- [Path Parameters](/tutorials/path-params/) — extracting path params
- [Header Parameters](/tutorials/header-params/) — typed header access
- [Cookie Parameters](/tutorials/cookie-params/) — typed cookie access
