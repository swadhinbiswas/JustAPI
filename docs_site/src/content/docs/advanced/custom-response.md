---
title: Custom Response Classes
description: Use HTML, streaming, file, and other custom response types in JustAPI.
keywords: [JustAPI, custom response, HTML, streaming, file response, redirect]
---

## HTML Response

```python
from starlette.responses import HTMLResponse
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/html")
def html_page():
    return HTMLResponse(content="<h1>Hello</h1><p>This is HTML</p>")
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

## File Response

```python
from starlette.responses import FileResponse

@app.get("/download")
def download():
    return FileResponse("report.pdf", filename="report.pdf")
```

## Plain Text Response

```python
from starlette.responses import PlainTextResponse

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

## See Also

- [Return a Response Directly](/advanced/response-directly/) — basic response object usage
- [Streaming Output](/advanced/streaming-output/) — JustAPI-native streaming
- [Static Files](/tutorials/static-files/) — serving files
