---
title: Header Parameters
description: Read and validate HTTP header parameters in JustAPI.
keywords: [JustAPI, header parameters, HTTP headers, validation]
---

## Basic Header Parameters

```python
from justapi import JustAPIApp, Header

app = JustAPIApp()

@app.get("/items/")
def read_items(x_token: str = Header(...)):
    return {"x_token": x_token}
```

The header name `X-Token` is automatically converted to `x_token` (underscores).

## Multiple Headers

```python
@app.get("/users/")
def read_users(
    x_token: str = Header(...),
    x_request_id: str = Header(None),
):
    return {"x_token": x_token, "x_request_id": x_request_id}
```

## Required vs Optional

- `Header(...)` — required (the `...` means "no default, required")
- `Header(None)` — optional, defaults to `None`
- `Header("default")` — optional, defaults to `"default"`

## Underscore Conversion

By default, JustAPI converts dashes to underscores in header names. To disable this:

```python
@app.get("/items/")
def read_items(custom_header: str = Header(..., convert_underscores=False)):
    return {"custom_header": custom_header}
```

## Duplicate Headers

For headers that can repeat (like `Set-Cookie`):

```python
@app.get("/items/")
def read_items(x_token_list: list[str] = Header(...)):
    return {"x_token_list": x_token_list}
```

## See Also

- [Cookie Parameters](/tutorials/cookie-params/) — reading cookies
- [Request Body](/tutorials/request-body/) — JSON request bodies
