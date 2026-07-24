---
title: Response Classes
description: "API reference for response classes in JustAPI, the FastAPI alternative — return responses with different content types, status codes, and streaming."
keywords: [responses, fastapi alternative, justapi, jsonresponse, streamingresponse, http response]
---

JustAPI provides several response classes optimized for zero-copy data transfer from Rust to Python.

## `Response`

Base response class:

```python
from justapi.responses import Response

return Response(
    content="Hello",
    status_code=200,
    headers={"X-Custom": "value"},
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `content` | `str` | `""` | Response body text |
| `status_code` | `int` | `200` | HTTP status code |
| `headers` | `dict` | `None` | Additional headers |

## `JSONResponse`

Automatically serializes a Python object to JSON using Rust's `serde_json`:

```python
from justapi.responses import JSONResponse

return JSONResponse({"message": "Hello", "items": [1, 2, 3]})
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `content` | `dict` or `list` | required | Python object to serialize |

## `HTMLResponse`

Sets `Content-Type: text/html`:

```python
from justapi.responses import HTMLResponse

return HTMLResponse("<h1>Hello</h1>")
```

## `PlainTextResponse`

Sets `Content-Type: text/plain`:

```python
from justapi.responses import PlainTextResponse

return PlainTextResponse("Hello, World!")
```

## `RedirectResponse`

Returns an HTTP redirect:

```python
from justapi.responses import RedirectResponse

return RedirectResponse(
    url="https://example.com",
    status_code=307,  # Temporary redirect
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `url` | `str` | required | Redirect target URL |
| `status_code` | `int` | `307` | Redirect status (307, 308, 301, 302) |

## `StreamingResponse`

Streams content using a Python generator:

```python
from justapi.responses import StreamingResponse
import asyncio


@app.get("/stream")
async def stream(request):
    async def generate():
        for i in range(100):
            yield f"data: {i}\n\n"
            await asyncio.sleep(0.1)

    return StreamingResponse(generate(), media_type="text/plain")
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `generator` | async generator | required | Content generator |
| `media_type` | `str` | `"text/plain"` | Content-Type header |

## Dict Return (Shortcut)

When a route handler returns a plain dict, it's automatically converted to `JSONResponse`:

```python
@app.get("/")
def handler(request):
    return {"message": "Hello"}  # Equivalent to JSONResponse(...)
```

## Setting Status Codes

Use tuples or the `status` key in the response dict for non-200 status codes:

```python
@app.post("/items")
def create_item(request):
    return {"status": 201, "body": {"id": 1, "name": "Item"}}
```

The `status` key sets the HTTP status code, while `body` contains the response body.

## See Also

- [Request Object](/api-reference/request/) — Accessing request data
- [Streaming Output](/advanced/streaming-output/) — Validated streaming with `@app.stream_json`
- [Error Handling](/tutorials/error-handling/) — Error response patterns
