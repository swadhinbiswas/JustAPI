---
title: Response Status Code
description: Set custom HTTP response status codes in JustAPI handlers.
keywords: [JustAPI, status code, HTTP status, response, error handling]
---

## Default Status Code

JustAPI returns `200 OK` by default for successful responses.

## Custom Status Code

Use `status_code` in the decorator:

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.post("/items", status_code=201)
def create_item():
    return {"message": "created"}
```

## Common Status Codes

| Code | Meaning | Use When |
|------|---------|----------|
| 200 | OK | GET, successful PUT/PATCH |
| 201 | Created | Successful POST |
| 204 | No Content | Successful DELETE |
| 301 | Moved Permanently | Redirect |
| 400 | Bad Request | Client error |
| 401 | Unauthorized | Authentication required |
| 403 | Forbidden | Authenticated but not authorized |
| 404 | Not Found | Resource doesn't exist |
| 422 | Unprocessable Entity | Validation error |

## Returning a Response Directly

For full control, return a `Response` object:

```python
from starlette.responses import JSONResponse

@app.get("/custom")
def custom_response():
    return JSONResponse(
        content={"message": "custom"},
        status_code=202,
        headers={"X-Custom": "header"},
    )
```

## See Also

- [Handling Errors](/tutorials/error-handling/) — exception handlers
- [Additional Status Codes](/advanced/response-directly/) — advanced status code patterns
- [Status Codes Reference](/api-reference/routing/) — all available status codes
