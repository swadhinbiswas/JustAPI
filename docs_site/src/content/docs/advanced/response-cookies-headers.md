---
title: Response Cookies and Headers
description: Set cookies and custom headers on responses in JustAPI.
keywords: [JustAPI, response cookies, response headers, Set-Cookie, custom headers]
---

## Setting Cookies

```python
from starlette.responses import JSONResponse
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/set-cookie")
def set_cookie():
    response = JSONResponse(content={"message": "cookie set"})
    response.set_cookie("session_id", "abc123", httponly=True, secure=True)
    return response
```

## Deleting Cookies

```python
@app.get("/clear-cookie")
def clear_cookie():
    response = JSONResponse(content={"message": "cookie cleared"})
    response.delete_cookie("session_id")
    return response
```

## Cookie Options

| Option | Description |
|--------|-------------|
| `max_age` | Expiration in seconds |
| `httponly` | Prevent JavaScript access |
| `secure` | Only send over HTTPS |
| `samesite` | CSRF protection (`"lax"`, `"strict"`, `"none"`) |
| `path` | Cookie scope path |

## Custom Response Headers

```python
@app.get("/with-headers")
def with_headers():
    response = JSONResponse(content={"data": "value"})
    response.headers["X-Request-Id"] = "req-123"
    response.headers["X-Response-Time"] = "42ms"
    return response
```

## See Also

- [Return a Response Directly](/advanced/response-directly/) — full response control
- [Security — First Steps](/tutorials/security/first-steps/) — auth with cookies
- [Cookie Parameters](/tutorials/cookie-params/) — reading cookies
