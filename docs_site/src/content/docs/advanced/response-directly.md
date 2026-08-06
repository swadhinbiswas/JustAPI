---
title: Return a Response Directly
description: Return a Response object directly from a JustAPI handler for full control over headers, status, and body.
keywords: [JustAPI, response, direct response, headers, status code]
---

## Returning a Response Object

For full control over the response, return a `Response` object:

```python
from justapi import JSONResponse
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/custom")
def custom_response():
    return JSONResponse(
        content={"message": "custom response"},
        status_code=202,
        headers={"X-Custom-Header": "value"},
    )
```

## Response Classes

| Class | Use Case |
|-------|----------|
| `JSONResponse` | JSON response with custom headers |
| `HTMLResponse` | HTML content |
| `PlainTextResponse` | Plain text |
| `RedirectResponse` | Redirect to another URL |
| `StreamingResponse` | Streaming data |
| `FileResponse` | Serve a file |

## Headers

Set custom headers on any response:

```python
@app.get("/with-headers")
def with_headers():
    return JSONResponse(
        content={"data": "value"},
        headers={"X-Request-Id": "abc-123", "Cache-Control": "no-cache"},
    )
```

## Redirect

```python
from justapi import RedirectResponse

@app.get("/old-path")
def old_path():
    return RedirectResponse(url="/new-path")

@app.get("/new-path")
def new_path():
    return {"message": "you are here"}
```

## See Also

- [Custom Response Classes](/advanced/custom-response/) — HTML, stream, file responses
- [Response Status Code](/tutorials/response-status-code/) — status codes
- [Response Cookies & Headers](/advanced/response-cookies-headers/) — cookies and headers
