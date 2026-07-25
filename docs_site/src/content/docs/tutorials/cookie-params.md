---
title: Cookie Parameters
description: Read and validate HTTP cookies in JustAPI.
keywords: [JustAPI, cookie parameters, cookies, session, HTTP cookies]
---

## Basic Cookie Parameters

```python
from justapi import JustAPIApp, Cookie

app = JustAPIApp()

@app.get("/check")
def check_session(session_id: str = Cookie(...)):
    return {"session_id": session_id}
```

## Required vs Optional

```python
@app.get("/check")
def check(
    session_id: str = Cookie(...),        # required
    language: str = Cookie("en"),          # optional, defaults to "en"
    theme: str = Cookie(None),            # optional, defaults to None
):
    return {"session_id": session_id, "language": language, "theme": theme}
```

## Cookie Models

```python
from pydantic import BaseModel
from justapi import JustAPIApp, Cookie

class Cookies(BaseModel):
    session_id: str
    language: str = "en"
    theme: str = "light"

app = JustAPIApp()

@app.get("/check")
def check(cookies: Cookies = Cookie(...)):
    return cookies.model_dump()
```

## See Also

- [Header Parameters](/tutorials/header-params/) — reading HTTP headers
- [Query Parameters](/tutorials/query-params/) — URL query parameters
- [Security](/tutorials/security/first-steps/) — authentication with cookies
