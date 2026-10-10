---
title: Response Status Code
description: Set custom HTTP response status codes in JustAPI handlers.
keywords: [JustAPI, status code, HTTP status, response, error handling]
---

## Default Status Code

JustAPI returns `200 OK` by default for successful responses that don't use a
`Response` object.

## Explicit Status Code

Set the success status directly on the decorator (FastAPI parity):

```python
@app.post("/items", status_code=201)
def create_item():
    return {"message": "created"}
```

The decorator's `status_code=` is applied to plain dict/list/scalar returns.
Explicit responses always win over it:

| Return value | Status sent |
|---|---|
| `@app.post(..., status_code=201)` + dict | `201` |
| `JSONResponse(..., status_code=207)` on a `status_code=202` route | `207` |
| `{"status": 204}` envelope on a `status_code=201` route | `204` |

A data dict that merely *contains* a `status` key (e.g.
`{"status": "ok", "n": 1}`) is not an envelope — it is serialized as JSON and
the decorator status applies.

To return a status dynamically (e.g. depending on the outcome), return a
`Response` object:

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
