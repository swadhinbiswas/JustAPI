---
title: Custom Response Classes
description: Use HTML, streaming, file, and other custom response types in JustAPI.
keywords: [JustAPI, custom response, HTML, streaming, file response, redirect, response]
---

JustAPI ships native response classes — `HTMLResponse`, `PlainTextResponse`,
`JSONResponse`, `RedirectResponse`, `StreamingResponse`, and `FileResponse` —
imported directly from `justapi`. Return them from any handler to override the
default JSON serialization.

## HTML Response

```python
from justapi import HTMLResponse, JustAPIApp

app = JustAPIApp()

@app.get("/html")
def html_page():
    return HTMLResponse(content="<h1>Hello</h1><p>This is HTML</p>")
```

## Plain Text Response

```python
from justapi import PlainTextResponse

@app.get("/text")
def text():
    return PlainTextResponse("Hello, plain text!")
```

## JSON Response with Custom Headers

```python
from justapi import JSONResponse

@app.get("/custom")
def custom():
    return JSONResponse(
        content={"data": "value"},
        headers={"X-Custom": "header-value"},
    )
```

## Streaming Response

```python
from justapi import StreamingResponse

async def generate_tokens():
    for i in range(10):
        yield f"data: token_{i}\n\n"
        import asyncio
        await asyncio.sleep(0.1)

@app.get("/stream")
def stream():
    return StreamingResponse(generate_tokens(), media_type="text/event-stream")
```

> JustAPI also ships a dedicated `TokenStreamResponse` for LLM token streaming
> (see [Streaming Output](/advanced/streaming-output/)).

## Redirect

```python
from justapi import RedirectResponse

@app.get("/old-path")
def old_path():
    return RedirectResponse(url="/new-path", status_code=302)

@app.get("/new-path")
def new_path():
    return {"message": "you are here"}
```

## File Response

```python
from justapi import FileResponse

@app.get("/download")
def download():
    return FileResponse("report.pdf", filename="report.pdf")
```

> `FileResponse` reads the file eagerly into memory; the media type is inferred
> from the file extension unless `media_type` is given.

## Custom Response Class

Subclass a native response and override `render`:

```python
from justapi import JSONResponse

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

## Response + Status Code

Set a non-default status by returning a response object with an explicit
`status_code` (the route-decorator `status_code=` kwarg is OpenAPI metadata,
not applied to the response):

```python
from justapi import JSONResponse

@app.post("/items")
def create_item():
    return JSONResponse(content={"message": "created"}, status_code=201)
```

## See Also

- [Return a Response Directly](/advanced/response-directly/) — basic response object usage
- [Streaming Output](/advanced/streaming-output/) — JustAPI-native streaming
- [Static Files](/tutorials/static-files/) — serving files