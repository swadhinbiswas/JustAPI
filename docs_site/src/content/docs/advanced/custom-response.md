---
title: Custom Response Classes
description: Use HTML, streaming, file, and other custom response types in JustAPI.
keywords: [JustAPI, custom response, HTML, streaming, file response, redirect, response_class]
---

## Using response_class in Decorator

Set the response class directly in the decorator:

```python
from starlette.responses import HTMLResponse, PlainTextResponse
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/", response_class=PlainTextResponse)
def root():
    return "Hello, plain text!"
```

This also documents the correct Content-Type in the OpenAPI schema.

## HTML Response

```python
@app.get("/html")
def html_page():
    return HTMLResponse(content="<h1>Hello</h1><p>This is HTML</p>")
```

Or using `response_class`:

```python
@app.get("/html", response_class=HTMLResponse)
def html_page():
    return "<h1>Hello</h1>"
```

## Streaming Response

```python
from starlette.responses import StreamingResponse
import asyncio

async def generate_tokens():
    for i in range(10):
        yield f"data: token_{i}\n\n"
        await asyncio.sleep(0.1)

@app.get("/stream")
def stream():
    return StreamingResponse(generate_tokens(), media_type="text/event-stream")
```

## Redirect

```python
from starlette.responses import RedirectResponse

@app.get("/old-path")
def old_path():
    return RedirectResponse(url="/new-path", status_code=302)

@app.get("/new-path")
def new_path():
    return {"message": "you are here"}
```

Or using `response_class`:

```python
@app.get("/old-path", response_class=RedirectResponse, status_code=302)
def old_path():
    return "/new-path"
```

## File Response

```python
from starlette.responses import FileResponse

@app.get("/download")
def download():
    return FileResponse("report.pdf", filename="report.pdf")
```

## JSON Response with Custom Headers

```python
from starlette.responses import JSONResponse

@app.get("/custom")
def custom():
    return JSONResponse(
        content={"data": "value"},
        headers={"X-Custom": "header-value"},
    )
```

## Plain Text Response

```python
@app.get("/text")
def text():
    return PlainTextResponse("Hello, plain text!")
```

## Default Response Class

Set a default response class for all routes:

```python
from starlette.responses import ORJSONResponse

app = JustAPIApp(default_response_class=ORJSONResponse)
```

:::tip
`ORJSONResponse` is faster than `JSONResponse` for large payloads. Install with `pip install orjson`.
:::

## Custom Response Class

Create your own response class:

```python
from starlette.responses import JSONResponse

class PrettyJSONResponse(JSONResponse):
    def render(self, content) -> bytes:
        import json
        return json.dumps(
            content,
            indent=2,
            ensure_ascii=False,
        ).encode("utf-8")

@app.get("/pretty")
def pretty():
    return PrettyJSONResponse(content={"data": "value"})
```

## Response Class + Status Code

Combine `response_class` with `status_code`:

```python
@app.post("/items", response_class=JSONResponse, status_code=201)
def create_item():
    return {"message": "created"}
```

## See Also

- [Return a Response Directly](/advanced/response-directly/) — basic response object usage
- [Streaming Output](/advanced/streaming-output/) — JustAPI-native streaming
- [Static Files](/tutorials/static-files/) — serving files
