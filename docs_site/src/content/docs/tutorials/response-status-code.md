---
title: Response Status Code
description: Set custom HTTP response status codes in JustAPI handlers.
keywords: [JustAPI, status code, HTTP status, response, error handling]
---

## Default Status Code

JustAPI returns `200 OK` by default for successful responses that don't use a
`Response` object.

## Explicit Status Code

The `status_code=` route decorator kwarg (e.g. `@app.post("/items", status_code=201)`)
is **OpenAPI documentation metadata only** — it documents the expected success
code in the generated schema but does **not** alter the actual HTTP status of a
plain dict/JSON return (which is still `200`).

To actually set the response status, return a `Response` object:

```python
from justapi import JustAPIApp, JSONResponse

app = JustAPIApp()

@app.post("/items")
def create_item():
    return JSONResponse(content={"message": "created"}, status_code=201)
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
from justapi import JSONResponse

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
